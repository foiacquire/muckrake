package cli

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/materialize"
	"go.foia.dev/muckrake/internal/resolve"
)

func RunTag(ctx *context.Context, args []string) error {
	fs := flag.NewFlagSet("tag", flag.ExitOnError)
	remove := fs.Bool("remove", false, "remove tag instead of adding")
	fs.BoolVar(remove, "r", false, "shorthand for --remove")
	fs.Parse(args)

	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	paths, tagName, err := tagTargets(ctx, fs.Args())
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

		tags, _ := ctx.ProjectDb.GetTags(*file.ID)
		matchingCats := matchingCategories(relPath, categories)
		materialize.MaterializeForFile(ctx.ProjectDb, relPath, hash, matchingCats, tags)
	}

	return nil
}

// tagTargets picks the file set to tag. With a subject the positional args
// are just (tag), otherwise they are (reference, tag).
func tagTargets(ctx *context.Context, args []string) ([]string, string, error) {
	if resolve.HasNarrowSubject(ctx) {
		if len(args) < 1 {
			return nil, "", fmt.Errorf("usage: mkrk :<ref> tag [--remove] <tag>")
		}
		rels, err := resolve.SubjectRelPaths(ctx)
		if err != nil {
			return nil, "", err
		}
		return rels, args[0], nil
	}
	if len(args) < 2 {
		return nil, "", fmt.Errorf("usage: mkrk tag [--remove] <reference> <tag>")
	}
	rels, err := resolve.RefRelPaths(ctx, args[0])
	if err != nil {
		return nil, "", err
	}
	return rels, args[1], nil
}
