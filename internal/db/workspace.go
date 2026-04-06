package db

import (
	"database/sql"
	"fmt"
	"os"
	"time"

	_ "modernc.org/sqlite"

	"go.foia.dev/muckrake/internal/models"
)

// WorkspaceDb wraps a connection to a .mksp workspace database.
type WorkspaceDb struct {
	db *sql.DB
}

// ProjectRow represents a registered project in the workspace.
type ProjectRow struct {
	ID          int64
	Name        string
	Path        string
	Description *string
	CreatedAt   string
}

// CreateWorkspace creates a new workspace database at the given path.
func CreateWorkspace(path string) (*WorkspaceDb, error) {
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, fmt.Errorf("create workspace db: %w", err)
	}
	if err := configureConn(db); err != nil {
		db.Close()
		return nil, err
	}
	if _, err := db.Exec(WorkspaceSchema); err != nil {
		db.Close()
		return nil, fmt.Errorf("create workspace schema: %w", err)
	}
	return &WorkspaceDb{db: db}, nil
}

// OpenWorkspace opens an existing workspace database.
// Runs schema with IF NOT EXISTS to add any new tables from newer versions.
func OpenWorkspace(path string) (*WorkspaceDb, error) {
	if _, err := os.Stat(path); err != nil {
		return nil, fmt.Errorf("workspace database not found: %s", path)
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, fmt.Errorf("open workspace db: %w", err)
	}
	if err := configureConn(db); err != nil {
		db.Close()
		return nil, err
	}
	if _, err := db.Exec(WorkspaceSchema); err != nil {
		db.Close()
		return nil, fmt.Errorf("migrate workspace schema: %w", err)
	}
	if err := MigrateWorkspace(db); err != nil {
		db.Close()
		return nil, fmt.Errorf("migrate workspace data: %w", err)
	}
	return &WorkspaceDb{db: db}, nil
}

// Close closes the database connection.
func (w *WorkspaceDb) Close() error {
	return w.db.Close()
}

// DB returns the underlying sql.DB.
func (w *WorkspaceDb) DB() *sql.DB {
	return w.db
}

// --- Config ---

func (w *WorkspaceDb) GetConfig(key string) (*string, error) {
	var val string
	err := w.db.QueryRow(`SELECT value FROM workspace_config WHERE key = ?`, key).Scan(&val)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	return &val, nil
}

func (w *WorkspaceDb) SetConfig(key, value string) error {
	_, err := w.db.Exec(
		`INSERT INTO workspace_config (key, value) VALUES (?, ?)
		 ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
		key, value,
	)
	return err
}

// --- Projects ---

func (w *WorkspaceDb) RegisterProject(name, path string, description *string) (int64, error) {
	if err := models.ValidateScopeName(name); err != nil {
		return 0, err
	}
	now := time.Now().UTC().Format(time.RFC3339)
	res, err := w.db.Exec(
		`INSERT INTO scopes (name, scope_type, pattern, description, created_at)
		 VALUES (?, 'project', ?, ?, ?)`,
		name, path, description, now,
	)
	if err != nil {
		return 0, fmt.Errorf("register project: %w", err)
	}
	return res.LastInsertId()
}

func (w *WorkspaceDb) ListProjects() ([]ProjectRow, error) {
	rows, err := w.db.Query(
		`SELECT id, name, COALESCE(pattern, ''), description, COALESCE(created_at, '')
		 FROM scopes WHERE scope_type = 'project' ORDER BY name`,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var projects []ProjectRow
	for rows.Next() {
		var p ProjectRow
		if err := rows.Scan(&p.ID, &p.Name, &p.Path, &p.Description, &p.CreatedAt); err != nil {
			return nil, err
		}
		projects = append(projects, p)
	}
	return projects, rows.Err()
}

func (w *WorkspaceDb) GetProjectByName(name string) (*ProjectRow, error) {
	var p ProjectRow
	err := w.db.QueryRow(
		`SELECT id, name, COALESCE(pattern, ''), description, COALESCE(created_at, '')
		 FROM scopes WHERE scope_type = 'project' AND name = ?`, name,
	).Scan(&p.ID, &p.Name, &p.Path, &p.Description, &p.CreatedAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	return &p, nil
}

func (w *WorkspaceDb) ProjectCount() (int64, error) {
	var n int64
	err := w.db.QueryRow(`SELECT COUNT(*) FROM scopes WHERE scope_type = 'project'`).Scan(&n)
	return n, err
}

// InsertWorkspaceScope inserts a scope into the workspace DB.
func InsertWorkspaceScope(w *WorkspaceDb, s *models.Scope) (int64, error) {
	catType := ""
	if s.CategoryType != nil {
		catType = string(*s.CategoryType)
	}
	res, err := w.db.Exec(
		`INSERT INTO scopes (name, scope_type, pattern, category_type, description, created_at)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		s.Name, string(s.ScopeType), s.Pattern, catType, s.Description, s.CreatedAt,
	)
	if err != nil {
		return 0, fmt.Errorf("insert workspace scope: %w", err)
	}
	return res.LastInsertId()
}

// InsertWorkspaceScopePolicy inserts a policy for a workspace scope.
func InsertWorkspaceScopePolicy(w *WorkspaceDb, scopeID int64, level models.ProtectionLevel) error {
	_, err := w.db.Exec(
		`INSERT INTO scope_policy (scope_id, protection_level) VALUES (?, ?)
		 ON CONFLICT(scope_id) DO UPDATE SET protection_level = excluded.protection_level`,
		scopeID, string(level),
	)
	return err
}
