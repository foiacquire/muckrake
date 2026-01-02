use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Document,
    Url,
    Manual,
    Api,
    Interview,
    Email,
    Archive,
}

impl SourceType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Url => "url",
            Self::Manual => "manual",
            Self::Api => "api",
            Self::Interview => "interview",
            Self::Email => "email",
            Self::Archive => "archive",
        }
    }
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SourceType {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "document" => Ok(Self::Document),
            "url" => Ok(Self::Url),
            "manual" => Ok(Self::Manual),
            "api" => Ok(Self::Api),
            "interview" => Ok(Self::Interview),
            "email" => Ok(Self::Email),
            "archive" => Ok(Self::Archive),
            _ => Err(crate::Error::InvalidSourceType(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

impl Default for SourceMetadata {
    fn default() -> Self {
        Self {
            author: None,
            published_date: None,
            mime_type: None,
            page_count: None,
            duration_seconds: None,
            extra: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: Uuid,
    pub source_type: SourceType,
    pub title: Option<String>,
    pub uri: Option<String>,
    pub content_hash: Option<String>,
    pub metadata: SourceMetadata,
    pub created_at: DateTime<Utc>,
}

impl Source {
    #[must_use]
    pub fn new(source_type: SourceType) -> Self {
        Self {
            id: Uuid::now_v7(),
            source_type,
            title: None,
            uri: None,
            content_hash: None,
            metadata: SourceMetadata::default(),
            created_at: Utc::now(),
        }
    }

    #[must_use]
    pub fn document(title: String, uri: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            source_type: SourceType::Document,
            title: Some(title),
            uri: Some(uri),
            content_hash: None,
            metadata: SourceMetadata::default(),
            created_at: Utc::now(),
        }
    }

    #[must_use]
    pub fn url(uri: String, title: Option<String>) -> Self {
        Self {
            id: Uuid::now_v7(),
            source_type: SourceType::Url,
            title,
            uri: Some(uri),
            content_hash: None,
            metadata: SourceMetadata::default(),
            created_at: Utc::now(),
        }
    }

    #[must_use]
    pub fn manual(title: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            source_type: SourceType::Manual,
            title: Some(title),
            uri: None,
            content_hash: None,
            metadata: SourceMetadata::default(),
            created_at: Utc::now(),
        }
    }

    #[must_use]
    pub fn with_hash(mut self, hash: String) -> Self {
        self.content_hash = Some(hash);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: SourceMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportLog {
    pub id: Uuid,
    pub source_uri: String,
    pub content_hash: String,
    pub imported_at: DateTime<Utc>,
    pub entity_count: u32,
    pub relationship_count: u32,
}

impl ImportLog {
    #[must_use]
    pub fn new(source_uri: String, content_hash: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            source_uri,
            content_hash,
            imported_at: Utc::now(),
            entity_count: 0,
            relationship_count: 0,
        }
    }

    #[must_use]
    pub fn with_counts(mut self, entities: u32, relationships: u32) -> Self {
        self.entity_count = entities;
        self.relationship_count = relationships;
        self
    }
}
