package resolve

import (
	"fmt"
	"path/filepath"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/reference"
	"go.foia.dev/muckrake/internal/walk"
)

// Ref returns absolute paths of files matching the given reference string
// within the current project context. An empty slice means no matches.
func Ref(ctx *context.Context, raw string) ([]string, error) {
	rels, err := RefRelPaths(ctx, raw)
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

// RefRelPaths returns matching paths relative to the project root. Bare
// paths (e.g. ./foo or ../foo) pass through unchanged.
func RefRelPaths(ctx *context.Context, raw string) ([]string, error) {
	ref, err := reference.ParseReference(raw)
	if err != nil {
		return nil, err
	}
	return FromReference(ctx, ref)
}

// FromReference runs the resolver on an already-parsed reference. This is
// the primitive shared by RefRelPaths (string input) and SubjectFiles
// (Context.Subject input), so tags and globs are handled the same way for
// both paths.
func FromReference(ctx *context.Context, ref *reference.Reference) ([]string, error) {
	if ref.Kind == reference.KindBarePath {
		return []string{ref.Raw}, nil
	}

	if ctx.Kind != context.ContextProject {
		return nil, fmt.Errorf("cannot resolve reference outside a project")
	}

	patterns, err := patternsForRef(ctx, ref)
	if err != nil {
		return nil, err
	}
	entries, err := walk.WalkAndCollect(ctx.ProjectRoot, patterns)
	if err != nil {
		return nil, err
	}

	var rels []string
	for _, relPath := range entries {
		if !globMatches(ref, relPath) {
			continue
		}
		if !tagFiltersMatch(ctx, relPath, ref.Tags) {
			continue
		}
		rels = append(rels, relPath)
	}
	return rels, nil
}

// tagFiltersMatch reports whether the file at relPath passes every TagFilter.
// Tags within a single filter are OR'd; filters are AND'd. Untracked files
// have no tags, so any present filter rejects them.
func tagFiltersMatch(ctx *context.Context, relPath string, filters []reference.TagFilter) bool {
	if len(filters) == 0 {
		return true
	}
	absPath := filepath.Join(ctx.ProjectRoot, relPath)
	hash, err := integrity.HashFile(absPath)
	if err != nil {
		return false
	}
	file, err := ctx.ProjectDb.GetFileByHash(hash)
	if err != nil || file == nil || file.ID == nil {
		return false
	}
	tags, err := ctx.ProjectDb.GetTags(*file.ID)
	if err != nil {
		return false
	}
	tagSet := make(map[string]bool, len(tags))
	for _, t := range tags {
		tagSet[t] = true
	}
	for _, group := range filters {
		if !anyTagMatch(group.Tags, tagSet) {
			return false
		}
	}
	return true
}

func anyTagMatch(candidates []string, have map[string]bool) bool {
	for _, t := range candidates {
		if have[t] {
			return true
		}
	}
	return false
}

func patternsForRef(ctx *context.Context, ref *reference.Reference) ([]string, error) {
	if len(ref.Scope) == 0 {
		return []string{"**"}, nil
	}
	catName := ref.Scope[0].Names[0]
	return walk.CategoryPatterns(ctx.ProjectDb, &catName)
}

func globMatches(ref *reference.Reference, relPath string) bool {
	if ref.Glob == nil {
		return true
	}
	fileName := filepath.Base(relPath)
	mf, _ := models.GlobMatch(*ref.Glob, fileName)
	mp, _ := models.GlobMatch(*ref.Glob, relPath)
	return mf || mp
}
