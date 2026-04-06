package cmd

import (
	"bufio"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/reference"
	"go.foia.dev/muckrake/internal/walk"
)

func RunRead(args []string) error {
	fs := flag.NewFlagSet("read", flag.ExitOnError)
	raw := fs.Bool("raw", false, "no color or decoration")
	pathFlag := fs.Bool("path", false, "show file path before content")
	fs.Parse(args)

	if fs.NArg() == 0 {
		return fmt.Errorf("usage: mkrk read <reference> [references...]")
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

	total := 0
	for _, rawRef := range fs.Args() {
		paths, err := resolveToFilePaths(ctx, rawRef)
		if err != nil {
			return err
		}
		for _, absPath := range paths {
			if total > 0 {
				fmt.Println()
			}
			if *pathFlag && !*raw {
				fmt.Printf("\033[1m%s\033[0m\n", absPath)
			} else if *pathFlag {
				fmt.Println(absPath)
			}
			if err := dumpContent(absPath, !*raw); err != nil {
				return err
			}
			total++
		}
	}

	if total == 0 {
		fmt.Fprintln(os.Stderr, "(no files)")
	}
	return nil
}

func resolveToFilePaths(ctx *context.Context, rawRef string) ([]string, error) {
	ref, err := reference.ParseReference(rawRef)
	if err != nil {
		return nil, err
	}

	if ref.Kind == reference.KindBarePath {
		abs := filepath.Join(ctx.ProjectRoot, ref.Raw)
		if _, err := os.Stat(abs); err != nil {
			return nil, fmt.Errorf("file not found: %s", ref.Raw)
		}
		return []string{abs}, nil
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

	var paths []string
	for _, relPath := range entries {
		if ref.Glob != nil {
			fileName := filepath.Base(relPath)
			mf, _ := reference.GlobMatchFile(*ref.Glob, fileName, relPath)
			if !mf {
				continue
			}
		}
		paths = append(paths, filepath.Join(ctx.ProjectRoot, relPath))
	}
	return paths, nil
}

func dumpContent(path string, colorize bool) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()

	info, err := f.Stat()
	if err != nil {
		return err
	}

	// Read header to check for binary
	reader := bufio.NewReaderSize(f, 8192)
	header, err := reader.Peek(min(int(info.Size()), 8192))
	if err != nil && err != io.EOF {
		return err
	}

	if isBinary(header) {
		sizeStr := formatSize(info.Size())
		if colorize {
			fmt.Printf("\033[2m(binary file, %s)\033[0m\n", sizeStr)
		} else {
			fmt.Printf("(binary file, %s)\n", sizeStr)
		}
		return nil
	}

	// Stream content
	if _, err := io.Copy(os.Stdout, reader); err != nil {
		return err
	}

	// Ensure trailing newline
	if len(header) > 0 && header[len(header)-1] != '\n' {
		fmt.Println()
	}
	return nil
}

func isBinary(data []byte) bool {
	for _, b := range data {
		if b == 0 {
			return true
		}
	}
	return false
}

func formatSize(bytes int64) string {
	switch {
	case bytes >= 1<<30:
		return fmt.Sprintf("%.1f GB", float64(bytes)/float64(1<<30))
	case bytes >= 1<<20:
		return fmt.Sprintf("%.1f MB", float64(bytes)/float64(1<<20))
	case bytes >= 1<<10:
		return fmt.Sprintf("%.1f KB", float64(bytes)/float64(1<<10))
	default:
		return fmt.Sprintf("%d B", bytes)
	}
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
