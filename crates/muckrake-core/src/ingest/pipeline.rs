use std::path::Path;
use thiserror::Error;

use super::extractor::{CompositeExtractor, ExtractionOutput, Extractor};
use super::normalizer::{DefaultNormalizer, NormalizationOutput, Normalizer};
use super::parser::{CompositeParser, ParsedDocument, Parser};
use crate::entity::Entity;
use crate::source::{ImportLog, Source, SourceMetadata, SourceType};

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("Parse error: {0}")]
    Parse(#[from] super::parser::ParseError),
    #[error("Extraction error: {0}")]
    Extraction(#[from] super::extractor::ExtractionError),
    #[error("Normalization error: {0}")]
    Normalization(#[from] super::normalizer::NormalizationError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Source already ingested: {0}")]
    AlreadyIngested(String),
}

pub type IngestResult<T> = Result<T, IngestError>;

#[derive(Debug, Clone)]
pub struct IngestStats {
    pub new_entities: usize,
    pub matched_entities: usize,
    pub new_relationships: usize,
    pub sections_processed: usize,
    pub duration_ms: u64,
}

impl IngestStats {
    #[must_use]
    pub fn new() -> Self {
        Self {
            new_entities: 0,
            matched_entities: 0,
            new_relationships: 0,
            sections_processed: 0,
            duration_ms: 0,
        }
    }

    pub fn total_entities(&self) -> usize {
        self.new_entities + self.matched_entities
    }
}

impl Default for IngestStats {
    fn default() -> Self {
        Self::new()
    }
}

pub struct IngestOutput {
    pub source: Source,
    pub document: ParsedDocument,
    pub extraction: ExtractionOutput,
    pub normalization: NormalizationOutput,
    pub import_log: ImportLog,
    pub stats: IngestStats,
}

impl IngestOutput {
    #[must_use]
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.normalization.entities.iter().map(|r| &r.entity)
    }

    #[must_use]
    pub fn new_entities(&self) -> impl Iterator<Item = &Entity> {
        self.normalization
            .entities
            .iter()
            .filter(|r| r.is_new)
            .map(|r| &r.entity)
    }
}

pub struct IngestPipeline {
    parser: Box<dyn Parser>,
    extractor: Box<dyn Extractor>,
    normalizer: Box<dyn Normalizer>,
    existing_entities: Vec<Entity>,
    skip_duplicates: bool,
}

impl IngestPipeline {
    #[must_use]
    pub fn new() -> Self {
        Self {
            parser: Box::new(CompositeParser::default()),
            extractor: Box::new(CompositeExtractor::default()),
            normalizer: Box::new(DefaultNormalizer::default()),
            existing_entities: Vec::new(),
            skip_duplicates: true,
        }
    }

    pub fn with_parser(mut self, parser: Box<dyn Parser>) -> Self {
        self.parser = parser;
        self
    }

    pub fn with_extractor(mut self, extractor: Box<dyn Extractor>) -> Self {
        self.extractor = extractor;
        self
    }

    pub fn with_normalizer(mut self, normalizer: Box<dyn Normalizer>) -> Self {
        self.normalizer = normalizer;
        self
    }

    pub fn with_existing_entities(mut self, entities: Vec<Entity>) -> Self {
        self.existing_entities = entities;
        self
    }

    pub fn add_existing_entity(&mut self, entity: Entity) {
        self.existing_entities.push(entity);
    }

    pub fn set_skip_duplicates(&mut self, skip: bool) {
        self.skip_duplicates = skip;
    }

    pub async fn ingest_file(&mut self, path: &Path) -> IngestResult<IngestOutput> {
        let start = std::time::Instant::now();

        let document = self.parser.parse_file(path).await?;

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from);

        let uri = path.to_string_lossy().to_string();

        let source = Source::document(file_name.unwrap_or_default(), uri.clone())
            .with_metadata(SourceMetadata {
                page_count: document.page_count,
                mime_type: Some(format!("{:?}", document.format)),
                ..Default::default()
            });

        let content_hash = compute_hash(&document.full_text);

        self.ingest_document(document, source, content_hash, start)
            .await
    }

    pub async fn ingest_url(&mut self, url: &str, content: &str) -> IngestResult<IngestOutput> {
        let start = std::time::Instant::now();

        let document = ParsedDocument::new(
            super::parser::DocumentFormat::Html,
            content.to_string(),
        );

        let source = Source::url(url.to_string(), None);
        let content_hash = compute_hash(content);

        self.ingest_document(document, source, content_hash, start)
            .await
    }

    pub async fn ingest_text(
        &mut self,
        text: &str,
        title: Option<String>,
    ) -> IngestResult<IngestOutput> {
        let start = std::time::Instant::now();

        let document = ParsedDocument::new(
            super::parser::DocumentFormat::PlainText,
            text.to_string(),
        )
        .with_title(title.clone().unwrap_or_default());

        let source = Source::manual(title.unwrap_or_else(|| "Manual Entry".into()));
        let content_hash = compute_hash(text);

        self.ingest_document(document, source, content_hash, start)
            .await
    }

    async fn ingest_document(
        &mut self,
        document: ParsedDocument,
        source: Source,
        content_hash: String,
        start: std::time::Instant,
    ) -> IngestResult<IngestOutput> {
        let source = source.with_hash(content_hash.clone());

        let extraction = self.extractor.extract(&document).await?;

        let normalization = self
            .normalizer
            .normalize(extraction.clone(), &source, &self.existing_entities)
            .await?;

        for resolved in &normalization.entities {
            if resolved.is_new {
                self.existing_entities.push(resolved.entity.clone());
            }
        }

        let import_log = ImportLog::new(
            source.uri.clone().unwrap_or_default(),
            content_hash,
        )
        .with_counts(
            normalization.entities.len() as u32,
            normalization.relationships.len() as u32,
        );

        let stats = IngestStats {
            new_entities: normalization.new_entity_count(),
            matched_entities: normalization.entity_count() - normalization.new_entity_count(),
            new_relationships: normalization.relationship_count(),
            sections_processed: document.sections.len().max(1),
            duration_ms: start.elapsed().as_millis() as u64,
        };

        Ok(IngestOutput {
            source,
            document,
            extraction,
            normalization,
            import_log,
            stats,
        })
    }

    pub async fn ingest_bytes(
        &mut self,
        data: &[u8],
        format: super::parser::DocumentFormat,
        source_type: SourceType,
        title: Option<String>,
    ) -> IngestResult<IngestOutput> {
        let start = std::time::Instant::now();

        let document = self.parser.parse_bytes(data, format).await?;

        let source = Source::new(source_type);
        let source = if let Some(t) = title {
            Source {
                title: Some(t),
                ..source
            }
        } else {
            source
        };

        let content_hash = compute_hash(&document.full_text);

        self.ingest_document(document, source, content_hash, start)
            .await
    }
}

impl Default for IngestPipeline {
    fn default() -> Self {
        Self::new()
    }
}

fn compute_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub struct BatchIngestResult {
    pub successful: Vec<IngestOutput>,
    pub failed: Vec<(String, IngestError)>,
    pub total_stats: IngestStats,
}

impl BatchIngestResult {
    #[must_use]
    pub fn new() -> Self {
        Self {
            successful: Vec::new(),
            failed: Vec::new(),
            total_stats: IngestStats::new(),
        }
    }

    fn add_success(&mut self, output: IngestOutput) {
        self.total_stats.new_entities += output.stats.new_entities;
        self.total_stats.matched_entities += output.stats.matched_entities;
        self.total_stats.new_relationships += output.stats.new_relationships;
        self.total_stats.sections_processed += output.stats.sections_processed;
        self.total_stats.duration_ms += output.stats.duration_ms;
        self.successful.push(output);
    }

    fn add_failure(&mut self, path: String, error: IngestError) {
        self.failed.push((path, error));
    }

    pub fn success_count(&self) -> usize {
        self.successful.len()
    }

    pub fn failure_count(&self) -> usize {
        self.failed.len()
    }
}

impl Default for BatchIngestResult {
    fn default() -> Self {
        Self::new()
    }
}

impl IngestPipeline {
    pub async fn ingest_directory(&mut self, dir: &Path) -> IngestResult<BatchIngestResult> {
        let mut result = BatchIngestResult::new();

        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_file() {
                let path_str = path.to_string_lossy().to_string();

                match self.ingest_file(&path).await {
                    Ok(output) => result.add_success(output),
                    Err(e) => result.add_failure(path_str, e),
                }
            }
        }

        Ok(result)
    }

    pub async fn ingest_files(&mut self, paths: &[&Path]) -> BatchIngestResult {
        let mut result = BatchIngestResult::new();

        for path in paths {
            let path_str = path.to_string_lossy().to_string();

            match self.ingest_file(path).await {
                Ok(output) => result.add_success(output),
                Err(e) => result.add_failure(path_str, e),
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ingest_text() {
        let mut pipeline = IngestPipeline::new();

        let result = pipeline
            .ingest_text("Alice met Bob at the conference.", Some("Test".into()))
            .await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.source.source_type, SourceType::Manual);
    }

    #[test]
    fn test_ingest_stats() {
        let mut stats = IngestStats::new();
        stats.new_entities = 5;
        stats.matched_entities = 3;

        assert_eq!(stats.total_entities(), 8);
    }

    #[test]
    fn test_compute_hash() {
        let hash1 = compute_hash("hello");
        let hash2 = compute_hash("hello");
        let hash3 = compute_hash("world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 16);
    }

    #[tokio::test]
    async fn test_batch_result() {
        let mut result = BatchIngestResult::new();

        let mut pipeline = IngestPipeline::new();
        let output = pipeline.ingest_text("Test content", None).await.unwrap();

        result.add_success(output);

        assert_eq!(result.success_count(), 1);
        assert_eq!(result.failure_count(), 0);
    }
}
