package reference

import "go.foia.dev/muckrake/internal/models"

// GlobMatchFile checks if a glob pattern matches a filename or full path.
func GlobMatchFile(pattern, fileName, relPath string) (bool, error) {
	if ok, err := models.GlobMatch(pattern, fileName); ok || err != nil {
		return ok, err
	}
	return models.GlobMatch(pattern, relPath)
}
