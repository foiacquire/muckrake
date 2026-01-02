use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::entity::EntityType;
use crate::relationship::RelationType;
use super::parser::ParsedDocument;

#[derive(Debug, Error)]
pub enum ExtractionError {
    #[error("Extraction failed: {0}")]
    Failed(String),
    #[error("Model unavailable: {0}")]
    ModelUnavailable(String),
    #[error("Rate limited")]
    RateLimited,
    #[error("Context too long: {0} tokens (max: {1})")]
    ContextTooLong(usize, usize),
}

pub type ExtractionResult<T> = Result<T, ExtractionError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSpan {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub page: Option<u32>,
}

impl TextSpan {
    #[must_use]
    pub fn new(text: String, start: usize, end: usize) -> Self {
        Self {
            text,
            start,
            end,
            page: None,
        }
    }

    #[must_use]
    pub fn with_page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub temp_id: Uuid,
    pub entity_type: EntityType,
    pub name: String,
    pub aliases: Vec<String>,
    pub mentions: Vec<TextSpan>,
    pub confidence: f64,
    pub attributes: serde_json::Value,
}

impl ExtractedEntity {
    #[must_use]
    pub fn new(entity_type: EntityType, name: String, confidence: f64) -> Self {
        Self {
            temp_id: Uuid::now_v7(),
            entity_type,
            name,
            aliases: Vec::new(),
            mentions: Vec::new(),
            confidence,
            attributes: serde_json::Value::Null,
        }
    }

    #[must_use]
    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        self.aliases = aliases;
        self
    }

    #[must_use]
    pub fn with_mention(mut self, span: TextSpan) -> Self {
        self.mentions.push(span);
        self
    }

    #[must_use]
    pub fn with_mentions(mut self, mentions: Vec<TextSpan>) -> Self {
        self.mentions = mentions;
        self
    }

    #[must_use]
    pub fn with_attributes(mut self, attrs: serde_json::Value) -> Self {
        self.attributes = attrs;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedRelationship {
    pub source_temp_id: Uuid,
    pub target_temp_id: Uuid,
    pub relation_type: RelationType,
    pub evidence: Vec<TextSpan>,
    pub confidence: f64,
    pub attributes: serde_json::Value,
}

impl ExtractedRelationship {
    #[must_use]
    pub fn new(
        source_temp_id: Uuid,
        target_temp_id: Uuid,
        relation_type: RelationType,
        confidence: f64,
    ) -> Self {
        Self {
            source_temp_id,
            target_temp_id,
            relation_type,
            evidence: Vec::new(),
            confidence,
            attributes: serde_json::Value::Null,
        }
    }

    #[must_use]
    pub fn with_evidence(mut self, span: TextSpan) -> Self {
        self.evidence.push(span);
        self
    }

    #[must_use]
    pub fn with_attributes(mut self, attrs: serde_json::Value) -> Self {
        self.attributes = attrs;
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionOutput {
    pub entities: Vec<ExtractedEntity>,
    pub relationships: Vec<ExtractedRelationship>,
}

impl ExtractionOutput {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_entity(mut self, entity: ExtractedEntity) -> Self {
        self.entities.push(entity);
        self
    }

    #[must_use]
    pub fn with_relationship(mut self, rel: ExtractedRelationship) -> Self {
        self.relationships.push(rel);
        self
    }

    pub fn merge(&mut self, other: ExtractionOutput) {
        self.entities.extend(other.entities);
        self.relationships.extend(other.relationships);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionStrategy {
    RuleBased,
    NerModel,
    LlmBased,
    Hybrid,
}

#[async_trait::async_trait]
pub trait Extractor: Send + Sync {
    fn strategy(&self) -> ExtractionStrategy;

    async fn extract(&self, document: &ParsedDocument) -> ExtractionResult<ExtractionOutput>;

    async fn extract_text(&self, text: &str) -> ExtractionResult<ExtractionOutput> {
        let doc = ParsedDocument::new(
            super::parser::DocumentFormat::PlainText,
            text.to_string(),
        );
        self.extract(&doc).await
    }
}

pub struct RuleBasedExtractor {
    patterns: Vec<ExtractionPattern>,
}

pub struct ExtractionPattern {
    pub entity_type: EntityType,
    pub regex: regex::Regex,
    pub confidence: f64,
}

impl ExtractionPattern {
    pub fn new(entity_type: EntityType, pattern: &str, confidence: f64) -> Result<Self, regex::Error> {
        Ok(Self {
            entity_type,
            regex: regex::Regex::new(pattern)?,
            confidence,
        })
    }
}

impl RuleBasedExtractor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    pub fn with_pattern(mut self, pattern: ExtractionPattern) -> Self {
        self.patterns.push(pattern);
        self
    }

    #[must_use]
    pub fn with_default_patterns() -> Self {
        let mut extractor = Self::new();

        let email_pattern = ExtractionPattern::new(
            EntityType::Person,
            r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}",
            0.6,
        );

        let url_pattern = ExtractionPattern::new(
            EntityType::Organization,
            r"https?://(?:www\.)?([a-zA-Z0-9-]+)\.(?:com|org|net|gov|edu)(?:/[^\s]*)?",
            0.5,
        );

        if let Ok(p) = email_pattern {
            extractor.patterns.push(p);
        }
        if let Ok(p) = url_pattern {
            extractor.patterns.push(p);
        }

        extractor
    }
}

impl Default for RuleBasedExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Extractor for RuleBasedExtractor {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::RuleBased
    }

    async fn extract(&self, document: &ParsedDocument) -> ExtractionResult<ExtractionOutput> {
        let mut output = ExtractionOutput::new();
        let text = &document.full_text;

        for pattern in &self.patterns {
            for capture in pattern.regex.find_iter(text) {
                let name = capture.as_str().to_string();
                let span = TextSpan::new(name.clone(), capture.start(), capture.end());

                let entity = ExtractedEntity::new(pattern.entity_type, name, pattern.confidence)
                    .with_mention(span);

                output.entities.push(entity);
            }
        }

        Ok(output)
    }
}

pub struct CompositeExtractor {
    extractors: Vec<Box<dyn Extractor>>,
}

impl CompositeExtractor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            extractors: Vec::new(),
        }
    }

    pub fn with_extractor(mut self, extractor: Box<dyn Extractor>) -> Self {
        self.extractors.push(extractor);
        self
    }

    pub fn add_extractor(&mut self, extractor: Box<dyn Extractor>) {
        self.extractors.push(extractor);
    }
}

impl Default for CompositeExtractor {
    fn default() -> Self {
        Self::new().with_extractor(Box::new(RuleBasedExtractor::with_default_patterns()))
    }
}

#[async_trait::async_trait]
impl Extractor for CompositeExtractor {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::Hybrid
    }

    async fn extract(&self, document: &ParsedDocument) -> ExtractionResult<ExtractionOutput> {
        let mut combined = ExtractionOutput::new();

        for extractor in &self.extractors {
            let result = extractor.extract(document).await?;
            combined.merge(result);
        }

        Ok(combined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rule_based_extraction() {
        let extractor = RuleBasedExtractor::with_default_patterns();
        let doc = ParsedDocument::new(
            super::super::parser::DocumentFormat::PlainText,
            "Contact us at info@example.com or visit https://acme.org for more info.".into(),
        );

        let output = extractor.extract(&doc).await.unwrap();

        assert!(!output.entities.is_empty());
    }

    #[test]
    fn test_extraction_output_merge() {
        let mut output1 = ExtractionOutput::new()
            .with_entity(ExtractedEntity::new(EntityType::Person, "Alice".into(), 0.9));

        let output2 = ExtractionOutput::new()
            .with_entity(ExtractedEntity::new(EntityType::Person, "Bob".into(), 0.8));

        output1.merge(output2);

        assert_eq!(output1.entities.len(), 2);
    }

    #[test]
    fn test_text_span() {
        let span = TextSpan::new("test".into(), 0, 4).with_page(1);
        assert_eq!(span.page, Some(1));
    }
}
