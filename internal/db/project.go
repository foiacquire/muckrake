package db

import (
	"database/sql"
	"fmt"
	"os"
	"time"

	_ "modernc.org/sqlite"

	"go.foia.dev/muckrake/internal/models"
)

// ProjectDb wraps a connection to a .mkrk project database.
type ProjectDb struct {
	db *sql.DB
}

// CreateProject creates a new project database at the given path.
func CreateProject(path string) (*ProjectDb, error) {
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, fmt.Errorf("create project db: %w", err)
	}
	if err := configureConn(db); err != nil {
		db.Close()
		return nil, err
	}
	if _, err := db.Exec(ProjectSchema); err != nil {
		db.Close()
		return nil, fmt.Errorf("create project schema: %w", err)
	}
	return &ProjectDb{db: db}, nil
}

// OpenProject opens an existing project database.
// Runs schema with IF NOT EXISTS to add any new tables from newer versions.
func OpenProject(path string) (*ProjectDb, error) {
	if _, err := os.Stat(path); err != nil {
		return nil, fmt.Errorf("project database not found: %s", path)
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, fmt.Errorf("open project db: %w", err)
	}
	if err := configureConn(db); err != nil {
		db.Close()
		return nil, err
	}
	// Create any missing tables (all use IF NOT EXISTS)
	if _, err := db.Exec(ProjectSchema); err != nil {
		db.Close()
		return nil, fmt.Errorf("migrate project schema: %w", err)
	}
	// Migrate legacy Rust data if present
	if err := MigrateProject(db); err != nil {
		db.Close()
		return nil, fmt.Errorf("migrate project data: %w", err)
	}
	return &ProjectDb{db: db}, nil
}

// Close closes the database connection.
func (p *ProjectDb) Close() error {
	return p.db.Close()
}

// DB returns the underlying sql.DB for direct access.
func (p *ProjectDb) DB() *sql.DB {
	return p.db
}

func configureConn(db *sql.DB) error {
	if _, err := db.Exec("PRAGMA journal_mode=WAL"); err != nil {
		if _, err := db.Exec("PRAGMA journal_mode=DELETE"); err != nil {
			return fmt.Errorf("configure journal mode: %w", err)
		}
	}
	if _, err := db.Exec("PRAGMA foreign_keys=ON"); err != nil {
		return fmt.Errorf("configure foreign keys: %w", err)
	}
	return nil
}

// --- Scope CRUD ---

func (p *ProjectDb) InsertScope(s *models.Scope) (int64, error) {
	catType := ""
	if s.CategoryType != nil {
		catType = string(*s.CategoryType)
	}
	res, err := p.db.Exec(
		`INSERT INTO scopes (name, scope_type, pattern, category_type, description, created_at)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		s.Name, string(s.ScopeType), s.Pattern, catType, s.Description, s.CreatedAt,
	)
	if err != nil {
		return 0, fmt.Errorf("insert scope: %w", err)
	}
	return res.LastInsertId()
}

func (p *ProjectDb) ListCategories() ([]models.Scope, error) {
	return p.listScopesByType(models.ScopeTypeCategory)
}

func (p *ProjectDb) GetScopeByName(name string) (*models.Scope, error) {
	row := p.db.QueryRow(
		`SELECT id, name, scope_type, pattern, category_type, description, created_at
		 FROM scopes WHERE name = ?`, name,
	)
	return scanScope(row)
}

func (p *ProjectDb) GetCategoryByName(name string) (*models.Scope, error) {
	row := p.db.QueryRow(
		`SELECT id, name, scope_type, pattern, category_type, description, created_at
		 FROM scopes WHERE name = ? AND scope_type = 'category'`, name,
	)
	return scanScope(row)
}

func (p *ProjectDb) MatchCategory(relPath string) (*models.Scope, error) {
	cats, err := p.ListCategories()
	if err != nil {
		return nil, err
	}
	var best *models.Scope
	bestLen := 0
	for i := range cats {
		matched, err := cats[i].Matches(relPath)
		if err != nil {
			return nil, err
		}
		if matched {
			patLen := 0
			if cats[i].Pattern != nil {
				patLen = len(*cats[i].Pattern)
			}
			if patLen > bestLen {
				best = &cats[i]
				bestLen = patLen
			}
		}
	}
	return best, nil
}

func (p *ProjectDb) RemoveScope(id int64) error {
	if _, err := p.db.Exec(`DELETE FROM scope_policy WHERE scope_id = ?`, id); err != nil {
		return fmt.Errorf("remove scope policy: %w", err)
	}
	if _, err := p.db.Exec(`DELETE FROM scopes WHERE id = ?`, id); err != nil {
		return fmt.Errorf("remove scope: %w", err)
	}
	return nil
}

func (p *ProjectDb) listScopesByType(st models.ScopeType) ([]models.Scope, error) {
	rows, err := p.db.Query(
		`SELECT id, name, scope_type, pattern, category_type, description, created_at
		 FROM scopes WHERE scope_type = ?`, string(st),
	)
	if err != nil {
		return nil, fmt.Errorf("list scopes: %w", err)
	}
	defer rows.Close()
	return scanScopes(rows)
}

// --- Scope Policy ---

func (p *ProjectDb) InsertScopePolicy(scopeID int64, level models.ProtectionLevel) error {
	_, err := p.db.Exec(
		`INSERT INTO scope_policy (scope_id, protection_level) VALUES (?, ?)
		 ON CONFLICT(scope_id) DO UPDATE SET protection_level = excluded.protection_level`,
		scopeID, string(level),
	)
	if err != nil {
		return fmt.Errorf("insert scope policy: %w", err)
	}
	return nil
}

func (p *ProjectDb) GetPolicyForScope(scopeID int64) (*models.ProtectionLevel, error) {
	var s string
	err := p.db.QueryRow(
		`SELECT protection_level FROM scope_policy WHERE scope_id = ?`, scopeID,
	).Scan(&s)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("get scope policy: %w", err)
	}
	level, err := models.ParseProtectionLevel(s)
	if err != nil {
		return nil, err
	}
	return &level, nil
}

func (p *ProjectDb) ResolveProtection(relPath string) (models.ProtectionLevel, error) {
	cats, err := p.ListCategories()
	if err != nil {
		return models.ProtectionEditable, err
	}
	var levels []models.ProtectionLevel
	for _, cat := range cats {
		matched, err := cat.Matches(relPath)
		if err != nil {
			return models.ProtectionEditable, err
		}
		if matched && cat.ID != nil {
			level, err := p.GetPolicyForScope(*cat.ID)
			if err != nil {
				return models.ProtectionEditable, err
			}
			if level != nil {
				levels = append(levels, *level)
			}
		}
	}
	return models.Strictest(levels), nil
}

// --- File CRUD ---

func (p *ProjectDb) InsertFile(f *models.TrackedFile) (int64, error) {
	res, err := p.db.Exec(
		`INSERT INTO files (sha256, fingerprint, mime_type, size, ingested_at, provenance)
		 VALUES (?, ?, ?, ?, ?, ?)`,
		f.SHA256, f.Fingerprint, f.MimeType, f.Size, f.IngestedAt, f.Provenance,
	)
	if err != nil {
		return 0, fmt.Errorf("insert file: %w", err)
	}
	return res.LastInsertId()
}

func (p *ProjectDb) GetFileByHash(sha256 string) (*models.TrackedFile, error) {
	row := p.db.QueryRow(
		`SELECT id, sha256, fingerprint, mime_type, size, ingested_at, provenance
		 FROM files WHERE sha256 = ?`, sha256,
	)
	return scanFile(row)
}

func (p *ProjectDb) GetFileByFingerprint(fp string) (*models.TrackedFile, error) {
	row := p.db.QueryRow(
		`SELECT id, sha256, fingerprint, mime_type, size, ingested_at, provenance
		 FROM files WHERE fingerprint = ?`, fp,
	)
	return scanFile(row)
}

func (p *ProjectDb) ListAllFiles() ([]models.TrackedFile, error) {
	rows, err := p.db.Query(
		`SELECT id, sha256, fingerprint, mime_type, size, ingested_at, provenance FROM files`,
	)
	if err != nil {
		return nil, fmt.Errorf("list files: %w", err)
	}
	defer rows.Close()

	var files []models.TrackedFile
	for rows.Next() {
		f, err := scanFileRow(rows)
		if err != nil {
			return nil, err
		}
		files = append(files, *f)
	}
	return files, rows.Err()
}

func (p *ProjectDb) UpdateFileFingerprint(fileID int64, fp string) error {
	_, err := p.db.Exec(`UPDATE files SET fingerprint = ? WHERE id = ?`, fp, fileID)
	return err
}

func (p *ProjectDb) UpdateFileSHA256(fileID int64, sha256 string) error {
	_, err := p.db.Exec(`UPDATE files SET sha256 = ? WHERE id = ?`, sha256, fileID)
	return err
}

// --- Tags ---

func (p *ProjectDb) InsertTag(fileID int64, tag, fileHash, fingerprint string) error {
	_, err := p.db.Exec(
		`INSERT OR IGNORE INTO file_tags (file_id, tag, file_hash, fingerprint)
		 VALUES (?, ?, ?, ?)`,
		fileID, tag, fileHash, fingerprint,
	)
	return err
}

func (p *ProjectDb) RemoveTag(fileID int64, tag string) error {
	_, err := p.db.Exec(
		`DELETE FROM file_tags WHERE file_id = ? AND tag = ?`, fileID, tag,
	)
	return err
}

func (p *ProjectDb) GetTags(fileID int64) ([]string, error) {
	rows, err := p.db.Query(
		`SELECT tag FROM file_tags WHERE file_id = ? ORDER BY tag`, fileID,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tags []string
	for rows.Next() {
		var tag string
		if err := rows.Scan(&tag); err != nil {
			return nil, err
		}
		tags = append(tags, tag)
	}
	return tags, rows.Err()
}

// --- File Links ---

func (p *ProjectDb) InsertFileLink(sourceID, targetID int64, linkType string, metadata *string) error {
	_, err := p.db.Exec(
		`INSERT INTO file_links (source_file_id, target_file_id, link_type, metadata)
		 VALUES (?, ?, ?, ?)`,
		sourceID, targetID, linkType, metadata,
	)
	return err
}

// --- Audit ---

func (p *ProjectDb) InsertAudit(operation string, fileID *int64, user, detail *string) error {
	now := time.Now().UTC().Format(time.RFC3339)
	_, err := p.db.Exec(
		`INSERT INTO audit_log (timestamp, operation, file_id, user, detail)
		 VALUES (?, ?, ?, ?, ?)`,
		now, operation, fileID, user, detail,
	)
	return err
}

// --- Counts ---

func (p *ProjectDb) FileCount() (int64, error) {
	var n int64
	err := p.db.QueryRow(`SELECT COUNT(*) FROM files`).Scan(&n)
	return n, err
}

func (p *ProjectDb) CategoryCount() (int64, error) {
	var n int64
	err := p.db.QueryRow(`SELECT COUNT(*) FROM scopes WHERE scope_type = 'category'`).Scan(&n)
	return n, err
}

// --- Row scanners ---

func scanScope(row *sql.Row) (*models.Scope, error) {
	var s models.Scope
	var scopeType, catType string
	err := row.Scan(&s.ID, &s.Name, &scopeType, &s.Pattern, &catType, &s.Description, &s.CreatedAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	s.ScopeType = models.ScopeType(scopeType)
	if catType != "" {
		ct := models.CategoryType(catType)
		s.CategoryType = &ct
	}
	return &s, nil
}

func scanScopes(rows *sql.Rows) ([]models.Scope, error) {
	var scopes []models.Scope
	for rows.Next() {
		var s models.Scope
		var scopeType, catType string
		err := rows.Scan(&s.ID, &s.Name, &scopeType, &s.Pattern, &catType, &s.Description, &s.CreatedAt)
		if err != nil {
			return nil, err
		}
		s.ScopeType = models.ScopeType(scopeType)
		if catType != "" {
			ct := models.CategoryType(catType)
			s.CategoryType = &ct
		}
		scopes = append(scopes, s)
	}
	return scopes, rows.Err()
}

type fileScanner interface {
	Scan(dest ...any) error
}

func scanFileFromRow(scanner fileScanner) (*models.TrackedFile, error) {
	var f models.TrackedFile
	err := scanner.Scan(&f.ID, &f.SHA256, &f.Fingerprint, &f.MimeType, &f.Size, &f.IngestedAt, &f.Provenance)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	return &f, nil
}

func scanFile(row *sql.Row) (*models.TrackedFile, error) {
	return scanFileFromRow(row)
}

func scanFileRow(rows *sql.Rows) (*models.TrackedFile, error) {
	return scanFileFromRow(rows)
}
