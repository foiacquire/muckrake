use sea_query::Iden;

#[derive(Iden)]
pub enum Scopes {
    Table,
    Id,
    Name,
    ScopeType,
    Pattern,
    CategoryType,
    Description,
    CreatedAt,
}

#[derive(Iden)]
pub enum ScopePolicy {
    Table,
    Id,
    ScopeId,
    ProtectionLevel,
}

#[derive(Iden)]
pub enum ScopeToolConfig {
    Table,
    Id,
    ScopeId,
    Action,
    FileType,
    Command,
    Env,
    Quiet,
}

#[derive(Clone, Copy, Iden)]
pub enum Files {
    Table,
    Id,
    Sha256,
    Fingerprint,
    MimeType,
    Size,
    IngestedAt,
    Provenance,
}

#[derive(Iden)]
pub enum FileTags {
    Table,
    FileId,
    Tag,
    FileHash,
    Fingerprint,
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
pub enum WorkspaceConfig {
    Table,
    Key,
    Value,
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
    Source,
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
