package tests

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

var binary string

func TestMain(m *testing.M) {
	// Build binary once for all tests
	tmp, err := os.MkdirTemp("", "mkrk-test-bin-")
	if err != nil {
		panic(err)
	}
	binary = filepath.Join(tmp, "mkrk")

	cmd := exec.Command("go", "build", "-o", binary, ".")
	cmd.Dir = ".."
	if out, err := cmd.CombinedOutput(); err != nil {
		panic("build failed: " + string(out))
	}

	code := m.Run()
	os.RemoveAll(tmp)
	os.Exit(code)
}

func mkrk(t *testing.T, dir string, args ...string) (string, string, error) {
	t.Helper()
	cmd := exec.Command(binary, args...)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "NO_COLOR=1")
	var stdout, stderr strings.Builder
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	return stdout.String(), stderr.String(), err
}

func mustMkrk(t *testing.T, dir string, args ...string) (string, string) {
	t.Helper()
	stdout, stderr, err := mkrk(t, dir, args...)
	if err != nil {
		t.Fatalf("mkrk %s failed: %v\nstderr: %s", strings.Join(args, " "), err, stderr)
	}
	return stdout, stderr
}

func projectDir(t *testing.T) string {
	t.Helper()
	dir := filepath.Join(t.TempDir(), "testproject")
	os.MkdirAll(dir, 0o755)
	return dir
}

func initTestProject(t *testing.T) string {
	t.Helper()
	dir := projectDir(t)
	mustMkrk(t, dir, "init")
	return dir
}

func createTestFile(t *testing.T, dir, relPath, content string) {
	t.Helper()
	abs := filepath.Join(dir, relPath)
	os.MkdirAll(filepath.Dir(abs), 0o755)
	os.WriteFile(abs, []byte(content), 0o644)
}

// --- Init ---

func TestInitCreatesProject(t *testing.T) {
	dir := projectDir(t)
	mustMkrk(t, dir, "init")

	if _, err := os.Stat(filepath.Join(dir, ".mkrk")); err != nil {
		t.Fatal("expected .mkrk")
	}
	for _, d := range []string{"evidence", "sources", "notes", "tools"} {
		if _, err := os.Stat(filepath.Join(dir, d)); err != nil {
			t.Fatalf("expected %s directory", d)
		}
	}
}

func TestInitNoCategories(t *testing.T) {
	dir := projectDir(t)
	mustMkrk(t, dir, "init", "--no-categories")

	if _, err := os.Stat(filepath.Join(dir, ".mkrk")); err != nil {
		t.Fatal("expected .mkrk")
	}
	if _, err := os.Stat(filepath.Join(dir, "evidence")); err == nil {
		t.Fatal("should not create evidence dir with --no-categories")
	}
}

func TestInitRefusesDouble(t *testing.T) {
	dir := projectDir(t)
	mustMkrk(t, dir, "init", "--no-categories")

	_, stderr, err := mkrk(t, dir, "init", "--no-categories")
	if err == nil {
		t.Fatal("expected error on double init")
	}
	if !strings.Contains(stderr, "already exists") {
		t.Fatalf("expected 'already exists' in stderr, got: %s", stderr)
	}
}

// --- Status ---

func TestStatusAfterInit(t *testing.T) {
	dir := initTestProject(t)
	mustMkrk(t, dir, "status")
}

// --- Sync + List ---

func TestSyncTracksFiles(t *testing.T) {
	dir := initTestProject(t)
	createTestFile(t, dir, "evidence/report.txt", "classified content")

	_, stderr := mustMkrk(t, dir, "sync")
	if !strings.Contains(stderr, "evidence/report.txt") {
		t.Fatalf("expected report.txt in sync output, got: %s", stderr)
	}

	stdout, _ := mustMkrk(t, dir, "list")
	if !strings.Contains(stdout, "report.txt") {
		t.Fatalf("expected report.txt in list output, got: %s", stdout)
	}
}

// --- Verify (via sync) ---

func TestSyncPassesUnmodified(t *testing.T) {
	dir := initTestProject(t)
	createTestFile(t, dir, "evidence/clean.txt", "untouched")
	mustMkrk(t, dir, "sync")

	// Second sync should just verify, no errors
	mustMkrk(t, dir, "sync")
}

func TestSyncFailsMissing(t *testing.T) {
	dir := initTestProject(t)
	createTestFile(t, dir, "evidence/vanished.txt", "here today")
	mustMkrk(t, dir, "sync")

	os.Remove(filepath.Join(dir, "evidence/vanished.txt"))

	_, stderr, err := mkrk(t, dir, "sync")
	if err == nil {
		t.Fatal("expected error for missing file")
	}
	if !strings.Contains(stderr, "missing") {
		t.Fatalf("expected 'missing' in stderr, got: %s", stderr)
	}
}

// --- Tags ---

func TestTagAndStatus(t *testing.T) {
	dir := initTestProject(t)
	createTestFile(t, dir, "evidence/doc.txt", "tagged content")
	mustMkrk(t, dir, "sync")

	_, stderr := mustMkrk(t, dir, "tag", "evidence/doc.txt", "important")
	if !strings.Contains(stderr, "!important") {
		t.Fatalf("expected tag confirmation, got: %s", stderr)
	}

	stdout, _ := mustMkrk(t, dir, "status", "evidence/doc.txt")
	if !strings.Contains(stdout, "important") {
		t.Fatalf("expected tag in status, got: %s", stdout)
	}
}

func TestTagRemove(t *testing.T) {
	dir := initTestProject(t)
	createTestFile(t, dir, "evidence/doc.txt", "tagged content")
	mustMkrk(t, dir, "sync")

	mustMkrk(t, dir, "tag", "evidence/doc.txt", "removeme")
	mustMkrk(t, dir, "tag", "--remove", "evidence/doc.txt", "removeme")

	stdout, _ := mustMkrk(t, dir, "status", "evidence/doc.txt")
	if strings.Contains(stdout, "removeme") {
		t.Fatalf("tag should be removed, got: %s", stdout)
	}
}

// --- Pipeline ---

func TestPipelineCreateAndRemove(t *testing.T) {
	dir := initTestProject(t)

	_, stderr := mustMkrk(t, dir, "pipeline", "editorial", "--states", "draft,review,published")
	if !strings.Contains(stderr, "Created pipeline") {
		t.Fatalf("expected creation message, got: %s", stderr)
	}

	stdout, _ := mustMkrk(t, dir, "status")
	if !strings.Contains(stdout, "editorial") {
		t.Fatalf("expected pipeline in status, got: %s", stdout)
	}

	mustMkrk(t, dir, "pipeline", "--remove", "editorial")

	stdout, _ = mustMkrk(t, dir, "status")
	if strings.Contains(stdout, "editorial") {
		t.Fatalf("pipeline should be removed, got: %s", stdout)
	}
}

// --- Sign ---

func TestSignAndRevoke(t *testing.T) {
	dir := initTestProject(t)
	createTestFile(t, dir, "evidence/doc.txt", "evidence content")
	mustMkrk(t, dir, "sync")
	mustMkrk(t, dir, "pipeline", "editorial", "--states", "draft,review,published")

	_, stderr := mustMkrk(t, dir, "sign", "evidence/doc.txt", "review", "--pipeline", "editorial")
	if !strings.Contains(stderr, "Signed") {
		t.Fatalf("expected sign confirmation, got: %s", stderr)
	}

	_, stderr = mustMkrk(t, dir, "sign", "--remove", "evidence/doc.txt", "review", "--pipeline", "editorial")
	if !strings.Contains(stderr, "Revoked") {
		t.Fatalf("expected revoke confirmation, got: %s", stderr)
	}
}

// --- Read ---

func TestRead(t *testing.T) {
	dir := initTestProject(t)
	createTestFile(t, dir, "evidence/report.txt", "secret content\n")
	mustMkrk(t, dir, "sync")

	stdout, _ := mustMkrk(t, dir, "read", "evidence/report.txt")
	if !strings.Contains(stdout, "secret content") {
		t.Fatalf("expected file content, got: %s", stdout)
	}
}

// --- Workspace dispatch ---

func TestWorkspaceSyncDispatch(t *testing.T) {
	wsDir := filepath.Join(t.TempDir(), "workspace")
	os.MkdirAll(wsDir, 0o755)

	mustMkrk(t, wsDir, "init", "--workspace", "projects/")
	mustMkrk(t, wsDir, "init", "alpha")
	mustMkrk(t, wsDir, "init", "beta")

	createTestFile(t, wsDir, "projects/alpha/evidence/a.txt", "alpha evidence")
	createTestFile(t, wsDir, "projects/beta/evidence/b.txt", "beta evidence")

	// Sync from workspace root should dispatch to both projects
	_, stderr := mustMkrk(t, wsDir, "sync")
	if !strings.Contains(stderr, "a.txt") {
		t.Fatalf("expected a.txt in output, got: %s", stderr)
	}
	if !strings.Contains(stderr, "b.txt") {
		t.Fatalf("expected b.txt in output, got: %s", stderr)
	}
}

func TestWorkspaceListDispatch(t *testing.T) {
	wsDir := filepath.Join(t.TempDir(), "workspace")
	os.MkdirAll(wsDir, 0o755)

	mustMkrk(t, wsDir, "init", "--workspace", "projects/")
	mustMkrk(t, wsDir, "init", "alpha")

	createTestFile(t, wsDir, "projects/alpha/evidence/report.txt", "test")
	mustMkrk(t, filepath.Join(wsDir, "projects/alpha"), "sync")

	// List from workspace root should show files with project-scoped references
	stdout, _ := mustMkrk(t, wsDir, "list")
	if !strings.Contains(stdout, ":alpha") {
		t.Fatalf("expected :alpha scoped reference, got: %s", stdout)
	}
	if !strings.Contains(stdout, "report.txt") {
		t.Fatalf("expected report.txt in output, got: %s", stdout)
	}
}
