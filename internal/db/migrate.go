package db

import "database/sql"

// MigrateProject migrates a project database from the Rust schema to Go.
// Copies data from legacy tables (categories, category_policy, tool_config,
// tag_tool_config) into the new scopes/scope_policy/scope_tool_config tables.
// Safe to run multiple times — skips if legacy tables don't exist or if
// data has already been migrated.
func MigrateProject(d *sql.DB) error {
	if !tableExists(d, "categories") {
		return nil
	}
	if rowCount(d, "scopes") > 0 {
		return nil // already migrated
	}

	// Migrate categories → scopes
	_, err := d.Exec(`
		INSERT OR IGNORE INTO scopes (name, scope_type, pattern, category_type, description)
		SELECT COALESCE(name, ''), 'category', pattern, COALESCE(category_type, 'files'), description
		FROM categories
	`)
	if err != nil {
		return err
	}

	// Migrate category_policy → scope_policy
	if tableExists(d, "category_policy") {
		_, err = d.Exec(`
			INSERT OR IGNORE INTO scope_policy (scope_id, protection_level)
			SELECT s.id, cp.protection_level
			FROM category_policy cp
			JOIN categories c ON c.id = cp.category_id
			JOIN scopes s ON s.name = COALESCE(c.name, '') AND s.scope_type = 'category'
		`)
		if err != nil {
			return err
		}
	}

	// Migrate tool_config → scope_tool_config
	if tableExists(d, "tool_config") {
		_, err = d.Exec(`
			INSERT OR IGNORE INTO scope_tool_config (scope_id, action, file_type, command, env, quiet)
			SELECT s.id, tc.action, tc.file_type, tc.command, tc.env, tc.quiet
			FROM tool_config tc
			LEFT JOIN scopes s ON s.name = tc.scope AND s.scope_type = 'category'
		`)
		if err != nil {
			return err
		}
	}

	// Migrate tag_tool_config → create tag scopes + scope_tool_config
	if tableExists(d, "tag_tool_config") {
		// Create tag scopes
		_, err = d.Exec(`
			INSERT OR IGNORE INTO scopes (name, scope_type)
			SELECT DISTINCT tag, 'tag' FROM tag_tool_config
		`)
		if err != nil {
			return err
		}
		// Copy configs
		_, err = d.Exec(`
			INSERT OR IGNORE INTO scope_tool_config (scope_id, action, file_type, command, env, quiet)
			SELECT s.id, ttc.action, ttc.file_type, ttc.command, ttc.env, ttc.quiet
			FROM tag_tool_config ttc
			JOIN scopes s ON s.name = ttc.tag AND s.scope_type = 'tag'
		`)
		if err != nil {
			return err
		}
	}

	return nil
}

// MigrateWorkspace migrates a workspace database from Rust schema to Go.
// Copies projects and default_categories into the scopes table.
func MigrateWorkspace(d *sql.DB) error {
	if !tableExists(d, "projects") {
		return nil
	}
	if rowCount(d, "scopes WHERE scope_type = 'project'") > 0 {
		return nil // already migrated
	}

	// Migrate projects → scopes
	_, err := d.Exec(`
		INSERT OR IGNORE INTO scopes (name, scope_type, pattern, description, created_at)
		SELECT name, 'project', path, description, created_at
		FROM projects
	`)
	if err != nil {
		return err
	}

	// Migrate default_categories → scopes
	if tableExists(d, "default_categories") {
		_, err = d.Exec(`
			INSERT OR IGNORE INTO scopes (name, scope_type, pattern, category_type, description)
			SELECT COALESCE(name, ''), 'category', pattern, COALESCE(category_type, 'files'), description
			FROM default_categories
		`)
		if err != nil {
			return err
		}
	}

	// Migrate default_category_policy → scope_policy
	if tableExists(d, "default_category_policy") {
		_, err = d.Exec(`
			INSERT OR IGNORE INTO scope_policy (scope_id, protection_level)
			SELECT s.id, dcp.protection_level
			FROM default_category_policy dcp
			JOIN default_categories dc ON dc.id = dcp.default_category_id
			JOIN scopes s ON s.name = COALESCE(dc.name, '') AND s.scope_type = 'category'
		`)
		if err != nil {
			return err
		}
	}

	return nil
}

func tableExists(d *sql.DB, name string) bool {
	var n int
	err := d.QueryRow(
		"SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?", name,
	).Scan(&n)
	return err == nil && n > 0
}

func rowCount(d *sql.DB, tableExpr string) int64 {
	var n int64
	d.QueryRow("SELECT COUNT(*) FROM " + tableExpr).Scan(&n)
	return n
}
