pub const PROJECT_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY,
    pattern TEXT NOT NULL UNIQUE,
    protection_level TEXT NOT NULL,
    description TEXT
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
    PRIMARY KEY (file_id, tag)
);

CREATE TABLE IF NOT EXISTS tool_config (
    id INTEGER PRIMARY KEY,
    scope TEXT,
    action TEXT NOT NULL,
    file_type TEXT NOT NULL,
    command TEXT NOT NULL,
    env TEXT
);

CREATE TABLE IF NOT EXISTS tag_tool_config (
    id INTEGER PRIMARY KEY,
    tag TEXT NOT NULL,
    action TEXT NOT NULL,
    file_type TEXT NOT NULL,
    command TEXT NOT NULL,
    env TEXT,
    UNIQUE(tag, action, file_type)
);

CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    operation TEXT NOT NULL,
    file_id INTEGER REFERENCES files(id),
    user TEXT,
    detail TEXT
);
";

pub const WORKSPACE_SCHEMA: &str = "
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
    pattern TEXT NOT NULL UNIQUE,
    protection_level TEXT NOT NULL,
    description TEXT
);

CREATE TABLE IF NOT EXISTS tool_config (
    id INTEGER PRIMARY KEY,
    scope TEXT,
    action TEXT NOT NULL,
    file_type TEXT NOT NULL,
    command TEXT NOT NULL,
    env TEXT
);

CREATE TABLE IF NOT EXISTS tag_tool_config (
    id INTEGER PRIMARY KEY,
    tag TEXT NOT NULL,
    action TEXT NOT NULL,
    file_type TEXT NOT NULL,
    command TEXT NOT NULL,
    env TEXT,
    UNIQUE(tag, action, file_type)
);

CREATE TABLE IF NOT EXISTS entity_links (
    id INTEGER PRIMARY KEY,
    entity_name TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    project_name TEXT NOT NULL,
    project_entity_id INTEGER,
    UNIQUE(entity_name, entity_type, project_name)
);
";
