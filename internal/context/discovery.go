package context

import (
	"fmt"
	"os"
	"path/filepath"

	"go.foia.dev/muckrake/internal/db"
)

// ContextKind identifies whether we're in a project, workspace, or neither.
type ContextKind int

const (
	ContextNone ContextKind = iota
	ContextProject
	ContextWorkspace
)

// Context holds the discovered project/workspace state for the current CWD.
type Context struct {
	Kind         ContextKind
	ProjectRoot  string
	ProjectDb    *db.ProjectDb
	ProjectName  *string
	Workspace    *WorkspaceContext
}

// WorkspaceContext holds workspace info when inside a workspace.
type WorkspaceContext struct {
	Root string
	Db   *db.WorkspaceDb
}

// Discover walks up from cwd to find .mkrk (project) and .mksp (workspace) markers.
func Discover(cwd string) (*Context, error) {
	projectRoot, workspaceRoot := findMarkers(cwd)

	switch {
	case projectRoot != "" && workspaceRoot != "":
		pdb, err := db.OpenProject(filepath.Join(projectRoot, ".mkrk"))
		if err != nil {
			return nil, err
		}
		wdb, err := db.OpenWorkspace(filepath.Join(workspaceRoot, ".mksp"))
		if err != nil {
			pdb.Close()
			return nil, err
		}
		name := lookupProjectName(projectRoot, workspaceRoot, wdb)
		return &Context{
			Kind:        ContextProject,
			ProjectRoot: projectRoot,
			ProjectDb:   pdb,
			ProjectName: name,
			Workspace: &WorkspaceContext{
				Root: workspaceRoot,
				Db:   wdb,
			},
		}, nil

	case projectRoot != "":
		pdb, err := db.OpenProject(filepath.Join(projectRoot, ".mkrk"))
		if err != nil {
			return nil, err
		}
		return &Context{
			Kind:        ContextProject,
			ProjectRoot: projectRoot,
			ProjectDb:   pdb,
		}, nil

	case workspaceRoot != "":
		wdb, err := db.OpenWorkspace(filepath.Join(workspaceRoot, ".mksp"))
		if err != nil {
			return nil, err
		}
		return &Context{
			Kind: ContextWorkspace,
			Workspace: &WorkspaceContext{
				Root: workspaceRoot,
				Db:   wdb,
			},
		}, nil

	default:
		return &Context{Kind: ContextNone}, nil
	}
}

// OpenProjectContext creates a project context for a known project inside a workspace.
// Used by workspace dispatch to create per-project contexts without re-discovery.
func OpenProjectContext(projectRoot, projectName string, ws *WorkspaceContext) (*Context, error) {
	pdb, err := db.OpenProject(filepath.Join(projectRoot, ".mkrk"))
	if err != nil {
		return nil, err
	}
	return &Context{
		Kind:        ContextProject,
		ProjectRoot: projectRoot,
		ProjectDb:   pdb,
		ProjectName: &projectName,
		Workspace:   ws,
	}, nil
}

// RequireProject returns project root and db, or error if not in a project.
func (c *Context) RequireProject() (string, *db.ProjectDb, error) {
	if c.Kind != ContextProject {
		return "", nil, fmt.Errorf("not in a project")
	}
	return c.ProjectRoot, c.ProjectDb, nil
}

// Close releases database connections.
func (c *Context) Close() {
	if c.ProjectDb != nil {
		c.ProjectDb.Close()
	}
	if c.Workspace != nil && c.Workspace.Db != nil {
		c.Workspace.Db.Close()
	}
}

func findMarkers(cwd string) (projectRoot, workspaceRoot string) {
	dir := cwd
	for {
		if projectRoot == "" {
			if fileExists(filepath.Join(dir, ".mkrk")) {
				projectRoot = dir
			}
		}
		if workspaceRoot == "" {
			if fileExists(filepath.Join(dir, ".mksp")) {
				workspaceRoot = dir
			}
		}
		if (projectRoot != "" && workspaceRoot != "") {
			break
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	return
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func lookupProjectName(projectRoot, workspaceRoot string, wdb *db.WorkspaceDb) *string {
	rel, err := filepath.Rel(workspaceRoot, projectRoot)
	if err != nil {
		return nil
	}
	rel = filepath.ToSlash(rel)
	projects, err := wdb.ListProjects()
	if err != nil {
		return nil
	}
	for _, p := range projects {
		if p.Path == rel {
			return &p.Name
		}
	}
	return nil
}
