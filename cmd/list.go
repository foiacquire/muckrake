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

func RunList(args []string) error {
	fs := flag.NewFlagSet("list", flag.ExitOnError)
	files := fs.Bool("files", false, "show matching files instead of scopes")
	fs.BoolVar(files, "f", false, "shorthand for --files")
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

	if ctx.Kind == context.ContextWorkspace {
		return listWorkspace(ctx)
	}

	if *files {
		return listFiles(ctx, fs.Args())
	}
	return listScopes(ctx)
}

func listWorkspace(ctx *context.Context) error {
	projects, err := ctx.Workspace.Db.ListProjects()
	if err != nil {
		return err
	}
	if len(projects) == 0 {
		fmt.Fprintln(os.Stderr, "No projects registered")
		return nil
	}
	for _, p := range projects {
		desc := ""
		if p.Description != nil {
			desc = fmt.Sprintf("  %s", *p.Description)
		}
		fmt.Printf("  %-15s %s%s\n", p.Name, p.Path, desc)
	}
	return nil
}

func listScopes(ctx *context.Context) error {
	cats, err := ctx.ProjectDb.ListCategories()
	if err != nil {
		return err
	}
	if len(cats) == 0 {
		fmt.Fprintln(os.Stderr, "No categories configured")
		return nil
	}

	for _, cat := range cats {
		pattern := ""
		if cat.Pattern != nil {
			pattern = *cat.Pattern
		}
		protection := models.ProtectionEditable
		if cat.ID != nil {
			if p, err := ctx.ProjectDb.GetPolicyForScope(*cat.ID); err == nil && p != nil {
				protection = *p
			}
		}
		catType := models.CategoryTypeFiles
		if cat.CategoryType != nil {
			catType = *cat.CategoryType
		}

		typeLabel := ""
		if catType != models.CategoryTypeFiles {
			typeLabel = fmt.Sprintf(" [%s]", catType)
		}
		fmt.Printf("  %-15s %-20s %s%s\n", cat.Name, pattern, protection, typeLabel)

		if cat.Description != nil {
			fmt.Printf("    %s\n", *cat.Description)
		}
	}
	return nil
}

func listFiles(ctx *context.Context, refs []string) error {
	if len(refs) == 0 {
		return listAllFiles(ctx)
	}

	for _, raw := range refs {
		ref, err := reference.ParseReference(raw)
		if err != nil {
			return err
		}
		if err := listRefFiles(ctx, ref); err != nil {
			return err
		}
	}
	return nil
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

	found := false
	for _, relPath := range entries {
		absPath := filepath.Join(ctx.ProjectRoot, relPath)
		hash, _ := integrity.HashFile(absPath)
		f, _ := ctx.ProjectDb.GetFileByHash(hash)
		tracked := f != nil
		prefix := " "
		if !tracked {
			prefix = "?"
		}
		hashPreview := ""
		if hash != "" && len(hash) >= 8 {
			hashPreview = hash[:8]
		}
		fmt.Printf("  %s %s  %s\n", prefix, relPath, hashPreview)
		found = true
	}

	if !found {
		fmt.Fprintln(os.Stderr, "(no files)")
	}
	return nil
}

func listRefFiles(ctx *context.Context, ref *reference.Reference) error {
	if ref.Kind == reference.KindBarePath {
		return fmt.Errorf("bare paths not supported in list, use a reference")
	}

	// For now, resolve scope to category patterns and walk
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
			matchFile, _ := models.GlobMatch(*ref.Glob, fileName)
			matchPath, _ := models.GlobMatch(*ref.Glob, relPath)
			if !matchFile && !matchPath {
				continue
			}
		}
		fmt.Printf("  %s\n", relPath)
	}
	return nil
}
