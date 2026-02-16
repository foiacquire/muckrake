const TOOL_TABLES_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tool_config (
    id INTEGER PRIMARY KEY,
    scope TEXT,
    action TEXT NOT NULL,
    file_type TEXT NOT NULL,
    command TEXT NOT NULL,
    env TEXT,
    quiet INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS tag_tool_config (
    id INTEGER PRIMARY KEY,
    tag TEXT NOT NULL,
    action TEXT NOT NULL,
    file_type TEXT NOT NULL,
    command TEXT NOT NULL,
    env TEXT,
    quiet INTEGER NOT NULL DEFAULT 1,
    UNIQUE(tag, action, file_type)
);
";

pub const PROJECT_SCHEMA_PREFIX: &str = "
CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    pattern TEXT NOT NULL UNIQUE,
    category_type TEXT NOT NULL DEFAULT 'files',
    description TEXT
);

CREATE TABLE IF NOT EXISTS category_policy (
    id INTEGER PRIMARY KEY,
    category_id INTEGER NOT NULL REFERENCES categories(id),
    protection_level TEXT NOT NULL DEFAULT 'editable',
    UNIQUE(category_id)
);

CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    sha256 TEXT,
    mime_type TEXT,
    size INTEGER,
    ingested_at TEXT NOT NULL,
    provenance TEXT,
    immutable INTEGER DEFAULT 0
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

CREATE TABLE IF NOT EXISTS file_tags (
    file_id INTEGER REFERENCES files(id),
    tag TEXT NOT NULL,
    file_hash TEXT,
    PRIMARY KEY (file_id, tag)
);

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
";

const PROJECT_SCHEMA_SUFFIX: &str = "
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    operation TEXT NOT NULL,
    file_id INTEGER REFERENCES files(id),
    user TEXT,
    detail TEXT
);
";

pub const WORKSPACE_SCHEMA_PREFIX: &str = "
CREATE TABLE IF NOT EXISTS workspace_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS default_categories (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    pattern TEXT NOT NULL UNIQUE,
    category_type TEXT NOT NULL DEFAULT 'files',
    description TEXT
);

CREATE TABLE IF NOT EXISTS default_category_policy (
    id INTEGER PRIMARY KEY,
    default_category_id INTEGER NOT NULL REFERENCES default_categories(id),
    protection_level TEXT NOT NULL DEFAULT 'editable',
    UNIQUE(default_category_id)
);
";

const WORKSPACE_SCHEMA_SUFFIX: &str = "
CREATE TABLE IF NOT EXISTS entity_links (
    id INTEGER PRIMARY KEY,
    entity_name TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    project_name TEXT NOT NULL,
    project_entity_id INTEGER,
    UNIQUE(entity_name, entity_type, project_name)
);
";

use std::sync::LazyLock;

pub static PROJECT_SCHEMA: LazyLock<String> =
    LazyLock::new(|| format!("{PROJECT_SCHEMA_PREFIX}{TOOL_TABLES_SCHEMA}{PROJECT_SCHEMA_SUFFIX}"));

pub static WORKSPACE_SCHEMA: LazyLock<String> = LazyLock::new(|| {
    format!("{WORKSPACE_SCHEMA_PREFIX}{TOOL_TABLES_SCHEMA}{WORKSPACE_SCHEMA_SUFFIX}")
});
