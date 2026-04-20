package reference

// ReferenceKind distinguishes workspace, context, and bare path references.
type ReferenceKind int

const (
	KindWorkspace ReferenceKind = iota // : prefix — widest available scope
	KindContext                        // . prefix or bare — current context
	KindBarePath                       // ./ or ../ — literal filesystem path
)

// Reference is a parsed query in the muckrake reference language.
type Reference struct {
	Kind ReferenceKind
	// WorkspaceWide is true for refs of the form ":.name" — a workspace-wide
	// scope, as opposed to ":name" which names a specific project.
	// Meaningful only when Kind == KindWorkspace.
	WorkspaceWide bool
	Scope         []ScopeLevel
	Tags          []TagFilter
	Glob          *string
	Raw           string // original input for bare paths
}

// ScopeLevel is one dot-separated level in a reference.
// Names has multiple entries when brace expansion is used: {a,b}.
type ScopeLevel struct {
	Names []string
}

// TagFilter is a ! group. Tags within one filter use OR logic.
// Multiple TagFilters use AND logic.
type TagFilter struct {
	Tags []string
}
