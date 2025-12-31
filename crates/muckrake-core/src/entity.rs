use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Document,
    Event,
}

impl EntityType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organization => "organization",
            Self::Location => "location",
            Self::Document => "document",
            Self::Event => "event",
        }
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for EntityType {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "person" => Ok(Self::Person),
            "organization" => Ok(Self::Organization),
            "location" => Ok(Self::Location),
            "document" => Ok(Self::Document),
            "event" => Ok(Self::Event),
            _ => Err(crate::Error::InvalidEntityType(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_of_birth: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_of_death: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nationalities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationType {
    Corporation,
    Government,
    NonProfit,
    Political,
    Religious,
    Educational,
    Media,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationData {
    pub org_type: OrganizationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub founded_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dissolved_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationType {
    Address,
    City,
    Region,
    Country,
    Coordinates,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationData {
    pub location_type: LocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum EntityData {
    Person(PersonData),
    Organization(OrganizationData),
    Location(LocationData),
    Document(DocumentData),
    Event(EventData),
}

impl EntityData {
    #[must_use]
    pub fn entity_type(&self) -> EntityType {
        match self {
            Self::Person(_) => EntityType::Person,
            Self::Organization(_) => EntityType::Organization,
            Self::Location(_) => EntityType::Location,
            Self::Document(_) => EntityType::Document,
            Self::Event(_) => EntityType::Event,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Uuid,
    pub canonical_name: String,
    pub data: EntityData,
    pub confidence: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Entity {
    #[must_use]
    pub fn new(canonical_name: String, data: EntityData) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            canonical_name,
            data,
            confidence: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[must_use]
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = Some(confidence.clamp(0.0, 1.0));
        self
    }

    #[must_use]
    pub fn entity_type(&self) -> EntityType {
        self.data.entity_type()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityAlias {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub alias: String,
    pub source: Option<String>,
}

impl EntityAlias {
    #[must_use]
    pub fn new(entity_id: Uuid, alias: String, source: Option<String>) -> Self {
        Self {
            id: Uuid::now_v7(),
            entity_id,
            alias,
            source,
        }
    }
}
