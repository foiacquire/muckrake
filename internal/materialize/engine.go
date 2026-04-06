package materialize

import (
	"go.foia.dev/muckrake/internal/db"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/reference"
)

// MaterializeForFile checks all pipeline and ruleset subscriptions against
// a file's known metadata and creates per-hash materialized records.
func MaterializeForFile(pdb *db.ProjectDb, relPath, sha256 string, matchingCats []models.Scope, tags []string) {
	materializePipelines(pdb, relPath, sha256, matchingCats, tags)
	materializeRulesets(pdb, relPath, sha256, matchingCats, tags)
}

func materializePipelines(pdb *db.ProjectDb, relPath, sha256 string, cats []models.Scope, tags []string) {
	subs, err := pdb.ListAllPipelineSubscriptions()
	if err != nil {
		return
	}
	for _, ps := range subs {
		ref, err := reference.ParseReference(ps.Sub.Reference)
		if err != nil {
			continue
		}
		if matchesReference(ref, relPath, cats, tags) {
			subID := int64(0)
			if ps.Sub.ID != nil {
				subID = *ps.Sub.ID
			}
			pdb.MaterializePipelineFile(ps.PipelineID, sha256, subID)
		}
	}
}

func materializeRulesets(pdb *db.ProjectDb, relPath, sha256 string, cats []models.Scope, tags []string) {
	subs, err := pdb.ListAllRulesetSubscriptions()
	if err != nil {
		return
	}
	for _, rs := range subs {
		ref, err := reference.ParseReference(rs.Sub.Reference)
		if err != nil {
			continue
		}
		if matchesReference(ref, relPath, cats, tags) {
			subID := int64(0)
			if rs.Sub.ID != nil {
				subID = *rs.Sub.ID
			}
			pdb.MaterializeRulesetFile(rs.RulesetID, sha256, subID)
		}
	}
}

func matchesReference(ref *reference.Reference, relPath string, cats []models.Scope, tags []string) bool {
	if ref.Kind == reference.KindBarePath {
		return false
	}
	return matchesScope(ref.Scope, cats) &&
		matchesTags(ref.Tags, tags) &&
		matchesGlob(ref.Glob, relPath)
}

func matchesScope(scope []reference.ScopeLevel, cats []models.Scope) bool {
	if len(scope) == 0 {
		return true
	}
	level := &scope[0]
	for _, name := range level.Names {
		for _, cat := range cats {
			if cat.Name == name {
				return true
			}
		}
	}
	return false
}

func matchesTags(filters []reference.TagFilter, fileTags []string) bool {
	for _, filter := range filters {
		if len(filter.Tags) == 0 {
			continue
		}
		groupMatch := false
		for _, t := range filter.Tags {
			for _, ft := range fileTags {
				if t == ft {
					groupMatch = true
					break
				}
			}
			if groupMatch {
				break
			}
		}
		if !groupMatch {
			return false
		}
	}
	return true
}

func matchesGlob(glob *string, relPath string) bool {
	if glob == nil {
		return true
	}
	pattern := *glob
	fileName := relPath
	if idx := lastSlash(relPath); idx >= 0 {
		fileName = relPath[idx+1:]
	}
	if ok, _ := models.GlobMatch(pattern, fileName); ok {
		return true
	}
	if ok, _ := models.GlobMatch(pattern, relPath); ok {
		return true
	}
	return false
}

func lastSlash(s string) int {
	for i := len(s) - 1; i >= 0; i-- {
		if s[i] == '/' {
			return i
		}
	}
	return -1
}
