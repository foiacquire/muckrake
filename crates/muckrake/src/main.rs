use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use iced::widget::{
    button, canvas, column, container, horizontal_rule, horizontal_space, pick_list, row,
    scrollable, text, text_input, vertical_rule, Column,
};
use iced::{mouse, Element, Fill, Length, Point, Rectangle, Renderer, Size, Task, Theme};
use iced::event;
use iced::widget::canvas::{Cache, Event, Geometry, Path, Program, Stroke};
use muckrake_core::{
    Entity, EntityData, EntityType, EventData, DocumentData, LocationData, LocationType,
    OrganizationData, OrganizationType, PersonData, Relationship, RelationType, Storage,
};
use tokio::sync::RwLock;
use uuid::Uuid;

// Catppuccin Mocha colors
#[allow(dead_code)]
mod colors {
    use iced::Color;

    pub const BASE: Color = Color::from_rgb(0.118, 0.118, 0.180);
    pub const MANTLE: Color = Color::from_rgb(0.094, 0.094, 0.145);
    pub const CRUST: Color = Color::from_rgb(0.067, 0.067, 0.106);
    pub const SURFACE0: Color = Color::from_rgb(0.192, 0.196, 0.267);
    pub const SURFACE1: Color = Color::from_rgb(0.271, 0.278, 0.353);
    pub const SURFACE2: Color = Color::from_rgb(0.345, 0.357, 0.439);
    pub const TEXT: Color = Color::from_rgb(0.804, 0.839, 0.957);
    pub const SUBTEXT: Color = Color::from_rgb(0.651, 0.678, 0.784);
    pub const BLUE: Color = Color::from_rgb(0.537, 0.706, 0.980);
    pub const MAUVE: Color = Color::from_rgb(0.796, 0.651, 0.969);
    pub const GREEN: Color = Color::from_rgb(0.651, 0.890, 0.631);
    pub const PEACH: Color = Color::from_rgb(0.980, 0.702, 0.529);
    pub const YELLOW: Color = Color::from_rgb(0.976, 0.886, 0.686);
    pub const RED: Color = Color::from_rgb(0.953, 0.545, 0.659);
}

fn main() -> iced::Result {
    iced::application("Muckrake", App::update, App::view)
        .theme(|_| Theme::CatppuccinMocha)
        .window_size((1200.0, 800.0))
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenMenu {
    File,
    Edit,
    View,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntityTypeOption {
    Person,
    Organization,
    Location,
    Document,
    Event,
}

impl EntityTypeOption {
    const ALL: [EntityTypeOption; 5] = [
        EntityTypeOption::Person,
        EntityTypeOption::Organization,
        EntityTypeOption::Location,
        EntityTypeOption::Document,
        EntityTypeOption::Event,
    ];
}

impl std::fmt::Display for EntityTypeOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityTypeOption::Person => write!(f, "Person"),
            EntityTypeOption::Organization => write!(f, "Organization"),
            EntityTypeOption::Location => write!(f, "Location"),
            EntityTypeOption::Document => write!(f, "Document"),
            EntityTypeOption::Event => write!(f, "Event"),
        }
    }
}

impl From<EntityTypeOption> for EntityType {
    fn from(opt: EntityTypeOption) -> Self {
        match opt {
            EntityTypeOption::Person => EntityType::Person,
            EntityTypeOption::Organization => EntityType::Organization,
            EntityTypeOption::Location => EntityType::Location,
            EntityTypeOption::Document => EntityType::Document,
            EntityTypeOption::Event => EntityType::Event,
        }
    }
}

impl From<EntityType> for EntityTypeOption {
    fn from(et: EntityType) -> Self {
        match et {
            EntityType::Person => EntityTypeOption::Person,
            EntityType::Organization => EntityTypeOption::Organization,
            EntityType::Location => EntityTypeOption::Location,
            EntityType::Document => EntityTypeOption::Document,
            EntityType::Event => EntityTypeOption::Event,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelationTypeOption {
    Owns,
    Controls,
    Employs,
    EmployedBy,
    DirectorOf,
    MemberOf,
    ParentOf,
    ChildOf,
    SpouseOf,
    LocatedAt,
    AssociatedWith,
}

impl RelationTypeOption {
    const ALL: [RelationTypeOption; 11] = [
        RelationTypeOption::AssociatedWith,
        RelationTypeOption::Owns,
        RelationTypeOption::Controls,
        RelationTypeOption::Employs,
        RelationTypeOption::EmployedBy,
        RelationTypeOption::DirectorOf,
        RelationTypeOption::MemberOf,
        RelationTypeOption::ParentOf,
        RelationTypeOption::ChildOf,
        RelationTypeOption::SpouseOf,
        RelationTypeOption::LocatedAt,
    ];
}

impl std::fmt::Display for RelationTypeOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationTypeOption::Owns => write!(f, "Owns"),
            RelationTypeOption::Controls => write!(f, "Controls"),
            RelationTypeOption::Employs => write!(f, "Employs"),
            RelationTypeOption::EmployedBy => write!(f, "Employed By"),
            RelationTypeOption::DirectorOf => write!(f, "Director Of"),
            RelationTypeOption::MemberOf => write!(f, "Member Of"),
            RelationTypeOption::ParentOf => write!(f, "Parent Of"),
            RelationTypeOption::ChildOf => write!(f, "Child Of"),
            RelationTypeOption::SpouseOf => write!(f, "Spouse Of"),
            RelationTypeOption::LocatedAt => write!(f, "Located At"),
            RelationTypeOption::AssociatedWith => write!(f, "Associated With"),
        }
    }
}

impl From<RelationTypeOption> for RelationType {
    fn from(opt: RelationTypeOption) -> Self {
        match opt {
            RelationTypeOption::Owns => RelationType::Owns,
            RelationTypeOption::Controls => RelationType::Controls,
            RelationTypeOption::Employs => RelationType::Employs,
            RelationTypeOption::EmployedBy => RelationType::EmployedBy,
            RelationTypeOption::DirectorOf => RelationType::DirectorOf,
            RelationTypeOption::MemberOf => RelationType::MemberOf,
            RelationTypeOption::ParentOf => RelationType::ParentOf,
            RelationTypeOption::ChildOf => RelationType::ChildOf,
            RelationTypeOption::SpouseOf => RelationType::SpouseOf,
            RelationTypeOption::LocatedAt => RelationType::LocatedAt,
            RelationTypeOption::AssociatedWith => RelationType::AssociatedWith,
        }
    }
}

impl From<RelationType> for RelationTypeOption {
    fn from(rt: RelationType) -> Self {
        match rt {
            RelationType::Owns => RelationTypeOption::Owns,
            RelationType::Controls => RelationTypeOption::Controls,
            RelationType::Employs => RelationTypeOption::Employs,
            RelationType::EmployedBy => RelationTypeOption::EmployedBy,
            RelationType::DirectorOf => RelationTypeOption::DirectorOf,
            RelationType::MemberOf => RelationTypeOption::MemberOf,
            RelationType::ParentOf => RelationTypeOption::ParentOf,
            RelationType::ChildOf => RelationTypeOption::ChildOf,
            RelationType::SpouseOf => RelationTypeOption::SpouseOf,
            RelationType::LocatedAt => RelationTypeOption::LocatedAt,
            RelationType::AssociatedWith => RelationTypeOption::AssociatedWith,
            _ => RelationTypeOption::AssociatedWith,
        }
    }
}

#[derive(Debug, Clone)]
struct DisplayEntity {
    id: Uuid,
    name: String,
    entity_type: EntityTypeOption,
    x: f32,
    y: f32,
    persisted: bool,
}

impl DisplayEntity {
    fn to_entity(&self) -> Entity {
        let data = default_entity_data(self.entity_type);
        let now = Utc::now();
        Entity {
            id: self.id,
            canonical_name: self.name.clone(),
            data,
            confidence: None,
            created_at: now,
            updated_at: now,
        }
    }
}

fn default_entity_data(entity_type: EntityTypeOption) -> EntityData {
    match entity_type {
        EntityTypeOption::Person => EntityData::Person(PersonData {
            date_of_birth: None,
            date_of_death: None,
            nationalities: Vec::new(),
            roles: Vec::new(),
        }),
        EntityTypeOption::Organization => EntityData::Organization(OrganizationData {
            org_type: OrganizationType::Other,
            jurisdiction: None,
            registration_number: None,
            founded_date: None,
            dissolved_date: None,
        }),
        EntityTypeOption::Location => EntityData::Location(LocationData {
            location_type: LocationType::City,
            address: None,
            city: None,
            region: None,
            country: None,
            latitude: None,
            longitude: None,
        }),
        EntityTypeOption::Document => EntityData::Document(DocumentData {
            source_url: None,
            source_id: None,
            mime_type: None,
            page_count: None,
            published_date: None,
        }),
        EntityTypeOption::Event => EntityData::Event(EventData {
            start_date: None,
            end_date: None,
            location_id: None,
            description: None,
        }),
    }
}

#[derive(Debug, Clone)]
struct DisplayRelationship {
    id: Uuid,
    source_id: Uuid,
    target_id: Uuid,
    relation_type: RelationType,
    persisted: bool,
}

struct WorkspaceData {
    storage: Arc<RwLock<Storage>>,
    entities: Vec<DisplayEntity>,
    relationships: Vec<DisplayRelationship>,
}

impl Clone for WorkspaceData {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            entities: self.entities.clone(),
            relationships: self.relationships.clone(),
        }
    }
}

impl std::fmt::Debug for WorkspaceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceData")
            .field("entities", &self.entities)
            .field("relationships", &self.relationships)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Message {
    // Menu
    ToggleMenu(OpenMenu),
    CloseMenu,
    // File menu
    NewWorkspace,
    OpenWorkspace,
    SaveWorkspace,
    SaveWorkspaceAs,
    OpenSettings,
    // File dialog results
    WorkspaceOpened(Option<PathBuf>),
    WorkspaceSaved(Option<PathBuf>),
    WorkspaceLoaded(Result<WorkspaceData, String>),
    // Edit menu
    Undo,
    Redo,
    // View menu
    ZoomIn,
    ZoomOut,
    FitToView,
    // Entity actions
    SelectEntity(usize),
    NewEntity,
    DeleteEntity,
    EntityNameChanged(String),
    EntityTypeChanged(EntityTypeOption),
    SaveEntity,
    ClosePanel,
    NodeClicked(usize),
    NodeDragged(usize, f32, f32),
    NodeDragEnd,
    SearchChanged(String),
    // Relationship actions
    StartRelationship,
    SelectTargetEntity(usize),
    RelationTypeChanged(RelationTypeOption),
    CreateRelationship,
    CancelRelationship,
    DeleteRelationship(Uuid),
    // Async results
    EntityCreated(Result<DisplayEntity, String>),
    EntityUpdated(Result<(), String>),
    EntityDeleted(Result<Uuid, String>),
    EntitiesLoaded(Result<Vec<DisplayEntity>, String>),
    RelationshipsLoaded(Result<Vec<DisplayRelationship>, String>),
    RelationshipCreated(Result<DisplayRelationship, String>),
    RelationshipDeleted(Result<Uuid, String>),
}

struct App {
    storage: Option<Arc<RwLock<Storage>>>,
    workspace_path: Option<PathBuf>,
    entities: Vec<DisplayEntity>,
    relationships: Vec<DisplayRelationship>,
    selected_index: Option<usize>,
    panel_open: bool,
    edit_name: String,
    edit_type: EntityTypeOption,
    graph_cache: Cache,
    open_menu: Option<OpenMenu>,
    status_message: String,
    // Relationship creation state
    creating_relationship: bool,
    relationship_source_index: Option<usize>,
    relationship_target_index: Option<usize>,
    relationship_type: RelationTypeOption,
    // Dragging state
    dragging_node: Option<usize>,
    // Search
    search_query: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            storage: None,
            workspace_path: None,
            entities: Vec::new(),
            relationships: Vec::new(),
            selected_index: None,
            panel_open: false,
            edit_name: String::new(),
            edit_type: EntityTypeOption::Person,
            graph_cache: Cache::new(),
            open_menu: None,
            status_message: "No workspace open".into(),
            creating_relationship: false,
            relationship_source_index: None,
            relationship_target_index: None,
            relationship_type: RelationTypeOption::AssociatedWith,
            dragging_node: None,
            search_query: String::new(),
        }
    }
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleMenu(menu) => {
                self.open_menu = if self.open_menu == Some(menu) { None } else { Some(menu) };
            }
            Message::CloseMenu => {
                self.open_menu = None;
            }
            Message::NewWorkspace => {
                self.open_menu = None;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Create New Workspace")
                            .add_filter("Muckrake Workspace", &["db", "sqlite"])
                            .save_file()
                            .await
                            .map(|h| h.path().to_path_buf())
                    },
                    Message::WorkspaceSaved,
                );
            }
            Message::OpenWorkspace => {
                self.open_menu = None;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Open Workspace")
                            .add_filter("Muckrake Workspace", &["db", "sqlite"])
                            .pick_file()
                            .await
                            .map(|h| h.path().to_path_buf())
                    },
                    Message::WorkspaceOpened,
                );
            }
            Message::SaveWorkspace => {
                self.open_menu = None;
                if let Some(ref path) = self.workspace_path {
                    self.status_message = format!("Saved: {}", path.display());
                }
            }
            Message::SaveWorkspaceAs => {
                self.open_menu = None;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Save Workspace As")
                            .add_filter("Muckrake Workspace", &["db", "sqlite"])
                            .save_file()
                            .await
                            .map(|h| h.path().to_path_buf())
                    },
                    Message::WorkspaceSaved,
                );
            }
            Message::WorkspaceOpened(Some(path)) => {
                let path_clone = path.clone();
                self.workspace_path = Some(path);
                self.status_message = format!("Opening: {}", path_clone.display());
                return Task::perform(
                    async move {
                        let path_str = path_clone.to_string_lossy().to_string();
                        match Storage::open(&path_str).await {
                            Ok(storage) => {
                                let entities = storage.list_entities(None).await
                                    .map_err(|e| e.to_string())?;
                                let relationships = storage.list_relationships().await
                                    .map_err(|e| e.to_string())?;

                                let display_entities: Vec<DisplayEntity> = entities.into_iter().enumerate().map(|(i, e)| {
                                    let entity_type = e.entity_type().into();
                                    DisplayEntity {
                                        id: e.id,
                                        name: e.canonical_name,
                                        entity_type,
                                        x: 150.0 + (i % 5) as f32 * 150.0,
                                        y: 100.0 + (i / 5) as f32 * 100.0,
                                        persisted: true,
                                    }
                                }).collect();

                                let display_relationships: Vec<DisplayRelationship> = relationships.into_iter().map(|r| {
                                    DisplayRelationship {
                                        id: r.id,
                                        source_id: r.source_id,
                                        target_id: r.target_id,
                                        relation_type: r.relation_type,
                                        persisted: true,
                                    }
                                }).collect();

                                Ok((storage, display_entities, display_relationships))
                            }
                            Err(e) => Err(e.to_string()),
                        }
                    },
                    |result| Message::WorkspaceLoaded(result.map(|(s, e, r)| WorkspaceData {
                        storage: Arc::new(RwLock::new(s)),
                        entities: e,
                        relationships: r,
                    })),
                );
            }
            Message::WorkspaceOpened(None) => {}
            Message::WorkspaceSaved(Some(path)) => {
                let path_clone = path.clone();
                self.workspace_path = Some(path);
                self.status_message = format!("Creating: {}", path_clone.display());
                return Task::perform(
                    async move {
                        let path_str = path_clone.to_string_lossy().to_string();
                        match Storage::open(&path_str).await {
                            Ok(storage) => Ok((storage, Vec::new(), Vec::new())),
                            Err(e) => Err(e.to_string()),
                        }
                    },
                    |result| Message::WorkspaceLoaded(result.map(|(s, e, r)| WorkspaceData {
                        storage: Arc::new(RwLock::new(s)),
                        entities: e,
                        relationships: r,
                    })),
                );
            }
            Message::WorkspaceSaved(None) => {}
            Message::WorkspaceLoaded(Ok(data)) => {
                self.storage = Some(data.storage);
                self.entities = data.entities;
                self.relationships = data.relationships;
                self.graph_cache.clear();
                if let Some(ref path) = self.workspace_path {
                    self.status_message = format!("Opened: {} ({} entities, {} relationships)",
                        path.display(), self.entities.len(), self.relationships.len());
                }
            }
            Message::WorkspaceLoaded(Err(e)) => {
                self.status_message = format!("Error: {}", e);
            }
            Message::OpenSettings | Message::Undo | Message::Redo
            | Message::ZoomIn | Message::ZoomOut | Message::FitToView => {
                self.open_menu = None;
            }
            Message::SelectEntity(idx) | Message::NodeClicked(idx) => {
                self.selected_index = Some(idx);
                self.dragging_node = Some(idx);
                if let Some(entity) = self.entities.get(idx) {
                    self.edit_name = entity.name.clone();
                    self.edit_type = entity.entity_type;
                    self.panel_open = true;
                }
                self.graph_cache.clear();
                self.open_menu = None;
            }
            Message::NodeDragged(idx, x, y) => {
                if let Some(entity) = self.entities.get_mut(idx) {
                    entity.x = x;
                    entity.y = y;
                    self.graph_cache.clear();
                }
            }
            Message::NodeDragEnd => {
                self.dragging_node = None;
            }
            Message::SearchChanged(query) => {
                self.search_query = query;
            }
            Message::NewEntity => {
                let x = 150.0 + (self.entities.len() % 5) as f32 * 150.0;
                let y = 100.0 + (self.entities.len() / 5) as f32 * 100.0;
                let new_entity = DisplayEntity {
                    id: Uuid::now_v7(),
                    name: format!("Entity {}", self.entities.len() + 1),
                    entity_type: EntityTypeOption::Person,
                    x,
                    y,
                    persisted: false,
                };

                if let Some(ref storage) = self.storage {
                    let storage = storage.clone();
                    let entity = new_entity.to_entity();
                    let display_entity = new_entity.clone();
                    self.entities.push(new_entity);
                    self.graph_cache.clear();
                    return Task::perform(
                        async move {
                            let storage = storage.write().await;
                            storage.insert_entity(&entity).await
                                .map_err(|e| e.to_string())?;
                            Ok(display_entity)
                        },
                        Message::EntityCreated,
                    );
                } else {
                    self.entities.push(new_entity);
                    self.graph_cache.clear();
                }
            }
            Message::DeleteEntity => {
                if let Some(idx) = self.selected_index {
                    let entity = &self.entities[idx];
                    let entity_id = entity.id;
                    let was_persisted = entity.persisted;

                    self.relationships.retain(|r| r.source_id != entity_id && r.target_id != entity_id);
                    self.entities.remove(idx);
                    self.selected_index = None;
                    self.panel_open = false;
                    self.graph_cache.clear();

                    if was_persisted {
                        if let Some(ref storage) = self.storage {
                            let storage = storage.clone();
                            return Task::perform(
                                async move {
                                    let storage = storage.write().await;
                                    storage.delete_entity(entity_id).await
                                        .map_err(|e| e.to_string())?;
                                    Ok(entity_id)
                                },
                                Message::EntityDeleted,
                            );
                        }
                    }
                }
            }
            Message::EntityNameChanged(name) => {
                self.edit_name = name;
            }
            Message::EntityTypeChanged(t) => {
                self.edit_type = t;
            }
            Message::SaveEntity => {
                if let Some(idx) = self.selected_index {
                    if let Some(entity) = self.entities.get_mut(idx) {
                        entity.name = self.edit_name.clone();
                        entity.entity_type = self.edit_type;

                        if let Some(ref storage) = self.storage {
                            let storage = storage.clone();
                            let core_entity = entity.to_entity();
                            let entity_id = entity.id;
                            let was_persisted = entity.persisted;
                            self.graph_cache.clear();

                            if was_persisted {
                                return Task::perform(
                                    async move {
                                        let storage = storage.write().await;
                                        storage.update_entity(&core_entity).await
                                            .map_err(|e| e.to_string())?;
                                        Ok(())
                                    },
                                    Message::EntityUpdated,
                                );
                            } else {
                                return Task::perform(
                                    async move {
                                        let storage = storage.write().await;
                                        storage.insert_entity(&core_entity).await
                                            .map_err(|e| e.to_string())?;
                                        Ok(entity_id)
                                    },
                                    |result| Message::EntityCreated(result.map(|id| DisplayEntity {
                                        id,
                                        name: String::new(),
                                        entity_type: EntityTypeOption::Person,
                                        x: 0.0,
                                        y: 0.0,
                                        persisted: true,
                                    })),
                                );
                            }
                        }
                    }
                }
                self.graph_cache.clear();
            }
            Message::ClosePanel => {
                self.panel_open = false;
                self.selected_index = None;
                self.graph_cache.clear();
            }
            Message::EntityCreated(Ok(display)) => {
                if let Some(entity) = self.entities.iter_mut().find(|e| e.id == display.id) {
                    entity.persisted = true;
                }
                self.status_message = "Entity saved".into();
            }
            Message::EntityCreated(Err(e)) => {
                self.status_message = format!("Error creating entity: {e}");
            }
            Message::EntityUpdated(Ok(())) => {
                self.status_message = "Entity updated".into();
            }
            Message::EntityUpdated(Err(e)) => {
                self.status_message = format!("Error updating entity: {e}");
            }
            Message::EntityDeleted(Ok(_)) => {
                self.status_message = "Entity deleted".into();
            }
            Message::EntityDeleted(Err(e)) => {
                self.status_message = format!("Error deleting entity: {e}");
            }
            Message::EntitiesLoaded(_) | Message::RelationshipsLoaded(_) => {}
            Message::StartRelationship => {
                if let Some(idx) = self.selected_index {
                    self.creating_relationship = true;
                    self.relationship_source_index = Some(idx);
                    self.relationship_target_index = None;
                    self.relationship_type = RelationTypeOption::AssociatedWith;
                    self.status_message = "Select target entity...".into();
                }
            }
            Message::SelectTargetEntity(idx) => {
                if self.creating_relationship && self.relationship_source_index != Some(idx) {
                    self.relationship_target_index = Some(idx);
                }
            }
            Message::RelationTypeChanged(rt) => {
                self.relationship_type = rt;
            }
            Message::CreateRelationship => {
                if let (Some(source_idx), Some(target_idx)) = (
                    self.relationship_source_index,
                    self.relationship_target_index,
                ) {
                    let source_entity = &self.entities[source_idx];
                    let target_entity = &self.entities[target_idx];
                    let source_id = source_entity.id;
                    let target_id = target_entity.id;
                    let relation_type: RelationType = self.relationship_type.into();

                    let display_rel = DisplayRelationship {
                        id: Uuid::now_v7(),
                        source_id,
                        target_id,
                        relation_type: relation_type.clone(),
                        persisted: false,
                    };

                    self.relationships.push(display_rel.clone());
                    self.creating_relationship = false;
                    self.relationship_source_index = None;
                    self.relationship_target_index = None;
                    self.graph_cache.clear();

                    if let Some(ref storage) = self.storage {
                        let storage = storage.clone();
                        let rel_id = display_rel.id;
                        return Task::perform(
                            async move {
                                let storage = storage.write().await;
                                let relationship = Relationship::new(source_id, target_id, relation_type)
                                    .map_err(|e| e.to_string())?;
                                storage.insert_relationship(&Relationship {
                                    id: rel_id,
                                    ..relationship
                                }).await.map_err(|e| e.to_string())?;
                                Ok(DisplayRelationship {
                                    id: rel_id,
                                    source_id,
                                    target_id,
                                    relation_type: relation_type.into(),
                                    persisted: true,
                                })
                            },
                            Message::RelationshipCreated,
                        );
                    }
                }
            }
            Message::CancelRelationship => {
                self.creating_relationship = false;
                self.relationship_source_index = None;
                self.relationship_target_index = None;
                self.status_message = "Relationship cancelled".into();
            }
            Message::DeleteRelationship(rel_id) => {
                if let Some(pos) = self.relationships.iter().position(|r| r.id == rel_id) {
                    let rel = self.relationships.remove(pos);
                    self.graph_cache.clear();

                    if rel.persisted {
                        if let Some(ref storage) = self.storage {
                            let storage = storage.clone();
                            return Task::perform(
                                async move {
                                    let storage = storage.write().await;
                                    storage.delete_relationship(rel_id).await
                                        .map_err(|e| e.to_string())?;
                                    Ok(rel_id)
                                },
                                Message::RelationshipDeleted,
                            );
                        }
                    }
                }
            }
            Message::RelationshipCreated(Ok(display)) => {
                if let Some(rel) = self.relationships.iter_mut().find(|r| r.id == display.id) {
                    rel.persisted = true;
                }
                self.status_message = "Relationship created".into();
            }
            Message::RelationshipCreated(Err(e)) => {
                self.status_message = format!("Error creating relationship: {e}");
            }
            Message::RelationshipDeleted(Ok(_)) => {
                self.status_message = "Relationship deleted".into();
            }
            Message::RelationshipDeleted(Err(e)) => {
                self.status_message = format!("Error deleting relationship: {e}");
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let menu_bar = self.view_menu_bar();
        let sidebar = self.view_sidebar();
        let graph = self.view_graph();
        let status_bar = self.view_status_bar();

        let mut main_row = row![sidebar, vertical_rule(1), graph];

        if self.panel_open || self.creating_relationship {
            main_row = main_row.push(vertical_rule(1)).push(self.view_entity_panel());
        }

        column![menu_bar, main_row, status_bar].into()
    }

    fn view_menu_bar(&self) -> Element<'_, Message> {
        let file_btn = button(container(text("File").size(14)).padding([4, 8]))
            .on_press(Message::ToggleMenu(OpenMenu::File))
            .style(|_, _| button::Style {
                background: Some(colors::MANTLE.into()),
                text_color: colors::TEXT,
                ..Default::default()
            });

        let edit_btn = button(container(text("Edit").size(14)).padding([4, 8]))
            .on_press(Message::ToggleMenu(OpenMenu::Edit))
            .style(|_, _| button::Style {
                background: Some(colors::MANTLE.into()),
                text_color: colors::TEXT,
                ..Default::default()
            });

        let view_btn = button(container(text("View").size(14)).padding([4, 8]))
            .on_press(Message::ToggleMenu(OpenMenu::View))
            .style(|_, _| button::Style {
                background: Some(colors::MANTLE.into()),
                text_color: colors::TEXT,
                ..Default::default()
            });

        let menu_buttons = row![file_btn, edit_btn, view_btn].spacing(4);

        let menu_bar_content = match self.open_menu {
            Some(OpenMenu::File) => column![
                container(menu_buttons).padding([4, 8]),
                row![container(self.view_file_menu()).padding([0, 8]), horizontal_space()],
            ],
            Some(OpenMenu::Edit) => column![
                container(menu_buttons).padding([4, 8]),
                row![
                    iced::widget::Space::with_width(Length::Fixed(58.0)),
                    container(self.view_edit_menu()),
                    horizontal_space(),
                ],
            ],
            Some(OpenMenu::View) => column![
                container(menu_buttons).padding([4, 8]),
                row![
                    iced::widget::Space::with_width(Length::Fixed(116.0)),
                    container(self.view_view_menu()),
                    horizontal_space(),
                ],
            ],
            None => column![container(menu_buttons).padding([4, 8])],
        };

        container(menu_bar_content)
            .width(Fill)
            .style(|_| container::Style {
                background: Some(colors::MANTLE.into()),
                ..Default::default()
            })
            .into()
    }

    fn view_file_menu(&self) -> Element<'_, Message> {
        container(
            column![
                self.menu_item("New Workspace", "Ctrl+N", Message::NewWorkspace),
                self.menu_item("Open Workspace", "Ctrl+O", Message::OpenWorkspace),
                horizontal_rule(1),
                self.menu_item("Save", "Ctrl+S", Message::SaveWorkspace),
                self.menu_item("Save As...", "Ctrl+Shift+S", Message::SaveWorkspaceAs),
                horizontal_rule(1),
                self.menu_item("Settings", "Ctrl+,", Message::OpenSettings),
            ]
            .spacing(2)
            .width(Length::Shrink),
        )
        .width(Length::Shrink)
        .style(|_| container::Style {
            background: Some(colors::SURFACE0.into()),
            border: iced::Border { radius: 4.0.into(), ..Default::default() },
            ..Default::default()
        })
        .into()
    }

    fn view_edit_menu(&self) -> Element<'_, Message> {
        container(
            column![
                self.menu_item("Undo", "Ctrl+Z", Message::Undo),
                self.menu_item("Redo", "Ctrl+Y", Message::Redo),
            ]
            .spacing(2)
            .width(Length::Shrink),
        )
        .width(Length::Shrink)
        .style(|_| container::Style {
            background: Some(colors::SURFACE0.into()),
            border: iced::Border { radius: 4.0.into(), ..Default::default() },
            ..Default::default()
        })
        .into()
    }

    fn view_view_menu(&self) -> Element<'_, Message> {
        container(
            column![
                self.menu_item("Zoom In", "Ctrl++", Message::ZoomIn),
                self.menu_item("Zoom Out", "Ctrl+-", Message::ZoomOut),
                self.menu_item("Fit to View", "Ctrl+0", Message::FitToView),
            ]
            .spacing(2)
            .width(Length::Shrink),
        )
        .width(Length::Shrink)
        .style(|_| container::Style {
            background: Some(colors::SURFACE0.into()),
            border: iced::Border { radius: 4.0.into(), ..Default::default() },
            ..Default::default()
        })
        .into()
    }

    fn menu_item(&self, label: &'static str, shortcut: &'static str, msg: Message) -> Element<'_, Message> {
        button(
            row![
                text(label).size(13),
                iced::widget::Space::with_width(Length::Fixed(24.0)),
                text(shortcut).size(10),
            ]
            .padding([4, 10]),
        )
        .on_press(msg)
        .style(|_, status| button::Style {
            background: Some(if status == button::Status::Hovered {
                colors::SURFACE1.into()
            } else {
                colors::SURFACE0.into()
            }),
            text_color: colors::TEXT,
            ..Default::default()
        })
        .into()
    }

    fn view_sidebar(&self) -> Element<'_, Message> {
        let header_text = if self.creating_relationship {
            "Select Target"
        } else {
            "Entities"
        };

        let header = row![
            text(header_text).size(14),
            horizontal_space(),
            button(text("+").size(14)).on_press(Message::NewEntity).padding([4, 8]),
            button(text("−").size(14)).on_press(Message::DeleteEntity).padding([4, 8]),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);

        let search_input = text_input("Search entities...", &self.search_query)
            .on_input(Message::SearchChanged)
            .padding(8)
            .size(13);

        let creating_rel = self.creating_relationship;
        let source_idx = self.relationship_source_index;
        let target_idx = self.relationship_target_index;
        let query = self.search_query.to_lowercase();

        let entity_list: Element<'_, Message> = scrollable(
            Column::with_children(
                self.entities
                    .iter()
                    .enumerate()
                    .filter(|(_, entity)| {
                        query.is_empty() || entity.name.to_lowercase().contains(&query)
                    })
                    .map(|(idx, entity)| {
                        let is_selected = self.selected_index == Some(idx);
                        let is_source = source_idx == Some(idx);
                        let is_target = target_idx == Some(idx);
                        let type_color = entity_type_color(entity.entity_type);

                        let bg_color = if is_source {
                            colors::BLUE
                        } else if is_target {
                            colors::GREEN
                        } else if is_selected {
                            colors::SURFACE1
                        } else {
                            colors::BASE
                        };

                        let msg = if creating_rel && !is_source {
                            Message::SelectTargetEntity(idx)
                        } else {
                            Message::SelectEntity(idx)
                        };

                        button(
                            row![
                                container(text("●").size(10))
                                    .style(move |_| container::Style {
                                        text_color: Some(type_color),
                                        ..Default::default()
                                    }),
                                text(&entity.name),
                            ]
                            .spacing(8)
                            .padding([8, 12]),
                        )
                        .on_press(msg)
                        .width(Fill)
                        .style(move |_, _| button::Style {
                            background: Some(bg_color.into()),
                            text_color: if is_source || is_target { colors::CRUST } else { colors::TEXT },
                            ..Default::default()
                        })
                        .into()
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(2),
        )
        .height(Fill)
        .into();

        container(column![header, search_input, entity_list].spacing(8).padding(12))
            .width(260)
            .height(Fill)
            .style(|_| container::Style {
                background: Some(colors::BASE.into()),
                ..Default::default()
            })
            .into()
    }

    fn view_graph(&self) -> Element<'_, Message> {
        let graph_view = GraphCanvas {
            entities: &self.entities,
            relationships: &self.relationships,
            selected_index: self.selected_index,
            dragging_node: self.dragging_node,
            cache: &self.graph_cache,
        };

        container(canvas(graph_view).width(Fill).height(Fill))
            .width(Fill)
            .height(Fill)
            .style(|_| container::Style {
                background: Some(colors::CRUST.into()),
                ..Default::default()
            })
            .into()
    }

    fn view_entity_panel(&self) -> Element<'_, Message> {
        if self.creating_relationship {
            return self.view_relationship_panel();
        }

        let header = row![
            text("Entity Details").size(14),
            horizontal_space(),
            button(text("×").size(14)).on_press(Message::ClosePanel).padding([4, 8]),
        ]
        .align_y(iced::Alignment::Center);

        let name_field = column![
            text("Name").size(12),
            text_input("Entity name", &self.edit_name)
                .on_input(Message::EntityNameChanged)
                .padding(8),
        ]
        .spacing(4);

        let type_field = column![
            text("Type").size(12),
            pick_list(
                EntityTypeOption::ALL.as_slice(),
                Some(self.edit_type),
                Message::EntityTypeChanged,
            )
            .width(Fill)
            .padding(8),
        ]
        .spacing(4);

        let buttons = row![
            button(text("Save")).on_press(Message::SaveEntity).padding([8, 16]),
            button(text("Link")).on_press(Message::StartRelationship).padding([8, 16]),
        ]
        .spacing(8);

        // Show relationships involving this entity
        let selected_id = self.selected_index.and_then(|idx| self.entities.get(idx).map(|e| e.id));
        let related: Vec<_> = self.relationships.iter()
            .filter(|r| selected_id == Some(r.source_id) || selected_id == Some(r.target_id))
            .collect();

        let relationships_section: Element<'_, Message> = if related.is_empty() {
            text("No relationships").size(12).into()
        } else {
            Column::with_children(
                related.iter().map(|r| {
                    let source_name = self.entities.iter()
                        .find(|e| e.id == r.source_id)
                        .map(|e| e.name.as_str())
                        .unwrap_or("?");
                    let target_name = self.entities.iter()
                        .find(|e| e.id == r.target_id)
                        .map(|e| e.name.as_str())
                        .unwrap_or("?");
                    let rel_type: RelationTypeOption = r.relation_type.into();
                    let rel_id = r.id;

                    row![
                        text(format!("{} → {} → {}", source_name, rel_type, target_name)).size(11),
                        horizontal_space(),
                        button(text("×").size(10))
                            .on_press(Message::DeleteRelationship(rel_id))
                            .padding([2, 6]),
                    ]
                    .spacing(4)
                    .align_y(iced::Alignment::Center)
                    .into()
                }).collect::<Vec<_>>()
            )
            .spacing(4)
            .into()
        };

        let rel_header = text("Relationships").size(12);

        container(
            column![header, name_field, type_field, buttons, horizontal_rule(1), rel_header, relationships_section]
                .spacing(12)
                .padding(12)
        )
        .width(300)
        .height(Fill)
        .style(|_| container::Style {
            background: Some(colors::BASE.into()),
            ..Default::default()
        })
        .into()
    }

    fn view_relationship_panel(&self) -> Element<'_, Message> {
        let header = row![
            text("Create Relationship").size(14),
            horizontal_space(),
            button(text("×").size(14)).on_press(Message::CancelRelationship).padding([4, 8]),
        ]
        .align_y(iced::Alignment::Center);

        let source_name = self.relationship_source_index
            .and_then(|idx| self.entities.get(idx))
            .map(|e| e.name.as_str())
            .unwrap_or("None");

        let target_name = self.relationship_target_index
            .and_then(|idx| self.entities.get(idx))
            .map(|e| e.name.as_str())
            .unwrap_or("Select in list...");

        let source_field = column![
            text("From").size(12),
            container(text(source_name).size(14))
                .padding(8)
                .style(|_| container::Style {
                    background: Some(colors::BLUE.into()),
                    border: iced::Border { radius: 4.0.into(), ..Default::default() },
                    text_color: Some(colors::CRUST),
                    ..Default::default()
                }),
        ]
        .spacing(4);

        let target_field = column![
            text("To").size(12),
            container(text(target_name).size(14))
                .padding(8)
                .style(|_| container::Style {
                    background: Some(if self.relationship_target_index.is_some() {
                        colors::GREEN.into()
                    } else {
                        colors::SURFACE1.into()
                    }),
                    border: iced::Border { radius: 4.0.into(), ..Default::default() },
                    text_color: Some(if self.relationship_target_index.is_some() {
                        colors::CRUST
                    } else {
                        colors::SUBTEXT
                    }),
                    ..Default::default()
                }),
        ]
        .spacing(4);

        let type_field = column![
            text("Relationship Type").size(12),
            pick_list(
                RelationTypeOption::ALL.as_slice(),
                Some(self.relationship_type),
                Message::RelationTypeChanged,
            )
            .width(Fill)
            .padding(8),
        ]
        .spacing(4);

        let can_create = self.relationship_source_index.is_some() && self.relationship_target_index.is_some();
        let create_btn = button(text("Create"))
            .on_press_maybe(if can_create { Some(Message::CreateRelationship) } else { None })
            .padding([8, 16]);

        let cancel_btn = button(text("Cancel"))
            .on_press(Message::CancelRelationship)
            .padding([8, 16]);

        let buttons = row![create_btn, cancel_btn].spacing(8);

        container(
            column![header, source_field, target_field, type_field, buttons]
                .spacing(12)
                .padding(12)
        )
        .width(300)
        .height(Fill)
        .style(|_| container::Style {
            background: Some(colors::BASE.into()),
            ..Default::default()
        })
        .into()
    }

    fn view_status_bar(&self) -> Element<'_, Message> {
        let left = text(format!(
            "{} entities · {} relationships",
            self.entities.len(),
            self.relationships.len(),
        ))
        .size(12);

        let right = text(&self.status_message).size(12);

        container(row![left, horizontal_space(), right].padding([0, 12]))
            .center_y(28)
            .width(Fill)
            .style(|_| container::Style {
                background: Some(colors::MANTLE.into()),
                ..Default::default()
            })
            .into()
    }
}

fn entity_type_color(et: EntityTypeOption) -> iced::Color {
    match et {
        EntityTypeOption::Person => colors::BLUE,
        EntityTypeOption::Organization => colors::MAUVE,
        EntityTypeOption::Location => colors::GREEN,
        EntityTypeOption::Document => colors::PEACH,
        EntityTypeOption::Event => colors::YELLOW,
    }
}

struct GraphCanvas<'a> {
    entities: &'a [DisplayEntity],
    relationships: &'a [DisplayRelationship],
    selected_index: Option<usize>,
    dragging_node: Option<usize>,
    cache: &'a Cache,
}

#[derive(Default)]
struct GraphCanvasState {
    dragging: Option<(usize, Point)>,
}

impl<'a> Program<Message> for GraphCanvas<'a> {
    type State = GraphCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (event::Status, Option<Message>) {
        let cursor_position = cursor.position_in(bounds);

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor_position {
                    for (idx, entity) in self.entities.iter().enumerate() {
                        let node_bounds = Rectangle::new(
                            Point::new(entity.x - 60.0, entity.y - 24.0),
                            Size::new(120.0, 48.0),
                        );
                        if node_bounds.contains(pos) {
                            state.dragging = Some((idx, Point::new(pos.x - entity.x, pos.y - entity.y)));
                            return (event::Status::Captured, Some(Message::NodeClicked(idx)));
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some((idx, offset)), Some(pos)) = (state.dragging, cursor_position) {
                    let new_x = pos.x - offset.x;
                    let new_y = pos.y - offset.y;
                    return (event::Status::Captured, Some(Message::NodeDragged(idx, new_x, new_y)));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging.is_some() {
                    state.dragging = None;
                    return (event::Status::Captured, Some(Message::NodeDragEnd));
                }
            }
            _ => {}
        }

        (event::Status::Ignored, None)
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging.is_some() {
            return mouse::Interaction::Grabbing;
        }

        if let Some(pos) = cursor.position_in(bounds) {
            for entity in self.entities {
                let node_bounds = Rectangle::new(
                    Point::new(entity.x - 60.0, entity.y - 24.0),
                    Size::new(120.0, 48.0),
                );
                if node_bounds.contains(pos) {
                    return mouse::Interaction::Grab;
                }
            }
        }

        mouse::Interaction::default()
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let entity_positions: std::collections::HashMap<Uuid, (f32, f32)> = self
            .entities
            .iter()
            .map(|e| (e.id, (e.x, e.y)))
            .collect();

        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            // Draw relationships
            for rel in self.relationships {
                if let (Some(&(x1, y1)), Some(&(x2, y2))) = (
                    entity_positions.get(&rel.source_id),
                    entity_positions.get(&rel.target_id),
                ) {
                    let path = Path::line(Point::new(x1, y1), Point::new(x2, y2));
                    frame.stroke(&path, Stroke::default().with_color(colors::SURFACE2).with_width(2.0));
                }
            }

            // Draw nodes
            for (idx, entity) in self.entities.iter().enumerate() {
                let is_selected = self.selected_index == Some(idx);
                let is_dragging = self.dragging_node == Some(idx);
                let color = if is_selected || is_dragging {
                    colors::MAUVE
                } else {
                    entity_type_color(entity.entity_type)
                };

                let node = Path::rectangle(
                    Point::new(entity.x - 60.0, entity.y - 24.0),
                    Size::new(120.0, 48.0),
                );
                frame.fill(&node, color);

                frame.fill_text(canvas::Text {
                    content: entity.name.clone(),
                    position: Point::new(entity.x, entity.y - 6.0),
                    color: colors::CRUST,
                    size: 12.0.into(),
                    horizontal_alignment: iced::alignment::Horizontal::Center,
                    vertical_alignment: iced::alignment::Vertical::Center,
                    ..Default::default()
                });

                frame.fill_text(canvas::Text {
                    content: entity.entity_type.to_string(),
                    position: Point::new(entity.x, entity.y + 10.0),
                    color: colors::MANTLE,
                    size: 10.0.into(),
                    horizontal_alignment: iced::alignment::Horizontal::Center,
                    vertical_alignment: iced::alignment::Vertical::Center,
                    ..Default::default()
                });
            }
        });

        vec![geometry]
    }
}
