pub mod categorize;
pub mod inbox;
pub mod ingest;
pub mod init;
pub mod list;
pub mod projects;
pub mod status;
pub mod tags;
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
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,
    },
    /// Safely view a file (respects protection level)
    View {
        /// File name
        name: String,
    },
    /// Edit a file (refused for immutable categories)
    Edit {
        /// File name
        name: String,
    },
    /// Check integrity hashes
    Verify {
        /// Specific file to verify (all files if omitted)
        name: Option<String>,
    },
    /// Move a file to a different category
    Categorize {
        /// File name
        name: String,
        /// Target category path
        category: String,
    },
    /// Add a tag to a file
    Tag {
        /// File name
        name: String,
        /// Tag to add
        tag: String,
    },
    /// Remove a tag from a file
    Untag {
        /// File name
        name: String,
        /// Tag to remove
        tag: String,
    },
    /// List tags (for a file, or all tags in scope)
    Tags {
        /// Specific file name
        name: Option<String>,
    },
    /// Workspace inbox operations
    Inbox {
        #[command(subcommand)]
        command: Option<InboxCommands>,
    },
    /// List all projects in the workspace
    Projects,
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
