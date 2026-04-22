package cmd

import (
	"bufio"
	"flag"
	"fmt"
	"io"
	"os"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/resolve"
)

func RunRead(ctx *context.Context, args []string) error {
	fs := flag.NewFlagSet("read", flag.ExitOnError)
	raw := fs.Bool("raw", false, "no color or decoration")
	pathFlag := fs.Bool("path", false, "show file path before content")
	fs.Parse(args)

	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	paths, err := readTargets(ctx, fs.Args())
	if err != nil {
		return err
	}

	total := 0
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

	if total == 0 {
		fmt.Fprintln(os.Stderr, "(no files)")
	}
	return nil
}

func readTargets(ctx *context.Context, args []string) ([]string, error) {
	if resolve.HasNarrowSubject(ctx) {
		return resolve.SubjectFiles(ctx)
	}
	if len(args) == 0 {
		return nil, fmt.Errorf("usage: mkrk :<ref> read  |  mkrk read <reference> [...]")
	}
	var all []string
	for _, raw := range args {
		paths, err := resolve.Ref(ctx, raw)
		if err != nil {
			return nil, err
		}
		all = append(all, paths...)
	}
	return all, nil
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

	if _, err := io.Copy(os.Stdout, reader); err != nil {
		return err
	}

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
