use sea_query::Iden;

#[derive(Iden)]
pub enum Categories {
    Table,
    Id,
    Name,
    Pattern,
    CategoryType,
    Description,
}

#[derive(Iden)]
pub enum CategoryPolicy {
    Table,
    Id,
    CategoryId,
    ProtectionLevel,
}

#[derive(Iden)]
pub enum Files {
    Table,
    Id,
    Name,
    Path,
    Sha256,
    MimeType,
    Size,
    IngestedAt,
    Provenance,
    Immutable,
}

#[derive(Iden)]
pub enum FileTags {
    Table,
    FileId,
    Tag,
    FileHash,
}

#[derive(Iden)]
pub enum AuditLog {
    Table,
    Id,
    Timestamp,
    Operation,
    FileId,
    User,
    Detail,
}

#[derive(Iden)]
pub enum ToolConfig {
    Table,
    Id,
    Scope,
    Action,
    FileType,
    Command,
    Env,
    Quiet,
}

#[derive(Iden)]
pub enum TagToolConfig {
    Table,
    Id,
    Tag,
    Action,
    FileType,
    Command,
    Env,
    Quiet,
}

#[derive(Iden)]
pub enum WorkspaceConfig {
    Table,
    Key,
    Value,
}

#[derive(Iden)]
pub enum Projects {
    Table,
    Id,
    Name,
    Path,
    Description,
    CreatedAt,
}

#[derive(Iden)]
pub enum DefaultCategories {
    Table,
    Id,
    Name,
    Pattern,
    CategoryType,
    Description,
}

#[derive(Iden)]
pub enum DefaultCategoryPolicy {
    Table,
    Id,
    DefaultCategoryId,
    ProtectionLevel,
}

#[derive(Iden)]
pub enum Rules {
    Table,
    Id,
    Name,
    Enabled,
    TriggerEvent,
    TriggerFilter,
    ActionType,
    ActionConfig,
    Priority,
    CreatedAt,
}

#[derive(Iden)]
pub enum Pipelines {
    Table,
    Id,
    Name,
    States,
    Transitions,
}

#[derive(Iden)]
pub enum PipelineAttachments {
    Table,
    Id,
    PipelineId,
    ScopeType,
    ScopeValue,
}

#[derive(Iden)]
pub enum Signs {
    Table,
    Id,
    PipelineId,
    FileId,
    FileHash,
    SignName,
    Signer,
    SignedAt,
    Signature,
    RevokedAt,
}

#[derive(Iden)]
pub enum DefaultPipelines {
    Table,
    Id,
    Name,
    States,
    Transitions,
}

#[derive(Iden)]
pub enum EntityLinks {
    Table,
    Id,
    EntityName,
    EntityType,
    ProjectName,
    ProjectEntityId,
}
