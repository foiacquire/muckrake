package cmd

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/generator"
	"go.foia.dev/muckrake/internal/integrity"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/resolve"
	"go.foia.dev/muckrake/internal/walk"
)

// RunGenerated dispatches a generated verb (e.g. "tool") to the correct
// generator based on the first argument, then executes the resolved tool
// file with the remaining arguments. Tools always execute once; when the
// subject spans multiple projects (workspace-wide refs), files from every
// context are aggregated into a single invocation.
func RunGenerated(ctxs []*context.Context, gens []generator.Generator, verb string, args []string) error {
	if len(ctxs) == 0 {
		return fmt.Errorf("%s: no context", verb)
	}
	primary := ctxs[0]

	if len(args) == 0 {
		return fmt.Errorf("%s: expected tool name as first argument", verb)
	}

	applicable := filterByVerb(gens, verb)
	if len(applicable) == 0 {
		return fmt.Errorf("no generators registered for verb %q", verb)
	}

	if strings.TrimSpace(args[0]) == "" || args[0] == ":" {
		return fmt.Errorf("%s: empty tool name", verb)
	}

	gen, toolPath, err := resolveTool(args[0], applicable, primary)
	if err != nil {
		return err
	}

	var inputPaths []string
	for _, ctx := range ctxs {
		paths, err := resolve.SubjectFiles(ctx)
		if err != nil {
			return err
		}
		inputPaths = append(inputPaths, paths...)
	}
	toolArgs := append(append([]string(nil), args[1:]...), inputPaths...)

	return execTool(gen, toolPath, toolArgs, inputPaths, primary)
}

func filterByVerb(gens []generator.Generator, verb string) []generator.Generator {
	var out []generator.Generator
	for _, g := range gens {
		if g.Verb == verb {
			out = append(out, g)
		}
	}
	return out
}

// resolveTool interprets the first argument as a tool identifier, returning
// the generator that owns it and the absolute path to the tool file.
func resolveTool(arg string, gens []generator.Generator, ctx *context.Context) (generator.Generator, string, error) {
	explicitProject, toolName := splitProjectAndName(arg)

	if explicitProject != "" {
		for _, g := range gens {
			if g.ProjectName == explicitProject {
				path, err := findToolFile(g, toolName)
				if err != nil {
					return generator.Generator{}, "", err
				}
				if path == "" {
					return generator.Generator{}, "", fmt.Errorf("no tool %q in project %q", toolName, explicitProject)
				}
				return g, path, nil
			}
		}
		return generator.Generator{}, "", fmt.Errorf("no generator for project %q", explicitProject)
	}

	// Bare name: prefer current project, fall back to builtins.
	current := currentProjectName(ctx)
	var ordered []generator.Generator
	for _, g := range gens {
		if g.ProjectName == current {
			ordered = append(ordered, g)
		}
	}
	for _, g := range gens {
		if g.ProjectName != current {
			ordered = append(ordered, g)
		}
	}

	for _, g := range ordered {
		path, err := findToolFile(g, toolName)
		if err != nil {
			return generator.Generator{}, "", err
		}
		if path != "" {
			return g, path, nil
		}
	}
	return generator.Generator{}, "", fmt.Errorf("no tool %q found (use :project.name to disambiguate)", toolName)
}

// splitProjectAndName pulls an explicit project prefix from a reference-style
// first argument. Returns (project, name). project is empty for bare names.
// Intermediate scope segments are ignored — auto-scope supplies the generator's
// own scope, so :project.tool and :project.tools.tool both resolve the same.
func splitProjectAndName(arg string) (string, string) {
	body := stripLeadingDot(strings.TrimPrefix(arg, ":"))
	if body == "" {
		return "", ""
	}
	parts := strings.Split(body, ".")
	if !strings.HasPrefix(arg, ":") || len(parts) == 1 {
		return "", parts[len(parts)-1]
	}
	return parts[0], parts[len(parts)-1]
}

func stripLeadingDot(s string) string {
	if strings.HasPrefix(s, ".") {
		return s[1:]
	}
	return s
}

// findToolFile walks the generator's scope pattern looking for a file whose
// basename (with or without extension) matches toolName.
func findToolFile(g generator.Generator, toolName string) (string, error) {
	if g.IsBuiltin {
		// Built-in tools live in a Go-side registry, not on disk.
		// No registry entries yet → no matches.
		return "", nil
	}
	if g.ProjectRoot == "" || g.Scope.Pattern == nil {
		return "", nil
	}
	patterns := patternsForScope(*g.Scope.Pattern)
	entries, err := walk.WalkAndCollect(g.ProjectRoot, patterns)
	if err != nil {
		return "", err
	}
	for _, rel := range entries {
		base := filepath.Base(rel)
		if base == toolName || stripExt(base) == toolName {
			return filepath.Join(g.ProjectRoot, rel), nil
		}
	}
	return "", nil
}

func patternsForScope(pattern string) []string {
	base := models.NameFromPattern(pattern)
	return []string{base + "/*", base + "/**/*"}
}

func stripExt(name string) string {
	ext := filepath.Ext(name)
	if ext == "" {
		return name
	}
	return name[:len(name)-len(ext)]
}

func currentProjectName(ctx *context.Context) string {
	if ctx != nil && ctx.ProjectName != nil {
		return *ctx.ProjectName
	}
	return ""
}

// execTool runs the resolved tool file with the given arguments, passing
// through stdio and injecting muckrake environment variables. On clean exit
// it ingests any files the tool produced in MKRK_OUTPUT_DIR.
func execTool(g generator.Generator, path string, args, inputPaths []string, ctx *context.Context) error {
	privacy := privacySettings(ctx)
	announcePrivacy(privacy)

	inputHashes := hashInputs(inputPaths)

	outputDir, err := os.MkdirTemp("", "mkrk-tool-out-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(outputDir)

	env := buildEnv(g, ctx, privacy)
	env = appendKV(env, "MKRK_OUTPUT_DIR", outputDir)

	cmd := exec.Command(path, args...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Env = env
	if err := cmd.Run(); err != nil {
		return err
	}

	return ingestOutputs(ctx, g, path, outputDir, inputHashes)
}

func hashInputs(paths []string) []string {
	var hashes []string
	for _, p := range paths {
		info, err := os.Stat(p)
		if err != nil || info.IsDir() {
			continue
		}
		h, err := integrity.HashFile(p)
		if err != nil {
			continue
		}
		hashes = append(hashes, h)
	}
	return hashes
}

func ingestOutputs(ctx *context.Context, g generator.Generator, toolPath, outputDir string, inputHashes []string) error {
	if ctx == nil || ctx.ProjectDb == nil || ctx.Kind != context.ContextProject {
		return nil
	}
	rels, err := listFiles(outputDir)
	if err != nil {
		return err
	}
	if len(rels) == 0 {
		return nil
	}

	toolName := stripExt(filepath.Base(toolPath))
	ts := time.Now().UTC().Format("20060102-150405")
	destDir := filepath.Join(ctx.ProjectRoot, "outputs", fmt.Sprintf("%s-%s", toolName, ts))
	if err := os.MkdirAll(destDir, 0o755); err != nil {
		return err
	}

	ingested := 0
	for _, rel := range rels {
		src := filepath.Join(outputDir, rel)
		dst := filepath.Join(destDir, rel)
		if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
			continue
		}
		if err := copyFile(src, dst); err != nil {
			continue
		}
		if ingestOutputFile(ctx, dst, toolName, inputHashes) {
			ingested++
		}
	}

	if ingested > 0 {
		rel, _ := filepath.Rel(ctx.ProjectRoot, destDir)
		fmt.Fprintf(os.Stderr, "ingested %d output file(s) to %s\n", ingested, rel)
	}
	return nil
}

func ingestOutputFile(ctx *context.Context, path, toolName string, inputHashes []string) bool {
	hash, fp, err := integrity.HashAndFingerprint(path)
	if err != nil {
		return false
	}

	provenance := provenanceJSON(toolName, inputHashes)
	file := &models.TrackedFile{
		SHA256:      hash,
		Fingerprint: fp.ToJSON(),
		IngestedAt:  time.Now().UTC().Format(time.RFC3339),
		Provenance:  &provenance,
	}
	fileID, err := ctx.ProjectDb.InsertFile(file)
	if err != nil {
		// Already tracked — look up existing ID for link creation.
		existing, _ := ctx.ProjectDb.GetFileByHash(hash)
		if existing == nil || existing.ID == nil {
			return false
		}
		fileID = *existing.ID
	}

	for _, inHash := range inputHashes {
		inFile, _ := ctx.ProjectDb.GetFileByHash(inHash)
		if inFile == nil || inFile.ID == nil {
			continue
		}
		ctx.ProjectDb.InsertFileLink(*inFile.ID, fileID, "derived_from", nil)
	}
	return true
}

func provenanceJSON(toolName string, inputHashes []string) string {
	payload := struct {
		Tool      string   `json:"tool"`
		Inputs    []string `json:"inputs"`
		Timestamp string   `json:"timestamp"`
	}{
		Tool:      toolName,
		Inputs:    inputHashes,
		Timestamp: time.Now().UTC().Format(time.RFC3339),
	}
	b, _ := json.Marshal(payload)
	return string(b)
}

func listFiles(root string) ([]string, error) {
	var rels []string
	err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() {
			return err
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		rels = append(rels, rel)
		return nil
	})
	return rels, err
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()
	_, err = io.Copy(out, in)
	return err
}

type privacyConfig struct {
	enabled bool
	socks   string
}

// DefaultSocksProxy is the default Tor SOCKS endpoint. Tools inherit this
// unless the workspace disables privacy or points elsewhere.
const DefaultSocksProxy = "socks5h://127.0.0.1:9050"

func privacySettings(ctx *context.Context) privacyConfig {
	cfg := privacyConfig{enabled: true, socks: DefaultSocksProxy}

	if override := os.Getenv("MKRK_SOCKS"); override != "" {
		cfg.socks = override
	}

	if ctx != nil && ctx.Workspace != nil && ctx.Workspace.Db != nil {
		if v, _ := ctx.Workspace.Db.GetConfig("privacy"); v != nil && *v == "off" {
			cfg.enabled = false
		}
		if v, _ := ctx.Workspace.Db.GetConfig("socks_proxy"); v != nil && *v != "" {
			cfg.socks = *v
		}
	}

	return cfg
}

func announcePrivacy(p privacyConfig) {
	if p.enabled {
		fmt.Fprintf(os.Stderr, "privacy: routing through %s (tool may not respect this)\n", p.socks)
		return
	}
	fmt.Fprintln(os.Stderr, "privacy: DISABLED — tool runs with unrestricted network (re-enable via workspace config 'privacy=on')")
}

func buildEnv(g generator.Generator, ctx *context.Context, p privacyConfig) []string {
	env := append([]string(nil), os.Environ()...)
	env = appendKV(env, "MKRK_GENERATOR_VERB", g.Verb)
	env = appendKV(env, "MKRK_GENERATOR_SCOPE", g.Scope.Name)
	if ctx != nil {
		if ctx.ProjectName != nil {
			env = appendKV(env, "MKRK_PROJECT", *ctx.ProjectName)
		}
		if ctx.ProjectRoot != "" {
			env = appendKV(env, "MKRK_PROJECT_ROOT", ctx.ProjectRoot)
		}
		if ctx.Workspace != nil {
			env = appendKV(env, "MKRK_WORKSPACE_ROOT", ctx.Workspace.Root)
		}
	}
	if p.enabled {
		for _, key := range []string{"http_proxy", "HTTP_PROXY", "https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"} {
			env = appendKV(env, key, p.socks)
		}
		env = appendKV(env, "MKRK_SOCKS", p.socks)
	}
	return env
}

func appendKV(env []string, key, value string) []string {
	prefix := key + "="
	for i, e := range env {
		if strings.HasPrefix(e, prefix) {
			env[i] = prefix + value
			return env
		}
	}
	return append(env, prefix+value)
}
