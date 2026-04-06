package db

import (
	"os"
	"path/filepath"
	"testing"

	"go.foia.dev/muckrake/internal/models"
)

func testDb(t *testing.T) *ProjectDb {
	t.Helper()
	dir := t.TempDir()
	db, err := CreateProject(filepath.Join(dir, ".mkrk"))
	if err != nil {
		t.Fatalf("create db: %v", err)
	}
	t.Cleanup(func() { db.Close() })
	return db
}

func strPtr(s string) *string { return &s }

func catType(ct models.CategoryType) *models.CategoryType { return &ct }

func makeCategory(name, pattern string) *models.Scope {
	return &models.Scope{
		Name:         name,
		ScopeType:    models.ScopeTypeCategory,
		Pattern:      strPtr(pattern),
		CategoryType: catType(models.CategoryTypeFiles),
	}
}

func TestScopeCRUD(t *testing.T) {
	db := testDb(t)

	id, err := db.InsertScope(makeCategory("evidence", "evidence/**"))
	if err != nil {
		t.Fatalf("insert: %v", err)
	}
	if id <= 0 {
		t.Fatal("expected positive id")
	}

	cats, err := db.ListCategories()
	if err != nil {
		t.Fatalf("list: %v", err)
	}
	if len(cats) != 1 {
		t.Fatalf("expected 1 category, got %d", len(cats))
	}
	if cats[0].Name != "evidence" {
		t.Fatalf("expected evidence, got %s", cats[0].Name)
	}

	found, err := db.GetCategoryByName("evidence")
	if err != nil {
		t.Fatalf("get by name: %v", err)
	}
	if found == nil {
		t.Fatal("expected to find evidence")
	}

	notFound, err := db.GetCategoryByName("nonexistent")
	if err != nil {
		t.Fatalf("get nonexistent: %v", err)
	}
	if notFound != nil {
		t.Fatal("expected nil for nonexistent")
	}

	if err := db.RemoveScope(id); err != nil {
		t.Fatalf("remove: %v", err)
	}
	cats, _ = db.ListCategories()
	if len(cats) != 0 {
		t.Fatal("expected empty after remove")
	}
}

func TestMatchCategory(t *testing.T) {
	db := testDb(t)
	db.InsertScope(makeCategory("evidence", "evidence/**"))
	db.InsertScope(makeCategory("notes", "notes/**"))

	cat, err := db.MatchCategory("evidence/doc.pdf")
	if err != nil {
		t.Fatalf("match: %v", err)
	}
	if cat == nil || cat.Name != "evidence" {
		t.Fatalf("expected evidence, got %v", cat)
	}

	cat, err = db.MatchCategory("notes/todo.md")
	if err != nil {
		t.Fatalf("match: %v", err)
	}
	if cat == nil || cat.Name != "notes" {
		t.Fatalf("expected notes, got %v", cat)
	}

	cat, err = db.MatchCategory("random/file.txt")
	if err != nil {
		t.Fatalf("match: %v", err)
	}
	if cat != nil {
		t.Fatalf("expected nil for unmatched path, got %v", cat)
	}
}

func TestScopePolicy(t *testing.T) {
	db := testDb(t)
	id, _ := db.InsertScope(makeCategory("evidence", "evidence/**"))

	if err := db.InsertScopePolicy(id, models.ProtectionImmutable); err != nil {
		t.Fatalf("insert policy: %v", err)
	}

	level, err := db.GetPolicyForScope(id)
	if err != nil {
		t.Fatalf("get policy: %v", err)
	}
	if level == nil || *level != models.ProtectionImmutable {
		t.Fatalf("expected immutable, got %v", level)
	}
}

func TestResolveProtection(t *testing.T) {
	db := testDb(t)
	id, _ := db.InsertScope(makeCategory("evidence", "evidence/**"))
	db.InsertScopePolicy(id, models.ProtectionImmutable)

	level, err := db.ResolveProtection("evidence/doc.pdf")
	if err != nil {
		t.Fatalf("resolve: %v", err)
	}
	if level != models.ProtectionImmutable {
		t.Fatalf("expected immutable, got %v", level)
	}

	level, err = db.ResolveProtection("random/file.txt")
	if err != nil {
		t.Fatalf("resolve: %v", err)
	}
	if level != models.ProtectionEditable {
		t.Fatalf("expected editable for unmatched, got %v", level)
	}
}

func TestFileCRUD(t *testing.T) {
	db := testDb(t)

	f := &models.TrackedFile{
		SHA256:      "abc123",
		Fingerprint: "[]",
		IngestedAt:  "2025-01-01T00:00:00Z",
	}
	id, err := db.InsertFile(f)
	if err != nil {
		t.Fatalf("insert: %v", err)
	}
	if id <= 0 {
		t.Fatal("expected positive id")
	}

	found, err := db.GetFileByHash("abc123")
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	if found == nil || found.SHA256 != "abc123" {
		t.Fatalf("expected abc123, got %v", found)
	}

	count, _ := db.FileCount()
	if count != 1 {
		t.Fatalf("expected 1 file, got %d", count)
	}
}

func TestTags(t *testing.T) {
	db := testDb(t)

	f := &models.TrackedFile{
		SHA256:      "abc123",
		Fingerprint: "[]",
		IngestedAt:  "2025-01-01T00:00:00Z",
	}
	fileID, _ := db.InsertFile(f)

	if err := db.InsertTag(fileID, "classified", "abc123", "[]"); err != nil {
		t.Fatalf("insert tag: %v", err)
	}

	tags, err := db.GetTags(fileID)
	if err != nil {
		t.Fatalf("get tags: %v", err)
	}
	if len(tags) != 1 || tags[0] != "classified" {
		t.Fatalf("expected [classified], got %v", tags)
	}

	if err := db.RemoveTag(fileID, "classified"); err != nil {
		t.Fatalf("remove tag: %v", err)
	}
	tags, _ = db.GetTags(fileID)
	if len(tags) != 0 {
		t.Fatal("expected empty after remove")
	}
}

func TestOpenProject(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".mkrk")

	db, err := CreateProject(path)
	if err != nil {
		t.Fatalf("create: %v", err)
	}
	db.InsertScope(makeCategory("evidence", "evidence/**"))
	db.Close()

	db2, err := OpenProject(path)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer db2.Close()

	cats, _ := db2.ListCategories()
	if len(cats) != 1 {
		t.Fatalf("expected 1 category after reopen, got %d", len(cats))
	}
}

func TestOpenProjectNotFound(t *testing.T) {
	_, err := OpenProject("/nonexistent/.mkrk")
	if err == nil {
		t.Fatal("expected error for missing db")
	}
}

func TestCreateProjectDir(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sub", "deep", ".mkrk")
	os.MkdirAll(filepath.Dir(path), 0o755)

	db, err := CreateProject(path)
	if err != nil {
		t.Fatalf("create: %v", err)
	}
	db.Close()
}
