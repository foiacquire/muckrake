package cli

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/integrity"
)

// RunRemove deletes/revokes a project resource. The first argument is the
// resource type (the "noun"). Each noun has its own contract.
func RunRemove(ctx *context.Context, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("usage: mkrk remove <%s> ...", strings.Join(removeNouns(), "|"))
	}
	noun := args[0]
	rest := args[1:]
	handler, ok := removeHandlers[noun]
	if !ok {
		return fmt.Errorf("cannot remove %q (try: %s)", noun, strings.Join(removeNouns(), ", "))
	}
	return handler(ctx, rest)
}

var removeHandlers = map[string]func(*context.Context, []string) error{
	"pipeline": removePipelineCmd,
	"ruleset":  removeRuleset,
	"tag":      removeTag,
	"sign":     removeSign,
	"tool":     removeTool,
}

func removeNouns() []string {
	out := make([]string, 0, len(removeHandlers))
	for k := range removeHandlers {
		out = append(out, k)
	}
	return out
}

func removePipelineCmd(ctx *context.Context, args []string) error {
	name, _ := extractName(args)
	if name == "" {
		return fmt.Errorf("usage: mkrk remove pipeline <name>")
	}
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}
	removed, err := ctx.ProjectDb.RemovePipeline(name)
	if err != nil {
		return err
	}
	if removed == 0 {
		return fmt.Errorf("pipeline '%s' not found", name)
	}
	fmt.Fprintf(os.Stderr, "Removed pipeline '%s'\n", name)
	return nil
}

func removeRuleset(ctx *context.Context, args []string) error {
	name, _ := extractName(args)
	if name == "" {
		return fmt.Errorf("usage: mkrk remove ruleset <name>")
	}
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}
	removed, err := ctx.ProjectDb.RemoveRuleset(name)
	if err != nil {
		return err
	}
	if removed == 0 {
		return fmt.Errorf("ruleset '%s' not found", name)
	}
	fmt.Fprintf(os.Stderr, "Removed ruleset '%s'\n", name)
	return nil
}

func removeTag(ctx *context.Context, args []string) error {
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}
	paths, tagName, err := tagTargets(ctx, args)
	if err != nil {
		return err
	}
	if len(paths) == 0 {
		return fmt.Errorf("no files matched")
	}
	return applyTag(ctx, paths, tagName, true)
}

func removeSign(ctx *context.Context, args []string) error {
	pipelineName, signName, paths, err := signSetup(ctx, args)
	if err != nil {
		return err
	}
	relPath := paths[0]
	absPath := filepath.Join(ctx.ProjectRoot, relPath)
	hash, err := integrity.HashFile(absPath)
	if err != nil {
		return err
	}
	file, _ := ctx.ProjectDb.GetFileByHash(hash)
	if file == nil || file.ID == nil {
		return fmt.Errorf("file not tracked (run sync first)")
	}
	pipeline, err := ctx.ProjectDb.GetPipelineByName(pipelineName)
	if err != nil || pipeline == nil {
		return fmt.Errorf("pipeline '%s' not found", pipelineName)
	}

	allSigns, err := ctx.ProjectDb.GetSignsForFile(*file.ID)
	if err != nil {
		return err
	}
	now := time.Now().UTC().Format(time.RFC3339)
	revoked := 0
	for _, s := range allSigns {
		if s.SignName == signName && s.PipelineID == *pipeline.ID && s.RevokedAt == nil && s.ID != nil {
			if _, err := ctx.ProjectDb.RevokeSign(*s.ID, now); err != nil {
				return err
			}
			revoked++
		}
	}
	if revoked == 0 {
		return fmt.Errorf("no active sign '%s' found for '%s'", signName, relPath)
	}
	fmt.Fprintf(os.Stderr, "Revoked '%s' on '%s' (%d sign(s))\n", signName, relPath, revoked)
	return nil
}

func removeTool(ctx *context.Context, args []string) error {
	name, _ := extractName(args)
	if name == "" {
		return fmt.Errorf("usage: mkrk remove tool <name>")
	}
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	target := findToolPath(ctx.ProjectRoot, name)
	if target == "" {
		return fmt.Errorf("tool '%s' not found in tools/", name)
	}
	if err := os.Remove(target); err != nil {
		return err
	}
	rel, _ := filepath.Rel(ctx.ProjectRoot, target)
	fmt.Fprintf(os.Stderr, "Removed tool '%s' (%s)\n", name, rel)
	return nil
}

func findToolPath(projectRoot, name string) string {
	candidates := []string{
		filepath.Join(projectRoot, "tools", name),
		filepath.Join(projectRoot, "tools", name+".sh"),
		filepath.Join(projectRoot, "tools", name+".py"),
		filepath.Join(projectRoot, "tools", name+".rb"),
		filepath.Join(projectRoot, "tools", name+".js"),
	}
	for _, p := range candidates {
		if _, err := os.Stat(p); err == nil {
			return p
		}
	}
	return ""
}
