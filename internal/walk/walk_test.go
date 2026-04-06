package walk

import (
	"os"
	"path/filepath"
	"testing"
)

func createTree(t *testing.T, root string) {
	t.Helper()
	dirs := []string{
		"alpha/sub",
		".hidden",
	}
	files := map[string]string{
		"alpha/one.txt":      "1",
		"alpha/sub/two.txt":  "2",
		".hidden/secret.txt": "s",
		".dotfile":           "d",
		"root.txt":           "r",
	}
	for _, d := range dirs {
		os.MkdirAll(filepath.Join(root, d), 0o755)
	}
	for name, content := range files {
		os.WriteFile(filepath.Join(root, name), []byte(content), 0o644)
	}
}

func TestWalkSkipsDotfiles(t *testing.T) {
	root := t.TempDir()
	createTree(t, root)

	results, err := WalkAndCollect(root, []string{"**"})
	if err != nil {
		t.Fatal(err)
	}

	for _, p := range results {
		if p[0] == '.' || contains(p, ".hidden") {
			t.Fatalf("should skip dot entries, got %s", p)
		}
	}

	if !containsStr(results, "root.txt") {
		t.Fatal("expected root.txt")
	}
	if !containsStr(results, "alpha/one.txt") {
		t.Fatal("expected alpha/one.txt")
	}
}

func TestWalkFiltersByPattern(t *testing.T) {
	root := t.TempDir()
	createTree(t, root)

	results, err := WalkAndCollect(root, []string{"alpha/*", "alpha/**/*"})
	if err != nil {
		t.Fatal(err)
	}

	if !containsStr(results, "alpha/one.txt") {
		t.Fatal("expected alpha/one.txt")
	}
	if !containsStr(results, "alpha/sub/two.txt") {
		t.Fatal("expected alpha/sub/two.txt")
	}
	if containsStr(results, "root.txt") {
		t.Fatal("should not include root.txt")
	}
}

func TestWalkReturnsSorted(t *testing.T) {
	root := t.TempDir()
	createTree(t, root)

	results, err := WalkAndCollect(root, []string{"**"})
	if err != nil {
		t.Fatal(err)
	}

	for i := 1; i < len(results); i++ {
		if results[i] < results[i-1] {
			t.Fatalf("not sorted: %s before %s", results[i-1], results[i])
		}
	}
}

func TestWalkMissingDir(t *testing.T) {
	results, err := WalkAndCollect("/nonexistent/path", []string{"**"})
	if err != nil {
		t.Fatal(err)
	}
	if len(results) != 0 {
		t.Fatal("expected empty for missing dir")
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > len(substr) && containsSubstr(s, substr))
}

func containsSubstr(s, sub string) bool {
	for i := 0; i <= len(s)-len(sub); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}

func containsStr(slice []string, s string) bool {
	for _, v := range slice {
		if v == s {
			return true
		}
	}
	return false
}
