package reference

import (
	"strings"

	"go.foia.dev/muckrake/internal/db"
	"go.foia.dev/muckrake/internal/models"
)

// FormatRef converts a filesystem relative path into a canonical reference
// string. If projectName is non-empty, output is workspace-scoped.
func FormatRef(relPath string, projectName string, pdb *db.ProjectDb) string {
	cat, _ := pdb.MatchCategory(relPath)

	if cat != nil && cat.Pattern != nil {
		base := models.NameFromPattern(*cat.Pattern)
		relative := relPath
		if after, ok := strings.CutPrefix(relPath, base+"/"); ok {
			relative = after
		}
		body := formatScoped(cat.Name, relative)
		if projectName != "" {
			return ":" + projectName + "." + body
		}
		return body
	}

	// Uncategorized
	dirs, filename := splitDirsAndFilename(relPath)
	sep := filenameSeparator(filename)
	if projectName != "" {
		if dirs != "" {
			return ":" + projectName + "." + dirs + sep + filename
		}
		return ":" + projectName + sep + filename
	}
	if dirs != "" {
		return dirs + sep + filename
	}
	return relPath
}

func formatScoped(category, relative string) string {
	dirs, filename := splitDirsAndFilename(relative)
	sep := filenameSeparator(filename)
	if dirs != "" {
		return category + "." + dirs + sep + filename
	}
	return category + sep + filename
}

func splitDirsAndFilename(relPath string) (string, string) {
	idx := strings.LastIndexByte(relPath, '/')
	if idx < 0 {
		return "", relPath
	}
	dotted := strings.ReplaceAll(relPath[:idx], "/", ".")
	return dotted, relPath[idx+1:]
}

func filenameSeparator(filename string) string {
	if strings.ContainsRune(filename, '.') {
		return "/"
	}
	return "."
}
