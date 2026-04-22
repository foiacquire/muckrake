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

// --- Tool dispatch ---

func TestToolExecutesScript(t *testing.T) {
	dir := initTestProject(t)
	script := "#!/bin/sh\necho hello-from-tool\n"
	createTestFile(t, dir, "tools/greet.sh", script)
	os.Chmod(filepath.Join(dir, "tools/greet.sh"), 0o755)

	stdout, stderr, err := mkrk(t, dir, "tool", "greet")
	if err != nil {
		t.Fatalf("tool greet failed: %v\nstderr: %s", err, stderr)
	}
	if !strings.Contains(stdout, "hello-from-tool") {
		t.Fatalf("expected tool output, got: %s", stdout)
	}
}

func TestToolPassesEnv(t *testing.T) {
	dir := initTestProject(t)
	script := "#!/bin/sh\necho \"PROJECT=$MKRK_PROJECT_ROOT\"\n"
	createTestFile(t, dir, "tools/envcheck.sh", script)
	os.Chmod(filepath.Join(dir, "tools/envcheck.sh"), 0o755)

	stdout, stderr, err := mkrk(t, dir, "tool", "envcheck")
	if err != nil {
		t.Fatalf("tool envcheck failed: %v\nstderr: %s", err, stderr)
	}
	if !strings.Contains(stdout, dir) {
		t.Fatalf("expected MKRK_PROJECT_ROOT in output, got: %s", stdout)
	}
}

func TestToolUnknownName(t *testing.T) {
	dir := initTestProject(t)
	_, stderr, err := mkrk(t, dir, "tool", "nonexistent")
	if err == nil {
		t.Fatal("expected tool nonexistent to fail")
	}
	if !strings.Contains(stderr, "no tool") {
		t.Fatalf("expected 'no tool' error, got: %s", stderr)
	}
}

func TestSubjectTargetsSpecificProject(t *testing.T) {
	wsDir := filepath.Join(t.TempDir(), "workspace")
	os.MkdirAll(wsDir, 0o755)
	mustMkrk(t, wsDir, "init", "--workspace", "projects/")
	mustMkrk(t, wsDir, "init", "alpha")
	mustMkrk(t, wsDir, "init", "beta")

	createTestFile(t, wsDir, "projects/alpha/evidence/a.txt", "alpha content")
	createTestFile(t, wsDir, "projects/beta/evidence/b.txt", "beta content")
	mustMkrk(t, wsDir, "sync")

	// :alpha list should only show alpha files, not beta.
	stdout, _ := mustMkrk(t, wsDir, ":alpha", "list")
	if !strings.Contains(stdout, ":alpha") {
		t.Fatalf("expected :alpha in output, got: %s", stdout)
	}
	if strings.Contains(stdout, ":beta") {
		t.Fatalf("did not expect :beta in output, got: %s", stdout)
	}
}

func TestSubjectTagAndStatus(t *testing.T) {
	wsDir := filepath.Join(t.TempDir(), "workspace")
	os.MkdirAll(wsDir, 0o755)
	mustMkrk(t, wsDir, "init", "--workspace", "projects/")
	mustMkrk(t, wsDir, "init", "alpha")

	createTestFile(t, wsDir, "projects/alpha/evidence/doc.txt", "content")
	mustMkrk(t, wsDir, "sync")

	// Tag via subject: no positional ref arg needed.
	_, stderr := mustMkrk(t, wsDir, ":alpha.evidence", "tag", "important")
	if !strings.Contains(stderr, "important") {
		t.Fatalf("expected tag confirmation, got: %s", stderr)
	}

	// Status via subject should show the tag.
	stdout, _ := mustMkrk(t, wsDir, ":alpha.evidence", "status")
	if !strings.Contains(stdout, "important") {
		t.Fatalf("expected tag in status, got: %s", stdout)
	}
}

func TestSubjectReadFile(t *testing.T) {
	wsDir := filepath.Join(t.TempDir(), "workspace")
	os.MkdirAll(wsDir, 0o755)
	mustMkrk(t, wsDir, "init", "--workspace", "projects/")
	mustMkrk(t, wsDir, "init", "alpha")
	createTestFile(t, wsDir, "projects/alpha/evidence/note.txt", "secret content\n")
	mustMkrk(t, wsDir, "sync")

	stdout, _ := mustMkrk(t, wsDir, ":alpha.evidence", "read")
	if !strings.Contains(stdout, "secret content") {
		t.Fatalf("expected file content, got: %s", stdout)
	}
}

func TestSubjectUnknownProject(t *testing.T) {
	wsDir := filepath.Join(t.TempDir(), "workspace")
	os.MkdirAll(wsDir, 0o755)
	mustMkrk(t, wsDir, "init", "--workspace", "projects/")

	_, stderr, err := mkrk(t, wsDir, ":nonexistent", "list")
	if err == nil {
		t.Fatal("expected :nonexistent subject to fail")
	}
	if !strings.Contains(stderr, "not found") {
		t.Fatalf("expected 'not found' error, got: %s", stderr)
	}
}

func TestToolSubjectAppendsFiles(t *testing.T) {
	wsDir := filepath.Join(t.TempDir(), "workspace")
	os.MkdirAll(wsDir, 0o755)
	mustMkrk(t, wsDir, "init", "--workspace", "projects/")
	mustMkrk(t, wsDir, "init", "alpha")

	projDir := filepath.Join(wsDir, "projects/alpha")
	createTestFile(t, wsDir, "projects/alpha/evidence/a.txt", "alpha")
	createTestFile(t, wsDir, "projects/alpha/evidence/b.txt", "beta")
	mustMkrk(t, projDir, "sync")

	script := "#!/bin/sh\nfor f in \"$@\"; do echo \"$f\"; done\n"
	createTestFile(t, wsDir, "projects/alpha/tools/ls.sh", script)
	os.Chmod(filepath.Join(projDir, "tools/ls.sh"), 0o755)

	// Subject goes before the verb. evidence files append to tool argv.
	stdout, stderr, err := mkrk(t, wsDir, ":alpha.evidence", "tool", "ls")
	if err != nil {
		t.Fatalf("tool failed: %v\nstderr: %s", err, stderr)
	}
	if !strings.Contains(stdout, "evidence/a.txt") || !strings.Contains(stdout, "evidence/b.txt") {
		t.Fatalf("expected both evidence files in output, got: %s", stdout)
	}
}

func TestToolPassesNonRefArgsVerbatim(t *testing.T) {
	dir := initTestProject(t)
	script := "#!/bin/sh\necho \"$1\"\n"
	createTestFile(t, dir, "tools/echo.sh", script)
	os.Chmod(filepath.Join(dir, "tools/echo.sh"), 0o755)

	stdout, _ := mustMkrk(t, dir, "tool", "echo", "--flag")
	if !strings.Contains(stdout, "--flag") {
		t.Fatalf("expected --flag passed through, got: %s", stdout)
	}
}

func TestToolSetsPrivacyProxy(t *testing.T) {
	dir := initTestProject(t)
	script := "#!/bin/sh\necho \"http_proxy=$http_proxy\"\necho \"all_proxy=$all_proxy\"\n"
	createTestFile(t, dir, "tools/proxycheck.sh", script)
	os.Chmod(filepath.Join(dir, "tools/proxycheck.sh"), 0o755)

	stdout, stderr, err := mkrk(t, dir, "tool", "proxycheck")
	if err != nil {
		t.Fatalf("tool failed: %v\nstderr: %s", err, stderr)
	}
	if !strings.Contains(stdout, "http_proxy=socks5h://127.0.0.1:9050") {
		t.Fatalf("expected default socks proxy set, got: %s", stdout)
	}
	if !strings.Contains(stderr, "privacy: routing through") {
		t.Fatalf("expected privacy notice on stderr, got: %s", stderr)
	}
}

func TestToolPrivacyOverride(t *testing.T) {
	dir := initTestProject(t)
	script := "#!/bin/sh\necho \"all_proxy=$all_proxy\"\n"
	createTestFile(t, dir, "tools/px.sh", script)
	os.Chmod(filepath.Join(dir, "tools/px.sh"), 0o755)

	cmd := exec.Command(binary, "tool", "px")
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "NO_COLOR=1", "MKRK_SOCKS=socks5h://127.0.0.1:1080")
	var stdout, stderr strings.Builder
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf("tool failed: %v\nstderr: %s", err, stderr.String())
	}
	if !strings.Contains(stdout.String(), "all_proxy=socks5h://127.0.0.1:1080") {
		t.Fatalf("expected override socks proxy, got: %s", stdout.String())
	}
}

func TestToolIngestsOutputs(t *testing.T) {
	dir := initTestProject(t)
	createTestFile(t, dir, "evidence/input.txt", "original content")
	mustMkrk(t, dir, "sync")

	// Tool writes a derived file to MKRK_OUTPUT_DIR
	script := "#!/bin/sh\nprintf 'derived' > \"$MKRK_OUTPUT_DIR/result.txt\"\n"
	createTestFile(t, dir, "tools/derive.sh", script)
	os.Chmod(filepath.Join(dir, "tools/derive.sh"), 0o755)

	stdout, stderr, err := mkrk(t, dir, "tool", "derive", ":evidence")
	if err != nil {
		t.Fatalf("tool derive failed: %v\nstdout: %s\nstderr: %s", err, stdout, stderr)
	}
	if !strings.Contains(stderr, "ingested 1 output file") {
		t.Fatalf("expected ingestion notice, got: %s", stderr)
	}

	// Output file should exist in project tree under outputs/derive-*/result.txt
	matches, _ := filepath.Glob(filepath.Join(dir, "outputs", "derive-*", "result.txt"))
	if len(matches) != 1 {
		t.Fatalf("expected one output file under outputs/derive-*, got: %v", matches)
	}
}

func TestToolExplicitProjectPrefix(t *testing.T) {
	dir := initTestProject(t)
	script := "#!/bin/sh\necho explicit\n"
	createTestFile(t, dir, "tools/x.sh", script)
	os.Chmod(filepath.Join(dir, "tools/x.sh"), 0o755)

	// Need to know the project's workspace name — there's no workspace here,
	// so fall back to :mkrk.x which should fail (no built-in x)
	_, stderr, err := mkrk(t, dir, "tool", ":mkrk.x")
	if err == nil {
		t.Fatal("expected :mkrk.x to fail — no built-in tools registered")
	}
	if !strings.Contains(stderr, "no tool") {
		t.Fatalf("expected 'no tool' error, got: %s", stderr)
	}
}
