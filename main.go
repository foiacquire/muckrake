package main

import (
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: mkrk <command> [args...]")
		os.Exit(1)
	}

	fmt.Fprintln(os.Stderr, "mkrk: not yet implemented")
	os.Exit(1)
}
