pub mod categorize;
pub mod category;
pub mod inbox;
pub mod ingest;
pub mod init;
pub mod list;
pub mod pipeline;
pub mod projects;
pub mod read;
pub mod rule;
pub mod scope;
pub mod sign;
pub mod status;
pub mod tags;
pub mod tool;
pub mod verify;
pub mod view;

use std::path::Path;

use clap::{Parser, Subcommand};

use crate::models::Category;

pub(crate) fn create_category_dir(project_root: &Path, pattern: &str) {
    let base = Category::name_from_pattern(pattern);
    if base.is_empty() || base == "**" || base == "*" {
        return;
    }
    let dir = project_root.join(&base);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("  warning: could not create {}: {e}", dir.display());
    }
}

#[derive(Parser)]
#[command(
    name = "mkrk",
    about = "Investigative journalism research management",
    version,
    after_long_help = "\
SCOPE PREFIX:
    Commands can be scoped to a specific project or the entire workspace
    by placing a ':'-prefixed argument before the subcommand:

    mkrk :<project> <command>    Run command in a different project
    mkrk : <command>             Run command across entire workspace

    Examples:
      mkrk :bailey list            List files in project \"bailey\"
      mkrk :bailey list :evidence  List evidence in project \"bailey\"
      mkrk : verify                Verify all projects in workspace

REFERENCES:
    Many commands accept structured references starting with ':'.
    These select files by category, project, tags, and globs:

    :category                    Files in a category
    :project.category            Cross-project reference
    :category.subcategory        Nested category path
    :{a,b}.{c,d}                 Brace expansion (cartesian product)
    :scope!tag                   Filter by tag
    :scope!t1,t2                 OR within a tag group
    :scope!t1!t2                 AND across tag groups
    :scope/*.pdf                 Glob filter on filenames
    :                            All files in current scope
    :!tag                        All files matching a tag
    :/*.pdf                      All files matching a glob"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Clone, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Commands {
    /// Create a new project or workspace
    Init {
        /// Project name (creates directory; required inside a workspace)
        name: Option<String>,
        /// Create a workspace instead of a project; value is the projects directory
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
        /// Create an inbox directory (workspace only)
        #[arg(long)]
        inbox: bool,
        /// Don't create default categories
        #[arg(short = 'n', long = "no-categories")]
        no_categories: bool,
        /// Define custom categories (pattern:level or pattern:type:level), implies --no-categories
        #[arg(long = "category")]
        categories: Vec<String>,
    },
    /// Show current context and project status
    Status,
    /// Track untracked files in the project (scans filesystem)
    Ingest {
        /// Scope to scan (e.g., evidence, evidence.emails); omit to scan entire project
        scope: Option<String>,
    },
    /// List files in the current scope
    List {
        /// References to list (defaults to current project)
        references: Vec<String>,
        /// Skip fingerprint verification on tag-filtered listings
        #[arg(long)]
        no_hash_check: bool,
    },
    /// Read file contents by reference
    Read {
        /// References to read (at least one required)
        #[arg(required = true)]
        references: Vec<String>,
        /// Show file path before content
        #[arg(long)]
        path: bool,
        /// Show query reference before content
        #[arg(long)]
        query: bool,
        /// Disable all decoration and color
        #[arg(long)]
        raw: bool,
    },
    /// Safely view a file (respects protection level)
    View {
        /// File reference
        reference: String,
    },
    /// Edit a file (refused for immutable categories)
    Edit {
        /// File reference
        reference: String,
    },
    /// Check integrity hashes
    Verify {
        /// File reference (all files in current project if omitted)
        reference: Option<String>,
    },
    /// Move a file to a different category
    Categorize {
        /// File reference
        reference: String,
        /// Target category path
        category: String,
    },
    /// Add a tag to a file
    Tag {
        /// File reference
        reference: String,
        /// Tag to add
        tag: String,
    },
    /// Remove a tag from a file
    Untag {
        /// File reference
        reference: String,
        /// Tag to remove
        tag: String,
    },
    /// List tags (for a file, or all tags in scope)
    Tags {
        /// File reference
        reference: Option<String>,
        /// Skip hash verification (faster, but won't detect stale tags)
        #[arg(long)]
        no_hash_check: bool,
    },
    /// Workspace inbox operations (lists inbox when run without subcommand)
    Inbox {
        #[command(subcommand)]
        command: Option<InboxCommands>,
    },
    /// List all projects in the workspace
    Projects,
    /// Run a project-local tool
    #[command(alias = "t")]
    Tool {
        /// Tool name or :project.name reference
        name: String,
        /// Arguments to pass to the tool
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Manage project categories
    #[command(alias = "cat")]
    Category {
        #[command(subcommand)]
        command: Option<CategoryCommands>,
    },
    /// Manage event-driven rules
    Rule {
        #[command(subcommand)]
        command: RuleCommands,
    },
    /// Manage pipelines (state machines for file workflows)
    Pipeline {
        #[command(subcommand)]
        command: PipelineCommands,
    },
    /// Attest a file has reached a pipeline stage
    Sign {
        /// File reference
        reference: String,
        /// Sign name (must match a pipeline transition requirement)
        sign_name: String,
        /// Pipeline name (required if file is in multiple pipelines)
        #[arg(long)]
        pipeline: Option<String>,
        /// Create a detached GPG signature
        #[arg(long)]
        gpg: bool,
    },
    /// Revoke a sign (attestation) on a file
    Unsign {
        /// File reference
        reference: String,
        /// Sign name to revoke
        sign_name: String,
        /// Pipeline name (required if file is in multiple pipelines)
        #[arg(long)]
        pipeline: Option<String>,
    },
    /// List signs (attestations) for files
    Signs {
        /// File reference (all files if omitted)
        reference: Option<String>,
    },
    /// Show pipeline state for files
    State {
        /// File reference (all files if omitted)
        reference: Option<String>,
        /// Filter to a specific pipeline
        #[arg(long)]
        pipeline: Option<String>,
    },
}

#[derive(Clone, Subcommand)]
pub enum CategoryCommands {
    /// Add a new category
    Add {
        /// Category name (e.g., evidence)
        name: String,
        /// Glob pattern (defaults to name/**)
        #[arg(long)]
        pattern: Option<String>,
        /// Category type
        #[arg(long = "type", default_value = "files")]
        category_type: String,
        /// Protection level
        #[arg(long, default_value = "editable")]
        protection: String,
        /// Description
        #[arg(long)]
        description: Option<String>,
    },
    /// Update an existing category
    Update {
        /// Category name
        name: String,
        /// New pattern
        #[arg(long)]
        pattern: Option<String>,
        /// New protection level
        #[arg(long)]
        protection: Option<String>,
    },
    /// Remove a category
    Remove {
        /// Category name
        name: String,
    },
}

#[derive(Clone, Subcommand)]
pub enum InboxCommands {
    /// Assign an inbox file to a project
    Assign {
        /// File name in inbox
        file: String,
        /// Target project name
        project: String,
        /// Target category path
        #[arg(long = "as")]
        category: Option<String>,
    },
}

#[derive(Clone, Subcommand)]
pub enum PipelineCommands {
    /// Add a new pipeline
    Add {
        /// Pipeline name
        name: String,
        /// Comma-separated states (e.g., draft,review,published)
        #[arg(long)]
        states: String,
        /// Custom transitions as JSON (auto-generated if omitted)
        #[arg(long)]
        transitions: Option<String>,
    },
    /// List all pipelines
    List,
    /// Remove a pipeline (also removes its signs and attachments)
    Remove {
        /// Pipeline name
        name: String,
    },
    /// Attach a pipeline to a category or tag
    Attach {
        /// Pipeline name
        pipeline: String,
        /// Category to attach to
        #[arg(long)]
        category: Option<String>,
        /// Tag to attach to
        #[arg(long)]
        tag: Option<String>,
    },
    /// Detach a pipeline from a category or tag
    Detach {
        /// Pipeline name
        pipeline: String,
        /// Category to detach from
        #[arg(long)]
        category: Option<String>,
        /// Tag to detach from
        #[arg(long)]
        tag: Option<String>,
    },
}

#[derive(Clone, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum RuleCommands {
    /// Add a new rule
    Add {
        /// Rule name (unique identifier)
        name: String,
        /// Trigger event: ingest, tag, untag, categorize, sign, state-change, project-enter, workspace-enter
        #[arg(long)]
        on: String,
        /// Action type: run-tool, add-tag, remove-tag, sign, unsign, attach-pipeline, detach-pipeline
        #[arg(long)]
        action: String,
        /// Tool name (required for run-tool action)
        #[arg(long)]
        tool: Option<String>,
        /// Tag name (for add-tag/remove-tag action, or attachment scope for attach/detach-pipeline)
        #[arg(long)]
        tag: Option<String>,
        /// Category filter / attachment scope for attach/detach-pipeline
        #[arg(long)]
        category: Option<String>,
        /// MIME type filter (e.g., application/pdf, image/*)
        #[arg(long)]
        mime_type: Option<String>,
        /// File extension filter (e.g., pdf, wav)
        #[arg(long = "file-type")]
        file_type: Option<String>,
        /// Tag name that triggers this rule (for tag/untag events)
        #[arg(long = "trigger-tag")]
        trigger_tag: Option<String>,
        /// Pipeline name filter (for sign/state-change triggers)
        #[arg(long = "trigger-pipeline")]
        trigger_pipeline: Option<String>,
        /// Sign name filter (for sign triggers)
        #[arg(long = "trigger-sign")]
        trigger_sign: Option<String>,
        /// Target state filter (for state-change triggers)
        #[arg(long = "trigger-state")]
        trigger_state: Option<String>,
        /// Pipeline name (for sign/unsign/attach-pipeline/detach-pipeline actions)
        #[arg(long)]
        pipeline: Option<String>,
        /// Sign name (for sign/unsign actions)
        #[arg(long = "sign-name")]
        sign_name: Option<String>,
        /// Priority (lower fires first, default 0)
        #[arg(long, default_value = "0")]
        priority: i32,
    },
    /// List all rules
    List,
    /// Remove a rule
    Remove {
        /// Rule name
        name: String,
    },
    /// Enable a disabled rule
    Enable {
        /// Rule name
        name: String,
    },
    /// Disable a rule without removing it
    Disable {
        /// Rule name
        name: String,
    },
}
