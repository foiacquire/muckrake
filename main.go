package main

import (
	"fmt"
	"os"
	"path/filepath"

	"go.foia.dev/muckrake/cmd"
	"go.foia.dev/muckrake/internal/context"
)

type command struct {
	run  func(*context.Context, []string) error
	desc string
}

var commands = map[string]command{
	"sync":     {cmd.RunSync, "scan filesystem, track new files, verify integrity"},
	"status":   {cmd.RunStatus, "show project or file status"},
	"list":     {cmd.RunList, "list files, optionally filtered by reference"},
	"tag":      {cmd.RunTag, "add or remove tags (--remove)"},
	"sign":     {cmd.RunSign, "create or revoke pipeline attestations (--remove)"},
	"pipeline": {cmd.RunPipeline, "create or remove pipelines (--remove)"},
	"read":     {cmd.RunRead, "output file contents to stdout"},
	"open":     {cmd.RunOpen, "open file in $PAGER"},
	"edit":     {cmd.RunEdit, "open file in $EDITOR"},
}

const helpText = `mkrk — investigative journalism research management

usage: mkrk <command> [args...]

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
  Files are addressed using a structured query language.

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
	if len(os.Args) < 2 {
		fmt.Fprint(os.Stderr, helpText)
		os.Exit(1)
	}

	// Init is special — it creates context rather than consuming it
	if os.Args[1] == "init" {
		if err := cmd.RunInit(os.Args[2:]); err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		return
	}

	c, ok := commands[os.Args[1]]
	if !ok {
		fmt.Fprintf(os.Stderr, "unknown command: %s\n", os.Args[1])
		os.Exit(1)
	}

	if err := dispatch(c, os.Args[2:]); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}

func dispatch(c command, args []string) error {
	cwd, err := os.Getwd()
	if err != nil {
		return err
	}

	ctx, err := context.Discover(cwd)
	if err != nil {
		return err
	}

	// If at workspace root (not inside a project), dispatch to each project
	if ctx.Kind == context.ContextWorkspace {
		return dispatchWorkspace(c, ctx, args)
	}

	defer ctx.Close()
	return c.run(ctx, args)
}

func dispatchWorkspace(c command, wsCtx *context.Context, args []string) error {
	defer wsCtx.Close()

	projects, err := wsCtx.Workspace.Db.ListProjects()
	if err != nil {
		return err
	}
	if len(projects) == 0 {
		return fmt.Errorf("no projects registered in workspace")
	}

	var lastErr error
	for _, p := range projects {
		projRoot := filepath.Join(wsCtx.Workspace.Root, p.Path)
		if !fileExists(filepath.Join(projRoot, ".mkrk")) {
			continue
		}

		projCtx, err := context.OpenProjectContext(projRoot, p.Name, wsCtx.Workspace)
		if err != nil {
			lastErr = err
			continue
		}

		if err := c.run(projCtx, args); err != nil {
			lastErr = err
		}
		projCtx.Close()
	}

	return lastErr
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}
