package cmd

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/generator"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/walk"
)

// RunGenerated dispatches a generated verb (e.g. "tool") to the correct
// generator based on the first argument, then executes the resolved tool
// file with the remaining arguments.
func RunGenerated(ctx *context.Context, gens []generator.Generator, verb string, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("%s: expected tool name as first argument", verb)
	}

	applicable := filterByVerb(gens, verb)
	if len(applicable) == 0 {
		return fmt.Errorf("no generators registered for verb %q", verb)
	}

	if strings.TrimSpace(args[0]) == "" || args[0] == ":" {
		return fmt.Errorf("%s: empty tool name", verb)
	}

	gen, toolPath, err := resolveTool(args[0], applicable, ctx)
	if err != nil {
		return err
	}

	toolArgs, err := resolveToolArgs(args[1:], ctx)
	if err != nil {
		return err
	}

	return execTool(gen, toolPath, toolArgs, ctx)
}

func filterByVerb(gens []generator.Generator, verb string) []generator.Generator {
	var out []generator.Generator
	for _, g := range gens {
		if g.Verb == verb {
			out = append(out, g)
		}
	}
	return out
}

// resolveTool interprets the first argument as a tool identifier, returning
// the generator that owns it and the absolute path to the tool file.
func resolveTool(arg string, gens []generator.Generator, ctx *context.Context) (generator.Generator, string, error) {
	explicitProject, toolName := splitProjectAndName(arg)

	if explicitProject != "" {
		for _, g := range gens {
			if g.ProjectName == explicitProject {
				path, err := findToolFile(g, toolName)
				if err != nil {
					return generator.Generator{}, "", err
				}
				if path == "" {
					return generator.Generator{}, "", fmt.Errorf("no tool %q in project %q", toolName, explicitProject)
				}
				return g, path, nil
			}
		}
		return generator.Generator{}, "", fmt.Errorf("no generator for project %q", explicitProject)
	}

	// Bare name: prefer current project, fall back to builtins.
	current := currentProjectName(ctx)
	var ordered []generator.Generator
	for _, g := range gens {
		if g.ProjectName == current {
			ordered = append(ordered, g)
		}
	}
	for _, g := range gens {
		if g.ProjectName != current {
			ordered = append(ordered, g)
		}
	}

	for _, g := range ordered {
		path, err := findToolFile(g, toolName)
		if err != nil {
			return generator.Generator{}, "", err
		}
		if path != "" {
			return g, path, nil
		}
	}
	return generator.Generator{}, "", fmt.Errorf("no tool %q found (use :project.name to disambiguate)", toolName)
}

// splitProjectAndName pulls an explicit project prefix from a reference-style
// first argument. Returns (project, name). project is empty for bare names.
// Intermediate scope segments are ignored — auto-scope supplies the generator's
// own scope, so :project.tool and :project.tools.tool both resolve the same.
func splitProjectAndName(arg string) (string, string) {
	body := stripLeadingDot(strings.TrimPrefix(arg, ":"))
	if body == "" {
		return "", ""
	}
	parts := strings.Split(body, ".")
	if !strings.HasPrefix(arg, ":") || len(parts) == 1 {
		return "", parts[len(parts)-1]
	}
	return parts[0], parts[len(parts)-1]
}

func stripLeadingDot(s string) string {
	if strings.HasPrefix(s, ".") {
		return s[1:]
	}
	return s
}

// findToolFile walks the generator's scope pattern looking for a file whose
// basename (with or without extension) matches toolName.
func findToolFile(g generator.Generator, toolName string) (string, error) {
	if g.IsBuiltin {
		// Built-in tools live in a Go-side registry, not on disk.
		// No registry entries yet → no matches.
		return "", nil
	}
	if g.ProjectRoot == "" || g.Scope.Pattern == nil {
		return "", nil
	}
	patterns := patternsForScope(*g.Scope.Pattern)
	entries, err := walk.WalkAndCollect(g.ProjectRoot, patterns)
	if err != nil {
		return "", err
	}
	for _, rel := range entries {
		base := filepath.Base(rel)
		if base == toolName || stripExt(base) == toolName {
			return filepath.Join(g.ProjectRoot, rel), nil
		}
	}
	return "", nil
}

func patternsForScope(pattern string) []string {
	base := models.NameFromPattern(pattern)
	return []string{base + "/*", base + "/**/*"}
}

func stripExt(name string) string {
	ext := filepath.Ext(name)
	if ext == "" {
		return name
	}
	return name[:len(name)-len(ext)]
}

func currentProjectName(ctx *context.Context) string {
	if ctx != nil && ctx.ProjectName != nil {
		return *ctx.ProjectName
	}
	return ""
}

// resolveToolArgs converts remaining argv into filesystem paths the tool can
// consume. For now, non-reference args pass through unchanged.
func resolveToolArgs(args []string, ctx *context.Context) ([]string, error) {
	// Tool arg resolution is deferred — passing through verbatim keeps the
	// current implementation unblocked. When the reference engine gains a
	// batch resolver, wire it in here.
	return args, nil
}

// execTool runs the resolved tool file with the given arguments, passing
// through stdio and injecting muckrake environment variables.
func execTool(g generator.Generator, path string, args []string, ctx *context.Context) error {
	cmd := exec.Command(path, args...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Env = append(os.Environ(), toolEnv(g, ctx)...)
	return cmd.Run()
}

func toolEnv(g generator.Generator, ctx *context.Context) []string {
	env := []string{
		"MKRK_GENERATOR_VERB=" + g.Verb,
		"MKRK_GENERATOR_SCOPE=" + g.Scope.Name,
	}
	if ctx != nil {
		if ctx.ProjectName != nil {
			env = append(env, "MKRK_PROJECT="+*ctx.ProjectName)
		}
		if ctx.ProjectRoot != "" {
			env = append(env, "MKRK_PROJECT_ROOT="+ctx.ProjectRoot)
		}
		if ctx.Workspace != nil {
			env = append(env, "MKRK_WORKSPACE_ROOT="+ctx.Workspace.Root)
		}
	}
	return env
}
