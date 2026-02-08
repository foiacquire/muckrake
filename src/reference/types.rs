#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    Structured {
        scope: Vec<ScopeLevel>,
        tags: Vec<TagFilter>,
        glob: Option<String>,
    },
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
