package cmd

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/reference"
	"go.foia.dev/muckrake/internal/walk"
)

func RunList(args []string) error {
	fs := flag.NewFlagSet("list", flag.ExitOnError)
	fs.Parse(args)

	cwd, _ := os.Getwd()
	ctx, err := context.Discover(cwd)
	if err != nil {
		return err
	}
	defer ctx.Close()

	if ctx.Kind == context.ContextNone {
		return fmt.Errorf("not in a muckrake project or workspace")
	}

	if fs.NArg() > 0 {
		return listRefFiles(ctx, fs.Args())
	}
	return listAllFiles(ctx)
}

func listAllFiles(ctx *context.Context) error {
	patterns, err := walk.CategoryPatterns(ctx.ProjectDb, nil)
	if err != nil {
		return err
	}
	entries, err := walk.WalkAndCollect(ctx.ProjectRoot, patterns)
	if err != nil {
		return err
	}

	for _, relPath := range entries {
		fmt.Println(relPath)
	}

	if len(entries) == 0 {
		fmt.Fprintln(os.Stderr, "(no files)")
	}
	return nil
}

func listRefFiles(ctx *context.Context, refs []string) error {
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
			return listAllFiles(ctx)
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
			fmt.Println(relPath)
		}
	}
	return nil
}
