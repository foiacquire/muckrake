package cli

import (
	"fmt"
	"os/user"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/resolve"
)

// signTargets picks the file set to sign. With a subject the positional
// args are just (sign-name), otherwise (reference, sign-name).
func signTargets(ctx *context.Context, args []string) ([]string, string, error) {
	if resolve.HasNarrowSubject(ctx) {
		if len(args) < 1 {
			return nil, "", fmt.Errorf("usage: mkrk :<ref> add sign <name> --pipeline <name>")
		}
		rels, err := resolve.SubjectRelPaths(ctx)
		if err != nil {
			return nil, "", err
		}
		return rels, args[0], nil
	}
	if len(args) < 2 {
		return nil, "", fmt.Errorf("usage: mkrk add sign <reference> <name> --pipeline <name>")
	}
	rels, err := resolve.RefRelPaths(ctx, args[0])
	if err != nil {
		return nil, "", err
	}
	return rels, args[1], nil
}

func whoami() string {
	if u, err := user.Current(); err == nil {
		return u.Username
	}
	return "unknown"
}
