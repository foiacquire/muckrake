package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"go.foia.dev/muckrake/internal/cli"
	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/generator"
	"go.foia.dev/muckrake/internal/reference"
)

type command struct {
	run  func(*context.Context, []string) error
	desc string
}

var commands = map[string]command{
	"sync":     {cli.RunSync, "scan filesystem, track new files, verify integrity"},
	"status":   {cli.RunStatus, "show project or file status"},
	"list":     {cli.RunList, "list files, optionally filtered by reference"},
	"tag":      {cli.RunTag, "add or remove tags (--remove)"},
	"sign":     {cli.RunSign, "create or revoke pipeline attestations (--remove)"},
	"pipeline": {cli.RunPipeline, "create or remove pipelines (--remove)"},
	"read":     {cli.RunRead, "output file contents to stdout"},
	"open":     {cli.RunOpen, "open file in $PAGER"},
	"edit":     {cli.RunEdit, "open file in $EDITOR"},
}

const helpText = `mkrk — investigative journalism research management

usage: mkrk [<subject>] <command> [args...]

  subject is an optional :reference prefix naming what the command should
  operate on. Without one, commands run in the current working directory.

subjects:
  :                     workspace-wide, iterate all projects
  :project              a specific project
  :project.category     a category within a project
  :.category            category across all projects in workspace

commands:
  init       initialize a project or workspace
  sync       scan filesystem, track new files, verify integrity
  status     show project or file status
  list       list files, optionally filtered by reference
  tag        add or remove tags (--remove)
  sign       create or revoke pipeline attestations (--remove)
  pipeline   create or remove pipelines (--remove)
  read       output file contents to stdout
  open       open file in $PAGER
  edit       open file in $EDITOR

references:
  :project              all files in a project (workspace scope)
  :.category            category across all projects in workspace
  :project.category     specific project + category
  category              category in current context
  .category             same as above (explicit context scope)
  :{a,b}.{c,d}          brace expansion (cartesian product)

  :scope!tag            filter by tag (AND across ! groups)
  :scope!t1,t2          OR within a tag group
  :scope!t1!t2          AND across tag groups

  scope/*.pdf           glob filter on filenames
  scope/filename.ext    specific file (/ needed when name has .)
  scope.Makefile        file without extension (. separator ok)
  /file.pdf             file at project root

  :                     all files (widest available scope)
  :!tag                 all files matching a tag
  :/*.pdf               all files matching a glob

  ./path                literal filesystem path (escape hatch)
`

func main() {
	args := os.Args[1:]
	if len(args) == 0 {
		fmt.Fprint(os.Stderr, helpText)
		os.Exit(1)
	}

	// Init creates context rather than consuming it.
	if args[0] == "init" {
		if err := cli.RunInit(args[1:]); err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		return
	}

	// Optional :ref prefix becomes the command subject.
	var subject *reference.Reference
	if strings.HasPrefix(args[0], ":") {
		r, err := reference.ParseReference(args[0])
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		subject = r
		args = args[1:]
	}

	if len(args) == 0 {
		fmt.Fprint(os.Stderr, helpText)
		os.Exit(1)
	}

	verb := args[0]
	cmdArgs := args[1:]

	if err := run(verb, cmdArgs, subject); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}

func run(verb string, args []string, subject *reference.Reference) error {
	cwd, err := os.Getwd()
	if err != nil {
		return err
	}

	d, err := resolveDispatch(cwd, subject)
	if err != nil {
		return err
	}
	defer d.close()

	if c, ok := commands[verb]; ok {
		return runBuiltin(c, d, args)
	}
	return runGenerated(verb, d, args)
}

// dispatch holds the set of project contexts a command should run against,
// plus an optional workspace context that owns the shared workspace DB.
type dispatch struct {
	workspace *context.Context
	projects  []*context.Context
	fallback  *context.Context
}

func (d *dispatch) close() {
	for _, p := range d.projects {
		p.Close()
	}
	if d.workspace != nil {
		d.workspace.Close()
	}
	if d.fallback != nil {
		d.fallback.Close()
	}
}

func (d *dispatch) contexts() []*context.Context {
	if len(d.projects) > 0 {
		return d.projects
	}
	if d.fallback != nil {
		return []*context.Context{d.fallback}
	}
	return nil
}

// resolveDispatch picks the contexts to run the verb against based on the
// subject ref (if any) and the current working directory.
func resolveDispatch(cwd string, subject *reference.Reference) (*dispatch, error) {
	if subject != nil && subject.Kind == reference.KindWorkspace {
		return dispatchFromWorkspaceSubject(cwd, subject)
	}

	ctx, err := context.Discover(cwd)
	if err != nil {
		return nil, err
	}

	switch ctx.Kind {
	case context.ContextProject:
		ctx.Subject = subject
		return &dispatch{fallback: ctx}, nil
	case context.ContextWorkspace:
		return iterateWorkspaceProjects(ctx, subject)
	default:
		ctx.Subject = subject
		return &dispatch{fallback: ctx}, nil
	}
}

func dispatchFromWorkspaceSubject(cwd string, subject *reference.Reference) (*dispatch, error) {
	wsCtx, err := context.DiscoverWorkspace(cwd)
	if err != nil {
		return nil, err
	}

	if !subject.WorkspaceWide && len(subject.Scope) > 0 {
		projName := subject.Scope[0].Names[0]
		proj, err := wsCtx.Workspace.Db.GetProjectByName(projName)
		if err != nil {
			wsCtx.Close()
			return nil, err
		}
		if proj == nil {
			wsCtx.Close()
			return nil, fmt.Errorf("project %q not found in workspace", projName)
		}
		projRoot := filepath.Join(wsCtx.Workspace.Root, proj.Path)
		pctx, err := context.OpenProjectContext(projRoot, projName, wsCtx.Workspace)
		if err != nil {
			wsCtx.Close()
			return nil, err
		}
		pctx.Subject = subject
		return &dispatch{workspace: wsCtx, projects: []*context.Context{pctx}}, nil
	}

	return iterateWorkspaceProjects(wsCtx, subject)
}

func iterateWorkspaceProjects(wsCtx *context.Context, subject *reference.Reference) (*dispatch, error) {
	projects, err := wsCtx.Workspace.Db.ListProjects()
	if err != nil {
		wsCtx.Close()
		return nil, err
	}
	if len(projects) == 0 {
		wsCtx.Close()
		return nil, fmt.Errorf("no projects registered in workspace")
	}

	var pctxs []*context.Context
	for _, p := range projects {
		projRoot := filepath.Join(wsCtx.Workspace.Root, p.Path)
		if !fileExists(filepath.Join(projRoot, ".mkrk")) {
			continue
		}
		pctx, err := context.OpenProjectContext(projRoot, p.Name, wsCtx.Workspace)
		if err != nil {
			continue
		}
		pctx.Subject = subject
		pctxs = append(pctxs, pctx)
	}
	return &dispatch{workspace: wsCtx, projects: pctxs}, nil
}

func runBuiltin(c command, d *dispatch, args []string) error {
	ctxs := d.contexts()
	if len(ctxs) == 0 {
		return fmt.Errorf("no context available")
	}
	var lastErr error
	for _, ctx := range ctxs {
		if err := c.run(ctx, args); err != nil {
			lastErr = err
		}
	}
	return lastErr
}

func runGenerated(verb string, d *dispatch, args []string) error {
	ctxs := d.contexts()
	if len(ctxs) == 0 {
		return fmt.Errorf("no context available")
	}
	gens, err := generator.Collect(ctxs...)
	if err != nil {
		return err
	}
	if err := checkVerbCollisions(gens); err != nil {
		return err
	}
	for _, g := range gens {
		if g.Verb == verb {
			return cli.RunGenerated(ctxs, gens, verb, args)
		}
	}
	return fmt.Errorf("unknown command: %s", verb)
}

func checkVerbCollisions(gens []generator.Generator) error {
	for _, g := range gens {
		if _, ok := commands[g.Verb]; ok {
			return fmt.Errorf("generator verb %q (from scope %q in project %q) collides with built-in command",
				g.Verb, g.Scope.Name, g.ProjectName)
		}
	}
	return nil
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}
