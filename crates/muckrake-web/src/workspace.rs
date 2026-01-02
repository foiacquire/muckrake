use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Current workspace file format version
pub const WORKSPACE_VERSION: u32 = 1;

/// Project access mode within a workspace
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProjectMode {
    #[default]
    ReadWrite,
    ReadOnly,
}

/// Reference to a project within a workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRef {
    /// Path to the project file (relative to workspace file)
    pub path: String,
    /// Access mode
    #[serde(default)]
    pub mode: ProjectMode,
    /// Optional alias for cross-project queries
    pub alias: Option<String>,
}

/// Directory structure configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceStructure {
    #[serde(default = "default_entities_dir")]
    pub entities_dir: String,
    #[serde(default = "default_sources_dir")]
    pub sources_dir: String,
    #[serde(default = "default_analysis_dir")]
    pub analysis_dir: String,
    #[serde(default = "default_exports_dir")]
    pub exports_dir: String,
}

fn default_entities_dir() -> String { "entities".to_string() }
fn default_sources_dir() -> String { "sources".to_string() }
fn default_analysis_dir() -> String { "analysis".to_string() }
fn default_exports_dir() -> String { "exports".to_string() }

/// Workspace configuration stored in .mkspc files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub projects: Vec<ProjectRef>,
    #[serde(default)]
    pub structure: WorkspaceStructure,
}

impl WorkspaceConfig {
    pub fn new(name: String) -> Self {
        Self {
            version: WORKSPACE_VERSION,
            name,
            projects: Vec::new(),
            structure: WorkspaceStructure::default(),
        }
    }

    pub fn add_project(&mut self, path: String, mode: ProjectMode, alias: Option<String>) {
        self.projects.push(ProjectRef { path, mode, alias });
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Runtime workspace state
pub struct Workspace {
    pub config: WorkspaceConfig,
    pub file_path: Option<PathBuf>,
}

impl Workspace {
    pub fn new(name: String) -> Self {
        Self {
            config: WorkspaceConfig::new(name),
            file_path: None,
        }
    }

    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let config = WorkspaceConfig::load(path)?;
        Ok(Self {
            config,
            file_path: Some(path.to_path_buf()),
        })
    }

    pub fn save(&self) -> anyhow::Result<PathBuf> {
        let path = self.file_path.clone().unwrap_or_else(|| {
            let workspaces_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("muckrake")
                .join("workspaces");
            let safe_name: String = self.config.name
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                .collect();
            workspaces_dir.join(format!("{}.mkspc", safe_name))
        });

        self.config.save(&path)?;
        Ok(path)
    }

    pub fn save_to(&mut self, path: &Path) -> anyhow::Result<()> {
        self.config.save(path)?;
        self.file_path = Some(path.to_path_buf());
        Ok(())
    }
}
