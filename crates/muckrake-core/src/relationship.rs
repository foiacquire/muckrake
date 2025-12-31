use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    // Ownership/Control
    Owns,
    Controls,
    Shareholders,

    // Employment/Affiliation
    Employs,
    EmployedBy,
    DirectorOf,
    OfficerOf,
    MemberOf,

    // Family
    ParentOf,
    ChildOf,
    SpouseOf,
    SiblingOf,
    RelativeOf,

    // Location
    LocatedAt,
    HeadquarteredAt,
    RegisteredAt,

    // Events
    ParticipatedIn,
    OrganizedBy,

    // Documents
    MentionedIn,
    AuthoredBy,
    SignedBy,

    // Generic
    AssociatedWith,
    LinkedTo,
}

impl RelationType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owns => "owns",
            Self::Controls => "controls",
            Self::Shareholders => "shareholders",
            Self::Employs => "employs",
            Self::EmployedBy => "employed_by",
            Self::DirectorOf => "director_of",
            Self::OfficerOf => "officer_of",
            Self::MemberOf => "member_of",
            Self::ParentOf => "parent_of",
            Self::ChildOf => "child_of",
            Self::SpouseOf => "spouse_of",
            Self::SiblingOf => "sibling_of",
            Self::RelativeOf => "relative_of",
            Self::LocatedAt => "located_at",
            Self::HeadquarteredAt => "headquartered_at",
            Self::RegisteredAt => "registered_at",
            Self::ParticipatedIn => "participated_in",
            Self::OrganizedBy => "organized_by",
            Self::MentionedIn => "mentioned_in",
            Self::AuthoredBy => "authored_by",
            Self::SignedBy => "signed_by",
            Self::AssociatedWith => "associated_with",
            Self::LinkedTo => "linked_to",
        }
    }

    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        match self {
            Self::Owns => Some(Self::Shareholders),
            Self::Shareholders => Some(Self::Owns),
            Self::Employs => Some(Self::EmployedBy),
            Self::EmployedBy => Some(Self::Employs),
            Self::ParentOf => Some(Self::ChildOf),
            Self::ChildOf => Some(Self::ParentOf),
            Self::SpouseOf => Some(Self::SpouseOf),
            Self::SiblingOf => Some(Self::SiblingOf),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_symmetric(&self) -> bool {
        matches!(
            self,
            Self::SpouseOf
                | Self::SiblingOf
                | Self::RelativeOf
                | Self::AssociatedWith
                | Self::LinkedTo
        )
    }
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for RelationType {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owns" => Ok(Self::Owns),
            "controls" => Ok(Self::Controls),
            "shareholders" => Ok(Self::Shareholders),
            "employs" => Ok(Self::Employs),
            "employed_by" => Ok(Self::EmployedBy),
            "director_of" => Ok(Self::DirectorOf),
            "officer_of" => Ok(Self::OfficerOf),
            "member_of" => Ok(Self::MemberOf),
            "parent_of" => Ok(Self::ParentOf),
            "child_of" => Ok(Self::ChildOf),
            "spouse_of" => Ok(Self::SpouseOf),
            "sibling_of" => Ok(Self::SiblingOf),
            "relative_of" => Ok(Self::RelativeOf),
            "located_at" => Ok(Self::LocatedAt),
            "headquartered_at" => Ok(Self::HeadquarteredAt),
            "registered_at" => Ok(Self::RegisteredAt),
            "participated_in" => Ok(Self::ParticipatedIn),
            "organized_by" => Ok(Self::OrganizedBy),
            "mentioned_in" => Ok(Self::MentionedIn),
            "authored_by" => Ok(Self::AuthoredBy),
            "signed_by" => Ok(Self::SignedBy),
            "associated_with" => Ok(Self::AssociatedWith),
            "linked_to" => Ok(Self::LinkedTo),
            _ => Err(crate::Error::InvalidRelationshipType(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

impl Default for RelationshipData {
    fn default() -> Self {
        Self {
            start_date: None,
            end_date: None,
            role: None,
            notes: None,
            extra: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub id: Uuid,
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub relation_type: RelationType,
    pub confidence: Option<f64>,
    pub data: RelationshipData,
    pub created_at: DateTime<Utc>,
}

impl Relationship {
    pub fn new(
        source_id: Uuid,
        target_id: Uuid,
        relation_type: RelationType,
    ) -> crate::Result<Self> {
        if source_id == target_id {
            return Err(crate::Error::SelfReference);
        }

        Ok(Self {
            id: Uuid::now_v7(),
            source_id,
            target_id,
            relation_type,
            confidence: None,
            data: RelationshipData::default(),
            created_at: Utc::now(),
        })
    }

    #[must_use]
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = Some(confidence.clamp(0.0, 1.0));
        self
    }

    #[must_use]
    pub fn with_data(mut self, data: RelationshipData) -> Self {
        self.data = data;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: Uuid,
    pub entity_id: Option<Uuid>,
    pub relationship_id: Option<Uuid>,
    pub document_id: String,
    pub page_number: Option<u32>,
    pub text_span: Option<String>,
    pub context: Option<String>,
}

impl Evidence {
    #[must_use]
    pub fn for_entity(entity_id: Uuid, document_id: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            entity_id: Some(entity_id),
            relationship_id: None,
            document_id,
            page_number: None,
            text_span: None,
            context: None,
        }
    }

    #[must_use]
    pub fn for_relationship(relationship_id: Uuid, document_id: String) -> Self {
        Self {
            id: Uuid::now_v7(),
            entity_id: None,
            relationship_id: Some(relationship_id),
            document_id,
            page_number: None,
            text_span: None,
            context: None,
        }
    }

    #[must_use]
    pub fn with_page(mut self, page_number: u32) -> Self {
        self.page_number = Some(page_number);
        self
    }

    #[must_use]
    pub fn with_span(mut self, text_span: String) -> Self {
        self.text_span = Some(text_span);
        self
    }

    #[must_use]
    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
}
