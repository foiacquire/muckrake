package main

import (
	"fmt"
	"os"

	"go.foia.dev/muckrake/cmd"
)

type command struct {
	run  func([]string) error
	desc string
}

var commands = map[string]command{
	"init": {cmd.RunInit, "initialize a project or workspace"},
	"sync": {cmd.RunSync, "scan filesystem, track new files, verify integrity"},
	"status": {cmd.RunStatus, "show project or file status"},
	"list":   {cmd.RunList, "show scopes, pipelines, or files (--files)"},
	"tag":      {cmd.RunTag, "add or remove tags (--remove)"},
	"sign":     {cmd.RunSign, "create or revoke pipeline attestations (--remove)"},
	"pipeline": {cmd.RunPipeline, "create or remove pipelines (--remove)"},
	"read": {cmd.RunRead, "output file contents to stdout"},
	"open": {cmd.RunOpen, "open file in $PAGER"},
	"edit": {cmd.RunEdit, "open file in $EDITOR"},
}

const helpText = `mkrk — investigative journalism research management

usage: mkrk <command> [args...]

commands:
  init       initialize a project or workspace
  sync       scan filesystem, track new files, verify integrity
  status     show project or file status
  list       show scopes or files (--files)
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

	c, ok := commands[os.Args[1]]
	if !ok {
		fmt.Fprintf(os.Stderr, "unknown command: %s\n", os.Args[1])
		os.Exit(1)
	}

	if err := c.run(os.Args[2:]); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}
