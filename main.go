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
}

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: mkrk <command> [args...]")
		fmt.Fprintln(os.Stderr)
		fmt.Fprintln(os.Stderr, "commands:")
		for name, c := range commands {
			fmt.Fprintf(os.Stderr, "  %-10s %s\n", name, c.desc)
		}
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
