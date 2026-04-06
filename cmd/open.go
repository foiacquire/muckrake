package cmd

import (
	"flag"
	"fmt"
	"os"
	"os/exec"

	"go.foia.dev/muckrake/internal/context"
)

func RunOpen(args []string) error {
	return runExternalViewer(args, "open", envOrDefault("PAGER", "less"))
}

func RunEdit(args []string) error {
	return runExternalViewer(args, "edit", envOrDefault("EDITOR", "vi"))
}

func runExternalViewer(args []string, action, defaultCmd string) error {
	fs := flag.NewFlagSet(action, flag.ExitOnError)
	fs.Parse(args)

	if fs.NArg() == 0 {
		return fmt.Errorf("usage: mkrk %s <reference>", action)
	}

	cwd, _ := os.Getwd()
	ctx, err := context.Discover(cwd)
	if err != nil {
		return err
	}
	defer ctx.Close()

	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project (run from a project directory or use sync first)")
	}

	paths, err := resolveToFilePaths(ctx, fs.Arg(0))
	if err != nil {
		return err
	}
	if len(paths) == 0 {
		return fmt.Errorf("no files matched")
	}
	if len(paths) > 1 {
		return fmt.Errorf("reference matched %d files, expected 1", len(paths))
	}

	cmd := exec.Command(defaultCmd, paths[0])
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

func envOrDefault(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
