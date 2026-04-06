package integrity

import (
	"fmt"
	"os"
)

// VerifyResult describes the outcome of verifying a file's integrity.
type VerifyResult int

const (
	VerifyOk       VerifyResult = iota
	VerifyModified              // hash mismatch
	VerifyMissing               // file not on disk
)

// VerifyFile checks if a file's SHA-256 hash matches the expected value.
func VerifyFile(path, expectedHash string) (VerifyResult, string, error) {
	if _, err := os.Stat(path); os.IsNotExist(err) {
		return VerifyMissing, "", nil
	}

	actual, err := HashFile(path)
	if err != nil {
		return 0, "", fmt.Errorf("verify file: %w", err)
	}

	if actual != expectedHash {
		return VerifyModified, actual, nil
	}
	return VerifyOk, actual, nil
}

// ChunkDiff describes a changed chunk in a fingerprint comparison.
type ChunkDiff struct {
	Index  int
	Offset int64
	Size   int64
}

// VerifyFingerprint checks if a file's fingerprint matches the expected value
// and returns which chunks changed.
func VerifyFingerprint(path string, expected *Fingerprint) ([]ChunkDiff, error) {
	if _, err := os.Stat(path); os.IsNotExist(err) {
		return nil, nil
	}

	actual, err := FingerprintFile(path)
	if err != nil {
		return nil, err
	}

	n := len(expected.Chunks)
	if len(actual.Chunks) > n {
		n = len(actual.Chunks)
	}

	var diffs []ChunkDiff
	for i := 0; i < n; i++ {
		var exp, act string
		if i < len(expected.Chunks) {
			exp = expected.Chunks[i]
		}
		if i < len(actual.Chunks) {
			act = actual.Chunks[i]
		}
		if exp != act {
			diffs = append(diffs, ChunkDiff{
				Index:  i,
				Offset: int64(i) * chunkSize,
				Size:   chunkSize,
			})
		}
	}

	return diffs, nil
}
