package integrity

import (
	"os"
	"path/filepath"
	"testing"
)

func writeTestFile(t *testing.T, content string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "test.txt")
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestHashFile(t *testing.T) {
	path := writeTestFile(t, "hello world")
	hash, err := HashFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if len(hash) != 64 {
		t.Fatalf("expected 64 char hex, got %d", len(hash))
	}

	hash2, err := HashFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if hash != hash2 {
		t.Fatal("hash not deterministic")
	}
}

func TestFingerprintFile(t *testing.T) {
	path := writeTestFile(t, "hello world")
	fp, err := FingerprintFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if len(fp.Chunks) == 0 {
		t.Fatal("expected at least one chunk")
	}
	if len(fp.Chunks[0]) != hashOutputSize*2 {
		t.Fatalf("expected %d hex chars per chunk, got %d", hashOutputSize*2, len(fp.Chunks[0]))
	}
}

func TestHashAndFingerprint(t *testing.T) {
	path := writeTestFile(t, "hello world")
	hash, fp, err := HashAndFingerprint(path)
	if err != nil {
		t.Fatal(err)
	}

	hashOnly, _ := HashFile(path)
	if hash != hashOnly {
		t.Fatal("hash mismatch between single and combined")
	}

	fpOnly, _ := FingerprintFile(path)
	if len(fp.Chunks) != len(fpOnly.Chunks) {
		t.Fatal("fingerprint mismatch between single and combined")
	}
	for i := range fp.Chunks {
		if fp.Chunks[i] != fpOnly.Chunks[i] {
			t.Fatalf("chunk %d mismatch", i)
		}
	}
}

func TestFingerprintJSON(t *testing.T) {
	fp := &Fingerprint{Chunks: []string{"abcd1234", "efgh5678"}}
	j := fp.ToJSON()

	fp2, err := FingerprintFromJSON(j)
	if err != nil {
		t.Fatal(err)
	}
	if len(fp2.Chunks) != 2 || fp2.Chunks[0] != "abcd1234" {
		t.Fatalf("round trip failed: %v", fp2.Chunks)
	}
}

func TestMatchingChunks(t *testing.T) {
	a := &Fingerprint{Chunks: []string{"aaa", "bbb", "ccc"}}
	b := &Fingerprint{Chunks: []string{"aaa", "xxx", "ccc"}}

	if n := a.MatchingChunks(b); n != 2 {
		t.Fatalf("expected 2 matching, got %d", n)
	}
}

func TestVerifyFileOk(t *testing.T) {
	path := writeTestFile(t, "test content")
	hash, _ := HashFile(path)

	result, _, err := VerifyFile(path, hash)
	if err != nil {
		t.Fatal(err)
	}
	if result != VerifyOk {
		t.Fatalf("expected ok, got %v", result)
	}
}

func TestVerifyFileModified(t *testing.T) {
	path := writeTestFile(t, "test content")

	result, _, err := VerifyFile(path, "wrong_hash")
	if err != nil {
		t.Fatal(err)
	}
	if result != VerifyModified {
		t.Fatalf("expected modified, got %v", result)
	}
}

func TestVerifyFileMissing(t *testing.T) {
	result, _, err := VerifyFile("/nonexistent/file.txt", "abc")
	if err != nil {
		t.Fatal(err)
	}
	if result != VerifyMissing {
		t.Fatalf("expected missing, got %v", result)
	}
}
