package cmd

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/evaluate"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/reference"
	"go.foia.dev/muckrake/internal/walk"
)

func RunStatus(args []string) error {
	fs := flag.NewFlagSet("status", flag.ExitOnError)
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
		return fileStatus(ctx, fs.Args())
	}
	return projectStatus(ctx)
}

func projectStatus(ctx *context.Context) error {
	if ctx.Kind == context.ContextWorkspace {
		ws := ctx.Workspace
		count, _ := ws.Db.ProjectCount()
		fmt.Printf("Workspace: %s\n", ws.Root)
		fmt.Printf("  Projects: %d\n", count)
		return nil
	}

	fileCount, _ := ctx.ProjectDb.FileCount()
	catCount, _ := ctx.ProjectDb.CategoryCount()

	fmt.Printf("Project: %s\n", ctx.ProjectRoot)
	if ctx.ProjectName != nil {
		fmt.Printf("  Name: %s\n", *ctx.ProjectName)
	}
	fmt.Printf("  Files: %d\n", fileCount)
	fmt.Printf("  Categories: %d\n", catCount)

	pipelines, _ := ctx.ProjectDb.ListPipelines()
	if len(pipelines) > 0 {
		fmt.Printf("  Pipelines: %d\n", len(pipelines))
		for _, p := range pipelines {
			subs, _ := ctx.ProjectDb.ListPipelineSubscriptions(*p.ID)
			fmt.Printf("    %s (%d states, %d subscriptions)\n",
				p.Name, len(p.States), len(subs))
		}
	}

	rulesets, _ := ctx.ProjectDb.ListRulesets()
	if len(rulesets) > 0 {
		fmt.Printf("  Rulesets: %d\n", len(rulesets))
		for _, rs := range rulesets {
			fmt.Printf("    %s\n", rs.Name)
		}
	}

	return nil
}

func fileStatus(ctx *context.Context, refs []string) error {
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("must be inside a project")
	}

	for _, rawRef := range refs {
		paths, err := resolveToRelPaths(ctx, rawRef)
		if err != nil {
			return err
		}
		for _, relPath := range paths {
			if err := printFileStatus(ctx, relPath); err != nil {
				return err
			}
		}
	}
	return nil
}

func printFileStatus(ctx *context.Context, relPath string) error {
	absPath := filepath.Join(ctx.ProjectRoot, relPath)
	hash, err := integrity.HashFile(absPath)
	if err != nil {
		return fmt.Errorf("%s: %w", relPath, err)
	}

	file, _ := ctx.ProjectDb.GetFileByHash(hash)
	if file == nil {
		fmt.Printf("%s: untracked\n", relPath)
		return nil
	}

	fmt.Printf("%s\n", relPath)
	fmt.Printf("  SHA-256: %s\n", hash)

	// Protection level
	protection, _ := ctx.ProjectDb.ResolveProtection(relPath)
	fmt.Printf("  Protection: %s\n", protection)

	// Tags
	if file.ID != nil {
		tags, _ := ctx.ProjectDb.GetTags(*file.ID)
		if len(tags) > 0 {
			fmt.Printf("  Tags:")
			for _, t := range tags {
				fmt.Printf(" %s", t)
			}
			fmt.Println()
		}
	}

	// Pipelines (materialized)
	pipelines, _ := ctx.ProjectDb.GetPipelinesForSHA256(hash)
	if len(pipelines) > 0 {
		fmt.Printf("  Pipelines:\n")
		for _, p := range pipelines {
			state := derivePipelineState(ctx, file, &p, hash)
			fmt.Printf("    %s: %s\n", p.Name, state)
		}
	}

	// Ruleset evaluation
	evalResult, _ := evaluate.EvaluateForFile(ctx.ProjectDb, &evaluate.EvalContext{
		SHA256:   hash,
		MimeType: file.MimeType,
	})
	if evalResult != nil && len(evalResult.ToolDispatches) > 0 {
		fmt.Printf("  Tools:\n")
		for _, td := range evalResult.ToolDispatches {
			fmt.Printf("    %s (from %s)\n", td.Command, td.RulesetName)
		}
	}

	return nil
}

func derivePipelineState(ctx *context.Context, file *models.TrackedFile, p *models.Pipeline, hash string) string {
	if file.ID == nil || p.ID == nil {
		return p.States[0]
	}
	signs, _ := ctx.ProjectDb.GetValidSignsForFilePipeline(*file.ID, *p.ID, hash)
	if len(signs) == 0 {
		return p.States[0]
	}

	// Walk states in order, find the latest reachable state
	current := p.States[0]
	for _, state := range p.States[1:] {
		required, ok := p.Transitions[state]
		if !ok {
			break
		}
		allSigned := true
		for _, req := range required {
			found := false
			for _, s := range signs {
				if s.SignName == req {
					found = true
					break
				}
			}
			if !found {
				allSigned = false
				break
			}
		}
		if allSigned {
			current = state
		} else {
			break
		}
	}
	return current
}

func resolveToRelPaths(ctx *context.Context, rawRef string) ([]string, error) {
	ref, err := reference.ParseReference(rawRef)
	if err != nil {
		return nil, err
	}

	if ref.Kind == reference.KindBarePath {
		return []string{ref.Raw}, nil
	}

	if len(ref.Scope) == 0 {
		return nil, fmt.Errorf("reference must specify a scope")
	}

	catName := ref.Scope[0].Names[0]
	patterns, err := walk.CategoryPatterns(ctx.ProjectDb, &catName)
	if err != nil {
		return nil, err
	}
	entries, err := walk.WalkAndCollect(ctx.ProjectRoot, patterns)
	if err != nil {
		return nil, err
	}

	if ref.Glob != nil {
		var filtered []string
		for _, relPath := range entries {
			fileName := filepath.Base(relPath)
			if ok, _ := reference.GlobMatchFile(*ref.Glob, fileName, relPath); ok {
				filtered = append(filtered, relPath)
			}
		}
		return filtered, nil
	}
	return entries, nil
}
