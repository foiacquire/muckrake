use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

use super::extractor::{ExtractionOutput, ExtractedEntity};
use crate::entity::{Entity, EntityAlias, EntityData};
use crate::relationship::Relationship;
use crate::source::Source;

#[derive(Debug, Error)]
pub enum NormalizationError {
    #[error("Entity resolution failed: {0}")]
    ResolutionFailed(String),
    #[error("Invalid entity data: {0}")]
    InvalidEntityData(String),
    #[error("Relationship creation failed: {0}")]
    RelationshipFailed(String),
}

pub type NormalizationResult<T> = Result<T, NormalizationError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedEntity {
    pub entity: Entity,
    pub aliases: Vec<EntityAlias>,
    pub is_new: bool,
    pub merged_from: Vec<Uuid>,
}

impl ResolvedEntity {
    #[must_use]
    pub fn new_entity(entity: Entity) -> Self {
        Self {
            entity,
            aliases: Vec::new(),
            is_new: true,
            merged_from: Vec::new(),
        }
    }

    #[must_use]
    pub fn existing(entity: Entity) -> Self {
        Self {
            entity,
            aliases: Vec::new(),
            is_new: false,
            merged_from: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_alias(mut self, alias: EntityAlias) -> Self {
        self.aliases.push(alias);
        self
    }

    pub fn add_merged(&mut self, temp_id: Uuid) {
        self.merged_from.push(temp_id);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NormalizationOutput {
    pub entities: Vec<ResolvedEntity>,
    pub relationships: Vec<Relationship>,
    pub temp_id_mapping: HashMap<Uuid, Uuid>,
}

impl NormalizationOutput {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn new_entity_count(&self) -> usize {
        self.entities.iter().filter(|e| e.is_new).count()
    }

    pub fn relationship_count(&self) -> usize {
        self.relationships.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionStrategy {
    ExactMatch,
    FuzzyMatch,
    SemanticSimilarity,
}

pub trait EntityResolver: Send + Sync {
    fn strategy(&self) -> ResolutionStrategy;

    fn find_match(&self, extracted: &ExtractedEntity, existing: &[Entity]) -> Option<Uuid>;

    fn similarity_score(&self, extracted: &ExtractedEntity, existing: &Entity) -> f64;
}

pub struct ExactMatchResolver {
    case_sensitive: bool,
}

impl ExactMatchResolver {
    #[must_use]
    pub fn new(case_sensitive: bool) -> Self {
        Self { case_sensitive }
    }
}

impl Default for ExactMatchResolver {
    fn default() -> Self {
        Self::new(false)
    }
}

impl EntityResolver for ExactMatchResolver {
    fn strategy(&self) -> ResolutionStrategy {
        ResolutionStrategy::ExactMatch
    }

    fn find_match(&self, extracted: &ExtractedEntity, existing: &[Entity]) -> Option<Uuid> {
        let name = if self.case_sensitive {
            extracted.name.clone()
        } else {
            extracted.name.to_lowercase()
        };

        existing.iter().find(|e| {
            let existing_name = if self.case_sensitive {
                e.canonical_name.clone()
            } else {
                e.canonical_name.to_lowercase()
            };
            existing_name == name
        }).map(|e| e.id)
    }

    fn similarity_score(&self, extracted: &ExtractedEntity, existing: &Entity) -> f64 {
        let (a, b) = if self.case_sensitive {
            (extracted.name.clone(), existing.canonical_name.clone())
        } else {
            (extracted.name.to_lowercase(), existing.canonical_name.to_lowercase())
        };

        if a == b { 1.0 } else { 0.0 }
    }
}

pub struct FuzzyMatchResolver {
    threshold: f64,
}

impl FuzzyMatchResolver {
    #[must_use]
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }

    fn levenshtein_distance(a: &str, b: &str) -> usize {
        let a_len = a.chars().count();
        let b_len = b.chars().count();

        if a_len == 0 {
            return b_len;
        }
        if b_len == 0 {
            return a_len;
        }

        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();

        let mut prev_row: Vec<usize> = (0..=b_len).collect();
        let mut curr_row = vec![0; b_len + 1];

        for i in 1..=a_len {
            curr_row[0] = i;

            for j in 1..=b_len {
                let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
                curr_row[j] = (prev_row[j] + 1)
                    .min(curr_row[j - 1] + 1)
                    .min(prev_row[j - 1] + cost);
            }

            std::mem::swap(&mut prev_row, &mut curr_row);
        }

        prev_row[b_len]
    }

    fn normalized_similarity(a: &str, b: &str) -> f64 {
        let distance = Self::levenshtein_distance(a, b);
        let max_len = a.chars().count().max(b.chars().count());
        if max_len == 0 {
            return 1.0;
        }
        1.0 - (distance as f64 / max_len as f64)
    }
}

impl Default for FuzzyMatchResolver {
    fn default() -> Self {
        Self::new(0.8)
    }
}

impl EntityResolver for FuzzyMatchResolver {
    fn strategy(&self) -> ResolutionStrategy {
        ResolutionStrategy::FuzzyMatch
    }

    fn find_match(&self, extracted: &ExtractedEntity, existing: &[Entity]) -> Option<Uuid> {
        let mut best_match: Option<(Uuid, f64)> = None;

        for entity in existing {
            let score = self.similarity_score(extracted, entity);
            if score >= self.threshold {
                if let Some((_, best_score)) = best_match {
                    if score > best_score {
                        best_match = Some((entity.id, score));
                    }
                } else {
                    best_match = Some((entity.id, score));
                }
            }
        }

        best_match.map(|(id, _)| id)
    }

    fn similarity_score(&self, extracted: &ExtractedEntity, existing: &Entity) -> f64 {
        let name_score = Self::normalized_similarity(
            &extracted.name.to_lowercase(),
            &existing.canonical_name.to_lowercase(),
        );

        if extracted.entity_type != existing.data.entity_type() {
            name_score * 0.5
        } else {
            name_score
        }
    }
}

#[async_trait::async_trait]
pub trait Normalizer: Send + Sync {
    async fn normalize(
        &self,
        extraction: ExtractionOutput,
        source: &Source,
        existing_entities: &[Entity],
    ) -> NormalizationResult<NormalizationOutput>;
}

pub struct DefaultNormalizer {
    resolver: Box<dyn EntityResolver>,
    merge_threshold: f64,
}

impl DefaultNormalizer {
    #[must_use]
    pub fn new(resolver: Box<dyn EntityResolver>) -> Self {
        Self {
            resolver,
            merge_threshold: 0.9,
        }
    }

    #[must_use]
    pub fn with_merge_threshold(mut self, threshold: f64) -> Self {
        self.merge_threshold = threshold;
        self
    }

    fn create_entity_data(extracted: &ExtractedEntity) -> EntityData {
        match extracted.entity_type {
            crate::entity::EntityType::Person => EntityData::Person(crate::entity::PersonData {
                date_of_birth: None,
                date_of_death: None,
                nationalities: Vec::new(),
                roles: Vec::new(),
            }),
            crate::entity::EntityType::Organization => {
                EntityData::Organization(crate::entity::OrganizationData {
                    org_type: crate::entity::OrganizationType::Other,
                    jurisdiction: None,
                    registration_number: None,
                    founded_date: None,
                    dissolved_date: None,
                })
            }
            crate::entity::EntityType::Location => {
                EntityData::Location(crate::entity::LocationData {
                    location_type: crate::entity::LocationType::Address,
                    address: None,
                    city: None,
                    region: None,
                    country: None,
                    latitude: None,
                    longitude: None,
                })
            }
            crate::entity::EntityType::Document => {
                EntityData::Document(crate::entity::DocumentData {
                    source_url: None,
                    source_id: None,
                    mime_type: None,
                    page_count: None,
                    published_date: None,
                })
            }
            crate::entity::EntityType::Event => EntityData::Event(crate::entity::EventData {
                start_date: None,
                end_date: None,
                location_id: None,
                description: None,
            }),
        }
    }

    fn deduplicate_extracted(entities: &[ExtractedEntity]) -> Vec<Vec<usize>> {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut assigned = vec![false; entities.len()];

        for i in 0..entities.len() {
            if assigned[i] {
                continue;
            }

            let mut group = vec![i];
            assigned[i] = true;

            for j in (i + 1)..entities.len() {
                if assigned[j] {
                    continue;
                }

                let name_i = entities[i].name.to_lowercase();
                let name_j = entities[j].name.to_lowercase();

                if name_i == name_j && entities[i].entity_type == entities[j].entity_type {
                    group.push(j);
                    assigned[j] = true;
                }
            }

            groups.push(group);
        }

        groups
    }
}

impl Default for DefaultNormalizer {
    fn default() -> Self {
        Self::new(Box::new(FuzzyMatchResolver::default()))
    }
}

#[async_trait::async_trait]
impl Normalizer for DefaultNormalizer {
    async fn normalize(
        &self,
        extraction: ExtractionOutput,
        _source: &Source,
        existing_entities: &[Entity],
    ) -> NormalizationResult<NormalizationOutput> {
        let mut output = NormalizationOutput::new();

        let entity_groups = Self::deduplicate_extracted(&extraction.entities);

        for group in entity_groups {
            let primary_idx = group[0];
            let primary = &extraction.entities[primary_idx];

            let mut resolved = if let Some(existing_id) =
                self.resolver.find_match(primary, existing_entities)
            {
                let existing = existing_entities
                    .iter()
                    .find(|e| e.id == existing_id)
                    .cloned()
                    .ok_or_else(|| {
                        NormalizationError::ResolutionFailed("Matched entity not found".into())
                    })?;

                output.temp_id_mapping.insert(primary.temp_id, existing_id);

                for &idx in &group[1..] {
                    output.temp_id_mapping.insert(extraction.entities[idx].temp_id, existing_id);
                }

                ResolvedEntity::existing(existing)
            } else {
                let entity_data = Self::create_entity_data(primary);
                let entity = Entity::new(primary.name.clone(), entity_data)
                    .with_confidence(primary.confidence);

                output.temp_id_mapping.insert(primary.temp_id, entity.id);

                for &idx in &group[1..] {
                    output.temp_id_mapping.insert(extraction.entities[idx].temp_id, entity.id);
                }

                ResolvedEntity::new_entity(entity)
            };

            for &idx in &group[1..] {
                let extracted = &extraction.entities[idx];
                resolved.add_merged(extracted.temp_id);

                if extracted.name.to_lowercase() != resolved.entity.canonical_name.to_lowercase() {
                    let alias = EntityAlias::new(resolved.entity.id, extracted.name.clone(), None);
                    resolved.aliases.push(alias);
                }
            }

            for extracted in group.iter().flat_map(|&i| extraction.entities[i].aliases.iter()) {
                if extracted.to_lowercase() != resolved.entity.canonical_name.to_lowercase() {
                    let alias = EntityAlias::new(resolved.entity.id, extracted.clone(), None);
                    resolved.aliases.push(alias);
                }
            }

            output.entities.push(resolved);
        }

        for extracted_rel in &extraction.relationships {
            let Some(&source_id) = output.temp_id_mapping.get(&extracted_rel.source_temp_id) else {
                continue;
            };
            let Some(&target_id) = output.temp_id_mapping.get(&extracted_rel.target_temp_id) else {
                continue;
            };

            match Relationship::new(source_id, target_id, extracted_rel.relation_type) {
                Ok(rel) => {
                    let rel = rel.with_confidence(extracted_rel.confidence);
                    output.relationships.push(rel);
                }
                Err(e) => {
                    tracing::warn!("Skipping invalid relationship: {}", e);
                }
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityType;
    use crate::source::SourceType;

    #[test]
    fn test_exact_match_resolver() {
        let resolver = ExactMatchResolver::new(false);

        let extracted = super::super::extractor::ExtractedEntity::new(
            EntityType::Person,
            "John Doe".into(),
            0.9,
        );

        let existing_data = EntityData::Person(crate::entity::PersonData {
            date_of_birth: None,
            date_of_death: None,
            nationalities: vec![],
            roles: vec![],
        });
        let existing = Entity::new("john doe".into(), existing_data);
        let existing_entities = vec![existing.clone()];

        let matched = resolver.find_match(&extracted, &existing_entities);

        assert_eq!(matched, Some(existing.id));
    }

    #[test]
    fn test_fuzzy_match_resolver() {
        let resolver = FuzzyMatchResolver::new(0.7);

        let extracted = super::super::extractor::ExtractedEntity::new(
            EntityType::Person,
            "Jon Doe".into(),
            0.9,
        );

        let existing_data = EntityData::Person(crate::entity::PersonData {
            date_of_birth: None,
            date_of_death: None,
            nationalities: vec![],
            roles: vec![],
        });
        let existing = Entity::new("John Doe".into(), existing_data);
        let existing_entities = vec![existing.clone()];

        let matched = resolver.find_match(&extracted, &existing_entities);

        assert_eq!(matched, Some(existing.id));
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(FuzzyMatchResolver::levenshtein_distance("", ""), 0);
        assert_eq!(FuzzyMatchResolver::levenshtein_distance("abc", ""), 3);
        assert_eq!(FuzzyMatchResolver::levenshtein_distance("", "abc"), 3);
        assert_eq!(FuzzyMatchResolver::levenshtein_distance("abc", "abc"), 0);
        assert_eq!(FuzzyMatchResolver::levenshtein_distance("abc", "abd"), 1);
        assert_eq!(FuzzyMatchResolver::levenshtein_distance("kitten", "sitting"), 3);
    }

    #[tokio::test]
    async fn test_default_normalizer() {
        let normalizer = DefaultNormalizer::default();

        let mut extraction = super::super::extractor::ExtractionOutput::new();
        extraction.entities.push(super::super::extractor::ExtractedEntity::new(
            EntityType::Person,
            "Alice".into(),
            0.9,
        ));

        let source = Source::new(SourceType::Document);

        let output = normalizer.normalize(extraction, &source, &[]).await.unwrap();

        assert_eq!(output.entities.len(), 1);
        assert!(output.entities[0].is_new);
        assert_eq!(output.entities[0].entity.canonical_name, "Alice");
    }
}
