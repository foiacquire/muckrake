package cmd

import (
	"flag"
	"fmt"
	"os"
	"os/user"
	"path/filepath"
	"time"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/models"
)

func RunSign(args []string) error {
	fs := flag.NewFlagSet("sign", flag.ExitOnError)
	remove := fs.Bool("remove", false, "revoke sign instead of creating")
	fs.BoolVar(remove, "r", false, "shorthand for --remove")
	pipelineName := fs.String("pipeline", "", "pipeline name")
	// Extract positional args (reference and sign-name) from mixed flag/arg list
	var positional []string
	var flagArgs []string
	for i := 0; i < len(args); i++ {
		if args[i] == "--remove" || args[i] == "-r" {
			flagArgs = append(flagArgs, args[i])
		} else if args[i] == "--pipeline" && i+1 < len(args) {
			flagArgs = append(flagArgs, args[i], args[i+1])
			i++
		} else if len(args[i]) > 0 && args[i][0] == '-' {
			flagArgs = append(flagArgs, args[i])
		} else {
			positional = append(positional, args[i])
		}
	}
	fs.Parse(flagArgs)

	if len(positional) < 2 {
		return fmt.Errorf("usage: mkrk sign [--remove] <reference> <sign-name> --pipeline <name>")
	}

	rawRef := positional[0]
	signName := positional[1]

	if *pipelineName == "" {
		return fmt.Errorf("--pipeline is required")
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

	paths, err := resolveToRelPaths(ctx, rawRef)
	if err != nil {
		return err
	}
	if len(paths) != 1 {
		return fmt.Errorf("sign requires exactly one file, got %d", len(paths))
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

	pipeline, err := ctx.ProjectDb.GetPipelineByName(*pipelineName)
	if err != nil || pipeline == nil {
		return fmt.Errorf("pipeline '%s' not found", *pipelineName)
	}

	if *remove {
		return revokeSign(ctx, *file.ID, *pipeline.ID, signName, relPath)
	}
	return createSign(ctx, *file.ID, *pipeline.ID, hash, signName, pipeline, relPath)
}

func createSign(ctx *context.Context, fileID, pipelineID int64, hash, signName string, pipeline *models.Pipeline, relPath string) error {
	// Validate sign name is a valid transition requirement
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

	signer := whoami()
	now := time.Now().UTC().Format(time.RFC3339)

	sign := &models.Sign{
		PipelineID: pipelineID,
		FileID:     fileID,
		FileHash:   hash,
		SignName:   signName,
		Signer:     signer,
		SignedAt:   now,
	}

	id, err := ctx.ProjectDb.InsertSign(sign)
	if err != nil {
		return err
	}

	fmt.Fprintf(os.Stderr, "Signed '%s' as '%s' in pipeline '%s' (id %d)\n",
		relPath, signName, pipeline.Name, id)
	return nil
}

func revokeSign(ctx *context.Context, fileID, pipelineID int64, signName, relPath string) error {
	signs, err := ctx.ProjectDb.GetValidSignsForFilePipeline(fileID, pipelineID, "")
	if err != nil {
		return err
	}

	// Find the sign by name (get all valid signs, filter by name)
	allSigns, err := ctx.ProjectDb.GetSignsForFile(fileID)
	if err != nil {
		return err
	}

	now := time.Now().UTC().Format(time.RFC3339)
	revoked := 0
	for _, s := range allSigns {
		if s.SignName == signName && s.PipelineID == pipelineID && s.RevokedAt == nil && s.ID != nil {
			if _, err := ctx.ProjectDb.RevokeSign(*s.ID, now); err != nil {
				return err
			}
			revoked++
		}
	}
	_ = signs

	if revoked == 0 {
		return fmt.Errorf("no active sign '%s' found for '%s'", signName, relPath)
	}

	fmt.Fprintf(os.Stderr, "Revoked '%s' on '%s' (%d sign(s))\n", signName, relPath, revoked)
	return nil
}

func whoami() string {
	if u, err := user.Current(); err == nil {
		return u.Username
	}
	return "unknown"
}
