package cmd

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/reference"
	"go.foia.dev/muckrake/internal/walk"
)

func RunList(ctx *context.Context, args []string) error {
	fs := flag.NewFlagSet("list", flag.ExitOnError)
	fs.Parse(args)

	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	projectName := ""
	if ctx.ProjectName != nil {
		projectName = *ctx.ProjectName
	}

	if fs.NArg() > 0 {
		return listRefFiles(ctx, projectName, fs.Args())
	}
	return listAllFiles(ctx, projectName)
}

func listAllFiles(ctx *context.Context, projectName string) error {
	patterns, err := walk.CategoryPatterns(ctx.ProjectDb, nil)
	if err != nil {
		return err
	}
	entries, err := walk.WalkAndCollect(ctx.ProjectRoot, patterns)
	if err != nil {
		return err
	}

	for _, relPath := range entries {
		absPath := filepath.Join(ctx.ProjectRoot, relPath)
		ref := reference.FormatRef(relPath, projectName, ctx.ProjectDb)

		fp, err := integrity.FingerprintFile(absPath)
		if err != nil {
			fmt.Printf("\033[31m%s\033[0m\n", ref)
			fmt.Fprintf(os.Stderr, "! %s: %v\n", ref, err)
			continue
		}

		// Exact fingerprint match — file unchanged
		if file, _ := ctx.ProjectDb.GetFileByFingerprint(fp.ToJSON()); file != nil {
			fmt.Println(ref)
			continue
		}

		// Hash match — tracked but fingerprint stale
		hash, _ := integrity.HashFile(absPath)
		if file, _ := ctx.ProjectDb.GetFileByHash(hash); file != nil {
			fmt.Println(ref)
			continue
		}

		// Partial fingerprint match — file was modified
		if bestMatch := findPartialMatch(ctx, fp); bestMatch != nil {
			fmt.Printf("\033[33m%s\033[0m\n", ref)
			continue
		}

		// No match at all — untracked
		fmt.Printf("\033[31m%s\033[0m\n", ref)
	}

	if len(entries) == 0 {
		fmt.Fprintln(os.Stderr, "(no files)")
	}
	return nil
}

func findPartialMatch(ctx *context.Context, diskFp *integrity.Fingerprint) *models.TrackedFile {
	allFiles, err := ctx.ProjectDb.ListAllFiles()
	if err != nil || len(diskFp.Chunks) == 0 {
		return nil
	}
	for _, f := range allFiles {
		dbFp, err := integrity.FingerprintFromJSON(f.Fingerprint)
		if err != nil || len(dbFp.Chunks) == 0 {
			continue
		}
		matching := diskFp.MatchingChunks(dbFp)
		minLen := len(diskFp.Chunks)
		if len(dbFp.Chunks) < minLen {
			minLen = len(dbFp.Chunks)
		}
		if minLen > 0 && matching*2 > minLen {
			return &f
		}
	}
	return nil
}

func listRefFiles(ctx *context.Context, projectName string, refs []string) error {
	for _, raw := range refs {
		ref, err := reference.ParseReference(raw)
		if err != nil {
			return err
		}

		if ref.Kind == reference.KindBarePath {
			fmt.Println(ref.Raw)
			continue
		}

		if len(ref.Scope) == 0 {
			return listAllFiles(ctx, projectName)
		}

		catName := ref.Scope[0].Names[0]
		patterns, err := walk.CategoryPatterns(ctx.ProjectDb, &catName)
		if err != nil {
			return err
		}
		entries, err := walk.WalkAndCollect(ctx.ProjectRoot, patterns)
		if err != nil {
			return err
		}

		for _, relPath := range entries {
			if ref.Glob != nil {
				fileName := filepath.Base(relPath)
				mf, _ := models.GlobMatch(*ref.Glob, fileName)
				mp, _ := models.GlobMatch(*ref.Glob, relPath)
				if !mf && !mp {
					continue
				}
			}
			fmt.Println(reference.FormatRef(relPath, projectName, ctx.ProjectDb))
		}
	}
	return nil
}
