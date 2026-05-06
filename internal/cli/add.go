package cli

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/materialize"
	"go.foia.dev/muckrake/internal/models"
)

// RunAdd creates a project resource. The first argument is the resource
// type (the "noun"). Each noun has its own positional/flag contract.
func RunAdd(ctx *context.Context, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("usage: mkrk add <%s> ...", strings.Join(addNouns(), "|"))
	}
	noun := args[0]
	rest := args[1:]
	handler, ok := addHandlers[noun]
	if !ok {
		return fmt.Errorf("cannot add %q (try: %s)", noun, strings.Join(addNouns(), ", "))
	}
	return handler(ctx, rest)
}

var addHandlers = map[string]func(*context.Context, []string) error{
	"pipeline": addPipeline,
	"ruleset":  addRuleset,
	"tag":      addTag,
	"sign":     addSign,
	"tool":     addTool,
}

func addNouns() []string {
	out := make([]string, 0, len(addHandlers))
	for k := range addHandlers {
		out = append(out, k)
	}
	return out
}

// --- pipeline ---

func addPipeline(ctx *context.Context, args []string) error {
	fs := flag.NewFlagSet("add pipeline", flag.ExitOnError)
	states := fs.String("states", "", "comma-separated state names (e.g., draft,review,published)")
	transitions := fs.String("transitions", "", "JSON transitions (optional, defaults to linear)")
	name, flagArgs := extractName(args)
	fs.Parse(flagArgs)

	if name == "" {
		return fmt.Errorf("usage: mkrk add pipeline <name> --states draft,review,published")
	}
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}
	if *states == "" {
		return fmt.Errorf("--states required when creating a pipeline")
	}

	stateList := strings.Split(*states, ",")
	for i := range stateList {
		stateList[i] = strings.TrimSpace(stateList[i])
	}

	pl := &models.Pipeline{Name: name, States: stateList}
	if *transitions != "" {
		if err := parseTransitions(*transitions, pl); err != nil {
			return err
		}
	} else {
		pl.Transitions = models.DefaultTransitions(stateList)
	}
	if err := pl.Validate(); err != nil {
		return err
	}
	if existing, _ := ctx.ProjectDb.GetPipelineByName(name); existing != nil {
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

// --- ruleset ---

func addRuleset(ctx *context.Context, args []string) error {
	fs := flag.NewFlagSet("add ruleset", flag.ExitOnError)
	desc := fs.String("description", "", "human-readable description")
	name, flagArgs := extractName(args)
	fs.Parse(flagArgs)

	if name == "" {
		return fmt.Errorf("usage: mkrk add ruleset <name> [--description ...]")
	}
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	rs := &models.Ruleset{Name: name}
	if *desc != "" {
		rs.Description = desc
	}
	id, err := ctx.ProjectDb.InsertRuleset(rs)
	if err != nil {
		return fmt.Errorf("create ruleset: %w", err)
	}
	fmt.Fprintf(os.Stderr, "Created ruleset '%s' (id %d)\n", name, id)
	return nil
}

// --- tag ---

func addTag(ctx *context.Context, args []string) error {
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}
	paths, tagName, err := tagTargets(ctx, args)
	if err != nil {
		return err
	}
	if len(paths) == 0 {
		return fmt.Errorf("no files matched")
	}
	return applyTag(ctx, paths, tagName, false)
}

func applyTag(ctx *context.Context, paths []string, tagName string, removing bool) error {
	categories, _ := ctx.ProjectDb.ListCategories()
	for _, relPath := range paths {
		absPath := filepath.Join(ctx.ProjectRoot, relPath)
		hash, fp, err := integrity.HashAndFingerprint(absPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "  ! %s: %v\n", relPath, err)
			continue
		}
		file, err := ctx.ProjectDb.GetFileByHash(hash)
		if err != nil || file == nil || file.ID == nil {
			fmt.Fprintf(os.Stderr, "  ! %s: not tracked (run sync first)\n", relPath)
			continue
		}
		if removing {
			if err := ctx.ProjectDb.RemoveTag(*file.ID, tagName); err != nil {
				fmt.Fprintf(os.Stderr, "  ! %s: %v\n", relPath, err)
				continue
			}
			fmt.Fprintf(os.Stderr, "  - %s !%s\n", relPath, tagName)
		} else {
			if err := ctx.ProjectDb.InsertTag(*file.ID, tagName, hash, fp.ToJSON()); err != nil {
				fmt.Fprintf(os.Stderr, "  ! %s: %v\n", relPath, err)
				continue
			}
			fmt.Fprintf(os.Stderr, "  + %s !%s\n", relPath, tagName)
		}
		tags, _ := ctx.ProjectDb.GetTags(*file.ID)
		matchingCats := matchingCategories(relPath, categories)
		materialize.MaterializeForFile(ctx.ProjectDb, relPath, hash, matchingCats, tags)
	}
	return nil
}

// --- sign ---

func addSign(ctx *context.Context, args []string) error {
	pipelineName, signName, paths, err := signSetup(ctx, args)
	if err != nil {
		return err
	}
	relPath := paths[0]
	absPath := filepath.Join(ctx.ProjectRoot, relPath)
	hash, err := integrity.HashFile(absPath)
	if err != nil {
		return err
	}
	file, err := ctx.ProjectDb.GetFileByHash(hash)
	if err != nil || file == nil || file.ID == nil {
		return fmt.Errorf("file not tracked (run sync first)")
	}
	pipeline, err := ctx.ProjectDb.GetPipelineByName(pipelineName)
	if err != nil || pipeline == nil {
		return fmt.Errorf("pipeline '%s' not found", pipelineName)
	}

	validName := false
	for _, reqs := range pipeline.Transitions {
		for _, r := range reqs {
			if r == signName {
				validName = true
				break
			}
		}
	}
	if !validName {
		return fmt.Errorf("'%s' is not a valid sign name for pipeline '%s'", signName, pipeline.Name)
	}

	sign := &models.Sign{
		PipelineID: *pipeline.ID,
		FileID:     *file.ID,
		FileHash:   hash,
		SignName:   signName,
		Signer:     whoami(),
		SignedAt:   time.Now().UTC().Format(time.RFC3339),
	}
	id, err := ctx.ProjectDb.InsertSign(sign)
	if err != nil {
		return err
	}
	fmt.Fprintf(os.Stderr, "Signed '%s' as '%s' in pipeline '%s' (id %d)\n",
		relPath, signName, pipeline.Name, id)
	return nil
}

// signSetup parses sign args and resolves the target file. Used by add and
// remove sign — both need pipeline name, sign name, and one resolved file.
func signSetup(ctx *context.Context, args []string) (pipelineName, signName string, paths []string, err error) {
	fs := flag.NewFlagSet("sign", flag.ExitOnError)
	plName := fs.String("pipeline", "", "pipeline name")
	positional, flagArgs := splitPositional(args)
	fs.Parse(flagArgs)

	if *plName == "" {
		return "", "", nil, fmt.Errorf("--pipeline is required")
	}
	if ctx.Kind != context.ContextProject {
		return "", "", nil, fmt.Errorf("not in a project")
	}
	rels, name, err := signTargets(ctx, positional)
	if err != nil {
		return "", "", nil, err
	}
	if len(rels) != 1 {
		return "", "", nil, fmt.Errorf("sign requires exactly one file, got %d", len(rels))
	}
	return *plName, name, rels, nil
}

func splitPositional(args []string) ([]string, []string) {
	var pos, flagArgs []string
	for i := 0; i < len(args); i++ {
		a := args[i]
		switch {
		case a == "--pipeline" && i+1 < len(args):
			flagArgs = append(flagArgs, a, args[i+1])
			i++
		case strings.HasPrefix(a, "-"):
			flagArgs = append(flagArgs, a)
		default:
			pos = append(pos, a)
		}
	}
	return pos, flagArgs
}

// --- tool ---

func addTool(ctx *context.Context, args []string) error {
	fs := flag.NewFlagSet("add tool", flag.ExitOnError)
	name, flagArgs := extractName(args)
	fs.Parse(flagArgs)

	if name == "" {
		return fmt.Errorf("usage: mkrk add tool <name>")
	}
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}

	target := filepath.Join(ctx.ProjectRoot, "tools", name)
	if filepath.Ext(target) == "" {
		target += ".sh"
	}
	if _, err := os.Stat(target); err == nil {
		return fmt.Errorf("tool '%s' already exists at %s", name, target)
	}
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return err
	}
	template := "#!/usr/bin/env sh\n# tool: " + name + "\n# files passed as arguments; outputs go to $MKRK_OUTPUT_DIR\n"
	if err := os.WriteFile(target, []byte(template), 0o755); err != nil {
		return err
	}
	rel, _ := filepath.Rel(ctx.ProjectRoot, target)
	fmt.Fprintf(os.Stderr, "Created tool '%s' at %s\n", name, rel)
	return nil
}

