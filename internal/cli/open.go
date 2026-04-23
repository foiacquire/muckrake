package cli

import (
	"flag"
	"fmt"
	"os"
	"os/exec"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/resolve"
)

func RunOpen(ctx *context.Context, args []string) error {
	return runExternalViewer(ctx, args, "open", envOrDefault("PAGER", "less"))
}

func RunEdit(ctx *context.Context, args []string) error {
	return runExternalViewer(ctx, args, "edit", envOrDefault("EDITOR", "vi"))
}

func runExternalViewer(ctx *context.Context, args []string, action, defaultCmd string) error {
	fs := flag.NewFlagSet(action, flag.ExitOnError)
	fs.Parse(args)

	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	paths, err := singleFileTarget(ctx, fs.Args(), action)
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

func singleFileTarget(ctx *context.Context, args []string, action string) ([]string, error) {
	if resolve.HasNarrowSubject(ctx) {
		return resolve.SubjectFiles(ctx)
	}
	if len(args) == 0 {
		return nil, fmt.Errorf("usage: mkrk :<ref> %s  |  mkrk %s <reference>", action, action)
	}
	return resolve.Ref(ctx, args[0])
}

func envOrDefault(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
