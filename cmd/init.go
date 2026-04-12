package cmd

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"

	"go.foia.dev/muckrake/internal/db"
	"go.foia.dev/muckrake/internal/models"
)

var defaultCategories = []struct {
	name, pattern, catType, protection, description string
}{
	{"evidence", "evidence/**", "files", "immutable", "Evidence files"},
	{"sources", "sources/**", "files", "immutable", "Source materials"},
	{"analysis", "analysis/**", "files", "protected", "Analysis documents"},
	{"notes", "notes/**", "files", "editable", "Working notes"},
	{"tools", "tools/**", "tools", "editable", "Project tools"},
}

func RunInit(args []string) error {
	fs := flag.NewFlagSet("init", flag.ExitOnError)
	workspace := fs.String("workspace", "", "initialize workspace with projects directory")
	noCategories := fs.Bool("no-categories", false, "skip default categories")
	fs.Parse(args)

	cwd, err := os.Getwd()
	if err != nil {
		return err
	}

	if *workspace != "" {
		return initWorkspace(cwd, *workspace, *noCategories)
	}

	name := ""
	if fs.NArg() > 0 {
		name = fs.Arg(0)
	}
	return initProject(cwd, name, *noCategories)
}

func initProject(cwd, name string, noCategories bool) error {
	projectDir := cwd

	// If inside a workspace and a name is given, resolve via projects_dir
	ws := findWorkspace(cwd)
	if name != "" {
		if err := models.ValidateScopeName(name); err != nil {
			return err
		}
		if ws != nil {
			projectsDir, _ := ws.db.GetConfig("projects_dir")
			if projectsDir != nil {
				projectDir = filepath.Join(ws.root, *projectsDir, name)
			} else {
				projectDir = filepath.Join(cwd, name)
			}
		} else {
			projectDir = filepath.Join(cwd, name)
		}
	}

	dbPath := filepath.Join(projectDir, ".mkrk")
	if fileExists(dbPath) {
		return fmt.Errorf("project already exists in %s", projectDir)
	}

	if err := os.MkdirAll(projectDir, 0o755); err != nil {
		return err
	}

	pdb, err := db.CreateProject(dbPath)
	if err != nil {
		return err
	}
	defer pdb.Close()

	if !noCategories {
		for _, c := range defaultCategories {
			ct := models.CategoryType(c.catType)
			scope := &models.Scope{
				Name:         c.name,
				ScopeType:    models.ScopeTypeCategory,
				Pattern:      &c.pattern,
				CategoryType: &ct,
				Description:  &c.description,
			}
			id, err := pdb.InsertScope(scope)
			if err != nil {
				return fmt.Errorf("insert category %s: %w", c.name, err)
			}
			level, _ := models.ParseProtectionLevel(c.protection)
			pdb.InsertScopePolicy(id, level)

			catDir := filepath.Join(projectDir, models.NameFromPattern(c.pattern))
			os.MkdirAll(catDir, 0o755)
		}

		if err := installToolsDispatch(pdb); err != nil {
			return fmt.Errorf("install tools dispatch: %w", err)
		}
	}

	// Register in workspace if applicable
	if ws != nil && name != "" {
		rel, _ := filepath.Rel(ws.root, projectDir)
		rel = filepath.ToSlash(rel)
		existing, _ := ws.db.GetProjectByName(name)
		if existing == nil {
			ws.db.RegisterProject(name, rel, nil)
			fmt.Fprintf(os.Stderr, "  Registered in workspace\n")
		}
	}
	if ws != nil {
		ws.db.Close()
	}

	fmt.Fprintf(os.Stderr, "Initialized project in %s\n", projectDir)
	if !noCategories {
		fmt.Fprintf(os.Stderr, "  %d categories configured\n", len(defaultCategories))
	}

	return nil
}

func installToolsDispatch(pdb *db.ProjectDb) error {
	desc := "Make tools executable and register the `tool` CLI verb"
	rs := &models.Ruleset{Name: "tools_dispatch", Description: &desc}
	rsID, err := pdb.InsertRuleset(rs)
	if err != nil {
		return err
	}

	if _, err := pdb.InsertRulesetRule(&models.RulesetRule{
		RulesetID:  rsID,
		Priority:   0,
		ActionType: models.ActionMakeExecutable,
	}); err != nil {
		return err
	}

	verb := "tool"
	autoScope := true
	if _, err := pdb.InsertRulesetRule(&models.RulesetRule{
		RulesetID:  rsID,
		Priority:   1,
		ActionType: models.ActionGenerateCommand,
		ActionConfig: models.RulesetActionConfig{
			Verb:      &verb,
			AutoScope: &autoScope,
		},
	}); err != nil {
		return err
	}

	if _, err := pdb.SubscribeRuleset(rsID, ":.tools"); err != nil {
		return err
	}
	return nil
}

func initWorkspace(cwd, projectsDir string, noCategories bool) error {
	dbPath := filepath.Join(cwd, ".mksp")
	if fileExists(dbPath) {
		return fmt.Errorf("workspace already exists in %s", cwd)
	}
	if fileExists(filepath.Join(cwd, ".mkrk")) {
		return fmt.Errorf("project already exists in %s", cwd)
	}

	wdb, err := db.CreateWorkspace(dbPath)
	if err != nil {
		return err
	}
	defer wdb.Close()

	wdb.SetConfig("projects_dir", projectsDir)
	os.MkdirAll(filepath.Join(cwd, projectsDir), 0o755)

	if !noCategories {
		for _, c := range defaultCategories {
			ct := models.CategoryType(c.catType)
			scope := &models.Scope{
				Name:         c.name,
				ScopeType:    models.ScopeTypeCategory,
				Pattern:      &c.pattern,
				CategoryType: &ct,
				Description:  &c.description,
			}
			// Workspace stores default categories as scopes too
			id, err := db.InsertWorkspaceScope(wdb, scope)
			if err != nil {
				continue
			}
			level, _ := models.ParseProtectionLevel(c.protection)
			db.InsertWorkspaceScopePolicy(wdb, id, level)
		}
	}

	fmt.Fprintf(os.Stderr, "Initialized workspace in %s\n", cwd)
	fmt.Fprintf(os.Stderr, "  Projects directory: %s\n", projectsDir)

	return nil
}

type workspaceInfo struct {
	root string
	db   *db.WorkspaceDb
}

func findWorkspace(cwd string) *workspaceInfo {
	dir := cwd
	for {
		mksp := filepath.Join(dir, ".mksp")
		if fileExists(mksp) {
			wdb, err := db.OpenWorkspace(mksp)
			if err != nil {
				return nil
			}
			return &workspaceInfo{root: dir, db: wdb}
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return nil
		}
		dir = parent
	}
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}
