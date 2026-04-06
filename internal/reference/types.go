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
	Kind  ReferenceKind
	Scope []ScopeLevel
	Tags  []TagFilter
	Glob  *string
	Raw   string // original input for bare paths
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
