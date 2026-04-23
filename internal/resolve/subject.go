package resolve

import (
	"path/filepath"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/reference"
)

// HasNarrowSubject reports whether ctx.Subject names something narrower
// than the project itself (a category, file, tag filter, or glob).
func HasNarrowSubject(ctx *context.Context) bool {
	if ctx == nil || ctx.Subject == nil {
		return false
	}
	return !isProjectOnly(ctx.Subject)
}

// SubjectFiles resolves ctx.Subject into absolute file paths. Returns nil
// when the subject is absent or only names a project (command should
// operate on the whole project).
func SubjectFiles(ctx *context.Context) ([]string, error) {
	rels, err := SubjectRelPaths(ctx)
	if err != nil {
		return nil, err
	}
	var paths []string
	for _, rel := range rels {
		if filepath.IsAbs(rel) {
			paths = append(paths, rel)
			continue
		}
		paths = append(paths, filepath.Join(ctx.ProjectRoot, rel))
	}
	return paths, nil
}

// SubjectRelPaths is SubjectFiles returning paths relative to ProjectRoot.
func SubjectRelPaths(ctx *context.Context) ([]string, error) {
	if ctx == nil || ctx.Subject == nil {
		return nil, nil
	}
	if isProjectOnly(ctx.Subject) {
		return nil, nil
	}
	narrowed := narrowReference(ctx.Subject)
	return FromReference(ctx, narrowed)
}

// isProjectOnly reports whether a subject names only a project (or the
// workspace root), with no scope/tag/glob narrower than that.
func isProjectOnly(r *reference.Reference) bool {
	if r == nil {
		return true
	}
	if len(r.Tags) > 0 || r.Glob != nil {
		return false
	}
	switch r.Kind {
	case reference.KindWorkspace:
		if r.WorkspaceWide {
			return len(r.Scope) == 0
		}
		return len(r.Scope) <= 1
	case reference.KindContext:
		return len(r.Scope) == 0
	}
	return true
}

// narrowReference returns a copy of the subject with the project prefix
// stripped, so the resolver sees just the project-local scope/tags/glob.
func narrowReference(r *reference.Reference) *reference.Reference {
	narrowed := *r
	if r.Kind == reference.KindWorkspace {
		if !r.WorkspaceWide && len(r.Scope) > 0 {
			narrowed.Scope = r.Scope[1:]
		}
		narrowed.Kind = reference.KindContext
		narrowed.WorkspaceWide = false
	}
	return &narrowed
}
