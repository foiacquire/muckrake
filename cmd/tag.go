package cmd

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/materialize"
)

func RunTag(args []string) error {
	fs := flag.NewFlagSet("tag", flag.ExitOnError)
	remove := fs.Bool("remove", false, "remove tag instead of adding")
	fs.BoolVar(remove, "r", false, "shorthand for --remove")
	fs.Parse(args)

	if fs.NArg() < 2 {
		return fmt.Errorf("usage: mkrk tag [--remove] <reference> <tag>")
	}

	rawRef := fs.Arg(0)
	tagName := fs.Arg(1)

	cwd, _ := os.Getwd()
	ctx, err := context.Discover(cwd)
	if err != nil {
		return err
	}
	defer ctx.Close()

	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("must be inside a project")
	}

	paths, err := resolveToRelPaths(ctx, rawRef)
	if err != nil {
		return err
	}
	if len(paths) == 0 {
		return fmt.Errorf("no files matched")
	}

	categories, _ := ctx.ProjectDb.ListCategories()

	for _, relPath := range paths {
		absPath := filepath.Join(ctx.ProjectRoot, relPath)
		hash, fp, err := integrity.HashAndFingerprint(absPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "  ! %s: %v\n", relPath, err)
			continue
		}

		file, err := ctx.ProjectDb.GetFileByHash(hash)
		if err != nil || file == nil || file.ID == nil {
			fmt.Fprintf(os.Stderr, "  ! %s: not tracked (run sync first)\n", relPath)
			continue
		}

		if *remove {
			if err := ctx.ProjectDb.RemoveTag(*file.ID, tagName); err != nil {
				fmt.Fprintf(os.Stderr, "  ! %s: %v\n", relPath, err)
				continue
			}
			fmt.Fprintf(os.Stderr, "  - %s !%s\n", relPath, tagName)
		} else {
			if err := ctx.ProjectDb.InsertTag(*file.ID, tagName, hash, fp.ToJSON()); err != nil {
				fmt.Fprintf(os.Stderr, "  ! %s: %v\n", relPath, err)
				continue
			}
			fmt.Fprintf(os.Stderr, "  + %s !%s\n", relPath, tagName)
		}

		// Re-materialize after tag change
		tags, _ := ctx.ProjectDb.GetTags(*file.ID)
		matchingCats := matchingCategories(relPath, categories)
		materialize.MaterializeForFile(ctx.ProjectDb, relPath, hash, matchingCats, tags)
	}

	return nil
}
