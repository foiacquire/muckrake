package db

const scopeTablesSchema = `
CREATE TABLE IF NOT EXISTS scopes (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    scope_type TEXT NOT NULL,
    pattern TEXT,
    category_type TEXT DEFAULT 'files',
    description TEXT,
    created_at TEXT
);

CREATE TABLE IF NOT EXISTS scope_policy (
    id INTEGER PRIMARY KEY,
    scope_id INTEGER NOT NULL UNIQUE REFERENCES scopes(id),
    protection_level TEXT NOT NULL DEFAULT 'editable'
);

CREATE TABLE IF NOT EXISTS scope_tool_config (
    id INTEGER PRIMARY KEY,
    scope_id INTEGER REFERENCES scopes(id),
    action TEXT NOT NULL,
    file_type TEXT NOT NULL,
    command TEXT NOT NULL,
    env TEXT,
    quiet INTEGER NOT NULL DEFAULT 1,
    UNIQUE(scope_id, action, file_type)
);
`

const filesSchema = `
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    sha256 TEXT NOT NULL UNIQUE,
    fingerprint TEXT NOT NULL,
    mime_type TEXT,
    size INTEGER,
    ingested_at TEXT NOT NULL,
    provenance TEXT
);

CREATE TABLE IF NOT EXISTS file_tags (
    file_id INTEGER REFERENCES files(id),
    tag TEXT NOT NULL,
    file_hash TEXT,
    fingerprint TEXT,
    PRIMARY KEY (file_id, tag)
);

CREATE TABLE IF NOT EXISTS entities (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    aliases TEXT,
    metadata TEXT
);

CREATE TABLE IF NOT EXISTS relationships (
    id INTEGER PRIMARY KEY,
    source_entity_id INTEGER REFERENCES entities(id),
    target_entity_id INTEGER REFERENCES entities(id),
    relationship_type TEXT NOT NULL,
    confidence REAL,
    evidence_file_id INTEGER REFERENCES files(id),
    metadata TEXT
);

CREATE TABLE IF NOT EXISTS file_entities (
    file_id INTEGER REFERENCES files(id),
    entity_id INTEGER REFERENCES entities(id),
    context TEXT,
    PRIMARY KEY (file_id, entity_id)
);

CREATE TABLE IF NOT EXISTS file_links (
    id INTEGER PRIMARY KEY,
    source_file_id INTEGER REFERENCES files(id),
    target_file_id INTEGER REFERENCES files(id),
    link_type TEXT NOT NULL,
    metadata TEXT
);
`

const rulesSchema = `
CREATE TABLE IF NOT EXISTS rules (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    enabled INTEGER NOT NULL DEFAULT 1,
    trigger_event TEXT NOT NULL,
    trigger_filter TEXT,
    action_type TEXT NOT NULL,
    action_config TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
`

const pipelineSchema = `
CREATE TABLE IF NOT EXISTS pipelines (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    states TEXT NOT NULL,
    transitions TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pipeline_subscriptions (
    id INTEGER PRIMARY KEY,
    pipeline_id INTEGER NOT NULL REFERENCES pipelines(id),
    reference TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(pipeline_id, reference)
);

CREATE TABLE IF NOT EXISTS pipeline_files (
    pipeline_id INTEGER NOT NULL REFERENCES pipelines(id),
    sha256 TEXT NOT NULL,
    subscription_id INTEGER NOT NULL REFERENCES pipeline_subscriptions(id),
    attached_at TEXT NOT NULL,
    PRIMARY KEY (pipeline_id, sha256)
);

CREATE TABLE IF NOT EXISTS signs (
    id INTEGER PRIMARY KEY,
    pipeline_id INTEGER NOT NULL REFERENCES pipelines(id),
    file_id INTEGER NOT NULL REFERENCES files(id),
    file_hash TEXT NOT NULL,
    sign_name TEXT NOT NULL,
    signer TEXT NOT NULL,
    signed_at TEXT NOT NULL,
    signature TEXT,
    revoked_at TEXT
);
`

const rulesetSchema = `
CREATE TABLE IF NOT EXISTS rulesets (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT
);

CREATE TABLE IF NOT EXISTS ruleset_rules (
    id INTEGER PRIMARY KEY,
    ruleset_id INTEGER NOT NULL REFERENCES rulesets(id),
    priority INTEGER NOT NULL DEFAULT 0,
    condition TEXT,
    action_type TEXT NOT NULL,
    action_config TEXT NOT NULL,
    UNIQUE(ruleset_id, priority)
);

CREATE TABLE IF NOT EXISTS ruleset_subscriptions (
    id INTEGER PRIMARY KEY,
    ruleset_id INTEGER NOT NULL REFERENCES rulesets(id),
    reference TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(ruleset_id, reference)
);

CREATE TABLE IF NOT EXISTS ruleset_files (
    ruleset_id INTEGER NOT NULL REFERENCES rulesets(id),
    sha256 TEXT NOT NULL,
    subscription_id INTEGER NOT NULL REFERENCES ruleset_subscriptions(id),
    attached_at TEXT NOT NULL,
    PRIMARY KEY (ruleset_id, sha256)
);
`

const auditSchema = `
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    operation TEXT NOT NULL,
    file_id INTEGER REFERENCES files(id),
    user TEXT,
    detail TEXT
);
`

const workspaceSchema = `
CREATE TABLE IF NOT EXISTS workspace_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS default_pipelines (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    states TEXT NOT NULL,
    transitions TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS entity_links (
    id INTEGER PRIMARY KEY,
    entity_name TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    project_name TEXT NOT NULL,
    project_entity_id INTEGER,
    UNIQUE(entity_name, entity_type, project_name)
);
`

// ProjectSchema is the full schema for a .mkrk project database.
var ProjectSchema = scopeTablesSchema + filesSchema + rulesSchema + pipelineSchema + rulesetSchema + auditSchema

// WorkspaceSchema is the full schema for a .mksp workspace database.
var WorkspaceSchema = workspaceSchema + scopeTablesSchema + rulesetSchema
