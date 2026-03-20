#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// `:` prefixed — workspace scope (or widest available scope).
    Workspace {
        scope: Vec<ScopeLevel>,
        tags: Vec<TagFilter>,
        glob: Option<String>,
    },
    /// `.` prefixed or bare scope name — resolves against current context.
    Context {
        scope: Vec<ScopeLevel>,
        tags: Vec<TagFilter>,
        glob: Option<String>,
    },
    /// Raw filesystem path (contains `/` but no scope syntax).
    BarePath(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeLevel {
    pub names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagFilter {
    pub tags: Vec<String>,
}
