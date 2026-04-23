package cli

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"strings"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/models"
)

func RunPipeline(ctx *context.Context, args []string) error {
	fs := flag.NewFlagSet("pipeline", flag.ExitOnError)
	remove := fs.Bool("remove", false, "remove pipeline")
	fs.BoolVar(remove, "r", false, "shorthand for --remove")
	states := fs.String("states", "", "comma-separated state names (e.g., draft,review,published)")
	transitions := fs.String("transitions", "", "JSON transitions (optional, defaults to linear)")

	name, flagArgs := extractName(args)
	fs.Parse(flagArgs)

	if name == "" {
		return fmt.Errorf("usage: mkrk pipeline [--remove] <name> [--states draft,review,published]")
	}
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	if *remove {
		return removePipeline(ctx, name)
	}
	return createPipeline(ctx, name, *states, *transitions)
}

func createPipeline(ctx *context.Context, name, statesStr, transitionsJSON string) error {
	if statesStr == "" {
		return fmt.Errorf("--states required when creating a pipeline")
	}

	stateList := strings.Split(statesStr, ",")
	for i := range stateList {
		stateList[i] = strings.TrimSpace(stateList[i])
	}

	pl := &models.Pipeline{
		Name:   name,
		States: stateList,
	}

	if transitionsJSON != "" {
		if err := parseTransitions(transitionsJSON, pl); err != nil {
			return err
		}
	} else {
		pl.Transitions = models.DefaultTransitions(stateList)
	}

	if err := pl.Validate(); err != nil {
		return err
	}

	existing, _ := ctx.ProjectDb.GetPipelineByName(name)
	if existing != nil {
		return fmt.Errorf("pipeline '%s' already exists", name)
	}

	id, err := ctx.ProjectDb.InsertPipeline(pl)
	if err != nil {
		return err
	}

	fmt.Fprintf(os.Stderr, "Created pipeline '%s' (id %d)\n", name, id)
	fmt.Fprintf(os.Stderr, "  States: %s\n", strings.Join(stateList, " -> "))
	return nil
}

func removePipeline(ctx *context.Context, name string) error {
	removed, err := ctx.ProjectDb.RemovePipeline(name)
	if err != nil {
		return err
	}
	if removed == 0 {
		return fmt.Errorf("pipeline '%s' not found", name)
	}
	fmt.Fprintf(os.Stderr, "Removed pipeline '%s'\n", name)
	return nil
}

func extractName(args []string) (string, []string) {
	var name string
	var rest []string
	for _, a := range args {
		if name == "" && !strings.HasPrefix(a, "-") {
			name = a
		} else {
			rest = append(rest, a)
		}
	}
	return name, rest
}

func parseTransitions(jsonStr string, pl *models.Pipeline) error {
	var trans map[string][]string
	if err := json.Unmarshal([]byte(jsonStr), &trans); err != nil {
		return fmt.Errorf("invalid transitions JSON: %w", err)
	}
	pl.Transitions = trans
	return nil
}
