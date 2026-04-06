package cmd

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"golang.org/x/term"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/materialize"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/reference"
	"go.foia.dev/muckrake/internal/walk"
)

type syncConflict struct {
	relPath   string
	ref       string
	diskFp    *integrity.Fingerprint
	diskHash  string
	matchFile *models.TrackedFile
}

type syncCounts struct {
	ok       int
	ingested int
	modified int
	missing  int
	other    int
}

func RunSync(ctx *context.Context, args []string) error {
	fs := flag.NewFlagSet("sync", flag.ExitOnError)
	quiet := fs.Bool("quiet", false, "non-interactive, exit with conflict count")
	fs.BoolVar(quiet, "q", false, "shorthand for --quiet")
	fs.Parse(args)

	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	interactive := !*quiet && term.IsTerminal(int(os.Stdin.Fd()))

	projectName := ""
	if ctx.ProjectName != nil {
		projectName = *ctx.ProjectName
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

	allFiles, _ := ctx.ProjectDb.ListAllFiles()
	seen := make(map[string]bool)

	var counts syncCounts
	var conflicts []syncConflict

	for _, relPath := range entries {
		absPath := filepath.Join(ctx.ProjectRoot, relPath)
		hash, fp, err := integrity.HashAndFingerprint(absPath)
		if err != nil {
			ref := reference.FormatRef(relPath, projectName, ctx.ProjectDb)
			fmt.Fprintf(os.Stderr, "  \033[31m✗\033[0m %s: %v\n", ref, err)
			continue
		}

		ref := reference.FormatRef(relPath, projectName, ctx.ProjectDb)

		// Exact fingerprint match
		if file, _ := ctx.ProjectDb.GetFileByFingerprint(fp.ToJSON()); file != nil {
			seen[file.SHA256] = true
			fmt.Fprintf(os.Stderr, "  \033[32m✓\033[0m %s\n", ref)
			counts.ok++
			continue
		}

		// Hash match — tracked but fingerprint stale, update it
		if file, _ := ctx.ProjectDb.GetFileByHash(hash); file != nil {
			seen[hash] = true
			if file.ID != nil {
				ctx.ProjectDb.UpdateFileFingerprint(*file.ID, fp.ToJSON())
			}
			fmt.Fprintf(os.Stderr, "  \033[32m✓\033[0m %s \033[36m(fingerprint updated)\033[0m\n", ref)
			counts.ok++
			continue
		}

		// Partial fingerprint match — conflict, needs resolution
		if match := findPartialMatchFile(allFiles, fp); match != nil {
			conflicts = append(conflicts, syncConflict{
				relPath:   relPath,
				ref:       ref,
				diskFp:    fp,
				diskHash:  hash,
				matchFile: match,
			})
			continue
		}

		// No match — new file, ingest
		file := &models.TrackedFile{
			SHA256:      hash,
			Fingerprint: fp.ToJSON(),
			IngestedAt:  time.Now().UTC().Format(time.RFC3339),
		}
		fileID, err := ctx.ProjectDb.InsertFile(file)
		if err != nil {
			fmt.Fprintf(os.Stderr, "  \033[31m✗\033[0m %s: %v\n", ref, err)
			continue
		}
		_ = fileID
		seen[hash] = true

		matchingCats := matchingCategories(relPath, categories)
		materialize.MaterializeForFile(ctx.ProjectDb, relPath, hash, matchingCats, nil)

		fmt.Fprintf(os.Stderr, "  \033[32m+\033[0m %s\n", ref)
		counts.ingested++
	}

	// Resolve conflicts
	if len(conflicts) > 0 {
		resolveConflicts(ctx, &counts, conflicts, categories, interactive, projectName)
	}

	// Check for missing files
	var missingRefs []string
	for _, f := range allFiles {
		if !seen[f.SHA256] {
			hashPreview := f.SHA256[:min(len(f.SHA256), 10)]
			missingRefs = append(missingRefs, hashPreview)
			counts.missing++
		}
	}

	// Summary
	fmt.Fprintln(os.Stderr)
	if len(missingRefs) > 0 {
		fmt.Fprintf(os.Stderr, "\033[33mMissing files:\033[0m\n")
		for _, h := range missingRefs {
			fmt.Fprintf(os.Stderr, "  \033[33m?\033[0m [%s...]\n", h)
		}
		fmt.Fprintln(os.Stderr)
	}

	fmt.Fprintf(os.Stderr, "Sync: %d ok, %d new, %d modified, %d missing, %d other\n",
		counts.ok, counts.ingested, counts.modified, counts.missing, counts.other)

	exitCode := counts.other
	if exitCode > 0 {
		return fmt.Errorf("%d file(s) need attention", exitCode)
	}
	if counts.missing > 0 {
		return fmt.Errorf("%d file(s) missing", counts.missing)
	}
	return nil
}

func resolveConflicts(
	ctx *context.Context,
	counts *syncCounts,
	conflicts []syncConflict,
	categories []models.Scope,
	interactive bool,
	projectName string,
) {
	for i, c := range conflicts {
		if !interactive {
			// Quiet mode: treat as "other"
			counts.other++
			fmt.Fprintf(os.Stderr, "  \033[33m~\033[0m %s (partial match, unresolved)\n", c.ref)
			continue
		}

		// Interactive prompt
		fmt.Fprintf(os.Stderr, "\n\033[1m%s\033[0m%s\n",
			c.ref, fmt.Sprintf("  [%d/%d]", i+1, len(conflicts)))

		// Show context about the matched file
		if c.matchFile != nil {
			hashPreview := c.matchFile.SHA256[:min(len(c.matchFile.SHA256), 16)]
			fmt.Fprintf(os.Stderr, "  Partial match for tracked file [%s...]\n", hashPreview)

			if c.matchFile.ID != nil {
				tags, _ := ctx.ProjectDb.GetTags(*c.matchFile.ID)
				if len(tags) > 0 {
					fmt.Fprintf(os.Stderr, "  Tags: %s\n", strings.Join(tags, ", "))
				}
				pipelines, _ := ctx.ProjectDb.GetPipelinesForSHA256(c.matchFile.SHA256)
				for _, p := range pipelines {
					fmt.Fprintf(os.Stderr, "  Pipeline: %s\n", p.Name)
				}
			}
		}

		fmt.Fprintf(os.Stderr, "  Is this file new, or a modified version of the matched file?\n")
		fmt.Fprintf(os.Stderr, "  > \033[1mNew\033[0m  \033[1mModified\033[0m  \033[1mOther\033[0m  ")

		choice := readChoice()

		switch choice {
		case "n", "new":
			// Ingest as new file
			file := &models.TrackedFile{
				SHA256:      c.diskHash,
				Fingerprint: c.diskFp.ToJSON(),
				IngestedAt:  time.Now().UTC().Format(time.RFC3339),
			}
			fileID, err := ctx.ProjectDb.InsertFile(file)
			if err != nil {
				fmt.Fprintf(os.Stderr, "  \033[31m✗\033[0m %s: %v\n", c.ref, err)
				continue
			}
			_ = fileID
			matchingCats := matchingCategories(c.relPath, categories)
			materialize.MaterializeForFile(ctx.ProjectDb, c.relPath, c.diskHash, matchingCats, nil)
			fmt.Fprintf(os.Stderr, "  \033[32m+\033[0m %s (new)\n", c.ref)
			counts.ingested++

		case "m", "modified":
			// Update the existing file's hash and fingerprint
			if c.matchFile != nil && c.matchFile.ID != nil {
				ctx.ProjectDb.UpdateFileFingerprint(*c.matchFile.ID, c.diskFp.ToJSON())
				// Update SHA256 too
				ctx.ProjectDb.UpdateFileSHA256(*c.matchFile.ID, c.diskHash)
			}
			fmt.Fprintf(os.Stderr, "  \033[33m~\033[0m %s (modified)\n", c.ref)
			counts.modified++

		default:
			// Other — flag for attention
			fmt.Fprintf(os.Stderr, "  \033[31m!\033[0m %s (needs attention)\n", c.ref)
			counts.other++
		}
	}
}

func readChoice() string {
	buf := make([]byte, 64)
	n, err := os.Stdin.Read(buf)
	if err != nil || n == 0 {
		return "other"
	}
	return strings.TrimSpace(strings.ToLower(string(buf[:n])))
}

func findPartialMatchFile(allFiles []models.TrackedFile, diskFp *integrity.Fingerprint) *models.TrackedFile {
	if len(diskFp.Chunks) == 0 {
		return nil
	}
	for i := range allFiles {
		dbFp, err := integrity.FingerprintFromJSON(allFiles[i].Fingerprint)
		if err != nil || len(dbFp.Chunks) == 0 {
			continue
		}
		matching := diskFp.MatchingChunks(dbFp)
		minLen := len(diskFp.Chunks)
		if len(dbFp.Chunks) < minLen {
			minLen = len(dbFp.Chunks)
		}
		if minLen > 0 && matching*2 > minLen {
			return &allFiles[i]
		}
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

