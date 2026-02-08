pub mod categorize;
pub mod inbox;
pub mod ingest;
pub mod init;
pub mod list;
pub mod projects;
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

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new project or workspace
    Init {
        /// Create a workspace instead of a project; value is the projects directory
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
        /// Create an inbox directory (workspace only)
        #[arg(long)]
        inbox: bool,
        /// Don't create default categories
        #[arg(short = 'n', long = "no-categories")]
        no_categories: bool,
        /// Define custom categories (pattern:level), implies --no-categories
        #[arg(long = "category")]
        categories: Vec<String>,
    },
    /// Show current context and project status
    Status,
    /// Import files with integrity tracking
    Ingest {
        /// File path(s) to ingest
        paths: Vec<String>,
        /// Target category path (e.g., evidence/financial)
        #[arg(long = "as")]
        category: Option<String>,
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
    /// Workspace inbox operations
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
}

#[derive(Subcommand)]
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

#[derive(Subcommand)]
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
