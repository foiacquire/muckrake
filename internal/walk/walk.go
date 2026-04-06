package walk

import (
	"os"
	"path/filepath"
	"sort"
	"strings"

	"go.foia.dev/muckrake/internal/db"
	"go.foia.dev/muckrake/internal/models"
)

// WalkAndCollect walks root, skipping dot-prefixed entries, and returns
// relative paths that match at least one of the given glob patterns.
func WalkAndCollect(root string, patterns []string) ([]string, error) {
	var entries []string
	err := walkRecursive(root, root, patterns, &entries)
	if err != nil {
		return nil, err
	}
	sort.Strings(entries)
	return entries, nil
}

func walkRecursive(root, dir string, patterns []string, entries *[]string) error {
	dirEntries, err := os.ReadDir(dir)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}

	for _, entry := range dirEntries {
		name := entry.Name()
		if strings.HasPrefix(name, ".") {
			continue
		}

		path := filepath.Join(dir, name)
		if entry.IsDir() {
			if err := walkRecursive(root, path, patterns, entries); err != nil {
				return err
			}
			continue
		}

		rel, err := filepath.Rel(root, path)
		if err != nil {
			continue
		}
		rel = filepath.ToSlash(rel)

		for _, pattern := range patterns {
			matched, _ := models.GlobMatch(pattern, rel)
			if matched {
				*entries = append(*entries, rel)
				break
			}
		}
	}

	return nil
}

// CategoryPatterns builds glob patterns for a given category name.
// If name is nil, returns ["**"] to match everything.
func CategoryPatterns(pdb *db.ProjectDb, categoryName *string) ([]string, error) {
	if categoryName == nil {
		return []string{"**"}, nil
	}

	cat, err := pdb.GetCategoryByName(*categoryName)
	if err != nil {
		return nil, err
	}
	if cat != nil && cat.Pattern != nil {
		base := models.NameFromPattern(*cat.Pattern)
		return []string{base + "/*", base + "/**/*"}, nil
	}

	// Treat as subcategory path prefix
	name := *categoryName
	return []string{name + "/*", name + "/**/*"}, nil
}
