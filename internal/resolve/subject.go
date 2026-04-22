package resolve

import (
	"path/filepath"
	"strings"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/reference"
)

// HasNarrowSubject reports whether ctx.Subject names a scope narrower than
// the project itself (i.e., a category or file within the project).
func HasNarrowSubject(ctx *context.Context) bool {
	if ctx == nil || ctx.Subject == nil {
		return false
	}
	return narrowSubject(ctx.Subject) != ""
}

// SubjectFiles resolves ctx.Subject into absolute file paths. Returns nil
// when no subject is set or when the subject does not narrow past the
// project level (in which case the command should operate on the whole
// project).
func SubjectFiles(ctx *context.Context) ([]string, error) {
	if ctx == nil || ctx.Subject == nil {
		return nil, nil
	}
	sub := narrowSubject(ctx.Subject)
	if sub == "" {
		return nil, nil
	}
	return Ref(ctx, sub)
}

// SubjectRelPaths is SubjectFiles returning paths relative to ProjectRoot.
func SubjectRelPaths(ctx *context.Context) ([]string, error) {
	paths, err := SubjectFiles(ctx)
	if err != nil {
		return nil, err
	}
	var rels []string
	for _, p := range paths {
		rel, err := filepath.Rel(ctx.ProjectRoot, p)
		if err != nil {
			continue
		}
		rels = append(rels, rel)
	}
	return rels, nil
}

// narrowSubject turns a subject reference into a resolver-consumable string
// representing the portion of the ref narrower than the project level.
// Returns empty string when the ref only names a project (or is bare ":").
func narrowSubject(r *reference.Reference) string {
	if r == nil {
		return ""
	}
	switch r.Kind {
	case reference.KindWorkspace:
		if r.WorkspaceWide {
			return joinScopeLevels(r.Scope)
		}
		if len(r.Scope) > 1 {
			return joinScopeLevels(r.Scope[1:])
		}
		return ""
	case reference.KindContext:
		return joinScopeLevels(r.Scope)
	case reference.KindBarePath:
		return r.Raw
	}
	return ""
}

func joinScopeLevels(levels []reference.ScopeLevel) string {
	var parts []string
	for _, l := range levels {
		if len(l.Names) == 0 {
			continue
		}
		parts = append(parts, l.Names[0])
	}
	return strings.Join(parts, ".")
}
