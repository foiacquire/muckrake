package cli

import (
	"fmt"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/resolve"
)

// tagTargets picks the file set to tag. With a subject the positional args
// are just (tag-name), otherwise they are (reference, tag-name).
func tagTargets(ctx *context.Context, args []string) ([]string, string, error) {
	if resolve.HasNarrowSubject(ctx) {
		if len(args) < 1 {
			return nil, "", fmt.Errorf("usage: mkrk :<ref> add tag <name>")
		}
		rels, err := resolve.SubjectRelPaths(ctx)
		if err != nil {
			return nil, "", err
		}
		return rels, args[0], nil
	}
	if len(args) < 2 {
		return nil, "", fmt.Errorf("usage: mkrk add tag <reference> <name>")
	}
	rels, err := resolve.RefRelPaths(ctx, args[0])
	if err != nil {
		return nil, "", err
	}
	return rels, args[1], nil
}
