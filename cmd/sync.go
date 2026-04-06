package cmd

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/materialize"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/walk"
)

func RunSync(ctx *context.Context, args []string) error {
	fs := flag.NewFlagSet("sync", flag.ExitOnError)
	fs.Parse(args)

	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	categories, err := ctx.ProjectDb.ListCategories()
	if err != nil {
		return err
	}

	patterns, err := walk.CategoryPatterns(ctx.ProjectDb, nil)
	if err != nil {
		return err
	}

	entries, err := walk.WalkAndCollect(ctx.ProjectRoot, patterns)
	if err != nil {
		return err
	}

	var ingested, verified, modified, missing int

	allFiles, _ := ctx.ProjectDb.ListAllFiles()
	seen := make(map[string]bool)

	for _, relPath := range entries {
		absPath := filepath.Join(ctx.ProjectRoot, relPath)
		hash, fp, err := integrity.HashAndFingerprint(absPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "  ! %s: %v\n", relPath, err)
			continue
		}

		seen[hash] = true

		existing, _ := ctx.ProjectDb.GetFileByHash(hash)
		if existing == nil {
			file := &models.TrackedFile{
				SHA256:      hash,
				Fingerprint: fp.ToJSON(),
				IngestedAt:  time.Now().UTC().Format(time.RFC3339),
			}
			fileID, err := ctx.ProjectDb.InsertFile(file)
			if err != nil {
				fmt.Fprintf(os.Stderr, "  ! %s: %v\n", relPath, err)
				continue
			}
			_ = fileID

			matchingCats := matchingCategories(relPath, categories)
			materialize.MaterializeForFile(ctx.ProjectDb, relPath, hash, matchingCats, nil)

			fmt.Fprintf(os.Stderr, "  + %s\n", relPath)
			ingested++
		} else {
			fmt.Fprintf(os.Stderr, "  ok %s\n", relPath)
			verified++

			if existing.Fingerprint != fp.ToJSON() && existing.ID != nil {
				ctx.ProjectDb.UpdateFileFingerprint(*existing.ID, fp.ToJSON())
			}
		}
	}

	for _, f := range allFiles {
		if !seen[f.SHA256] {
			fmt.Fprintf(os.Stderr, "  ? [%s...]\n", f.SHA256[:10])
			missing++
		}
	}

	fmt.Fprintf(os.Stderr, "\nSync: %d new, %d verified, %d modified, %d missing\n",
		ingested, verified, modified, missing)

	if missing > 0 {
		return fmt.Errorf("integrity check failed: %d files missing", missing)
	}
	return nil
}

func matchingCategories(relPath string, categories []models.Scope) []models.Scope {
	var matched []models.Scope
	for _, cat := range categories {
		if ok, _ := cat.Matches(relPath); ok {
			matched = append(matched, cat)
		}
	}
	return matched
}
