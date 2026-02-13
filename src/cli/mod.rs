pub mod categorize;
pub mod category;
pub mod inbox;
pub mod ingest;
pub mod init;
pub mod list;
pub mod projects;
pub mod scope;
pub mod status;
pub mod tags;
pub mod tool;
pub mod verify;
pub mod view;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "mkrk",
    about = "Investigative journalism research management",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Clone, Subcommand)]
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
    /// Run or manage project-local tools
    #[command(alias = "t")]
    Tool {
        #[command(subcommand)]
        command: ToolCommands,
    },
    /// Manage project categories
    #[command(alias = "cat")]
    Category {
        #[command(subcommand)]
        command: Option<CategoryCommands>,
    },
}

#[derive(Clone, Subcommand)]
pub enum ToolCommands {
    /// Register a tool in the database
    Add {
        /// Tool name (action name: ocr, transcribe, etc.)
        name: String,
        /// Command to execute
        command: String,
        /// Category scope (e.g., evidence, evidence/financial)
        #[arg(long)]
        scope: Option<String>,
        /// File type filter (e.g., wav, pdf, image/*)
        #[arg(long = "file-type", default_value = "*")]
        file_type: String,
        /// Tag to scope this config to (uses `tag_tool_config` instead)
        #[arg(long)]
        tag: Option<String>,
        /// JSON env var overrides
        #[arg(long)]
        env: Option<String>,
        /// Show command when running (default is quiet)
        #[arg(long)]
        verbose: bool,
    },
    /// List registered tools
    List,
    /// Remove a tool configuration
    Remove {
        /// Tool name
        name: String,
        /// Category scope to match
        #[arg(long)]
        scope: Option<String>,
        /// File type to match
        #[arg(long = "file-type")]
        file_type: Option<String>,
        /// Tag to match (removes from `tag_tool_config`)
        #[arg(long)]
        tag: Option<String>,
    },
    /// Run a tool by name (default when no subcommand matches)
    #[command(external_subcommand)]
    Run(Vec<String>),
}

#[derive(Clone, Subcommand)]
pub enum CategoryCommands {
    /// Add a new category
    Add {
        /// Pattern (e.g., evidence/**)
        pattern: String,
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
        /// Current pattern to match
        current: String,
        /// New pattern
        #[arg(long)]
        pattern: Option<String>,
        /// New protection level
        #[arg(long)]
        protection: Option<String>,
    },
    /// Remove a category
    Remove {
        /// Pattern to remove
        pattern: String,
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
