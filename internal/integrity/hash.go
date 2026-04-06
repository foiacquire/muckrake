package integrity

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"

	"lukechampine.com/blake3"
)

const chunkSize = 64 * 1024 // 64KB chunks for fingerprinting
const hashOutputSize = 8    // 8 bytes per chunk hash → 16 hex chars

// Fingerprint is a list of BLAKE3 chunk hashes for fast file identification.
type Fingerprint struct {
	Chunks []string
}

// ToJSON serializes the fingerprint to a JSON array string.
func (fp *Fingerprint) ToJSON() string {
	b, _ := json.Marshal(fp.Chunks)
	return string(b)
}

// FromJSON deserializes a fingerprint from a JSON array string.
func FingerprintFromJSON(s string) (*Fingerprint, error) {
	var chunks []string
	if err := json.Unmarshal([]byte(s), &chunks); err != nil {
		return nil, fmt.Errorf("parse fingerprint: %w", err)
	}
	return &Fingerprint{Chunks: chunks}, nil
}

// MatchingChunks counts how many chunks at the same index match.
func (fp *Fingerprint) MatchingChunks(other *Fingerprint) int {
	n := len(fp.Chunks)
	if len(other.Chunks) < n {
		n = len(other.Chunks)
	}
	count := 0
	for i := 0; i < n; i++ {
		if fp.Chunks[i] == other.Chunks[i] {
			count++
		}
	}
	return count
}

// HashFile computes the SHA-256 hash of a file, returned as a hex string.
func HashFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", fmt.Errorf("hash file: %w", err)
	}
	defer f.Close()

	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", fmt.Errorf("hash file: %w", err)
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

// FingerprintFile computes a BLAKE3 chunk fingerprint of a file.
func FingerprintFile(path string) (*Fingerprint, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("fingerprint file: %w", err)
	}
	defer f.Close()

	var chunks []string
	buf := make([]byte, chunkSize)

	for {
		n, err := f.Read(buf)
		if n > 0 {
			h := blake3.Sum256(buf[:n])
			chunks = append(chunks, hex.EncodeToString(h[:hashOutputSize]))
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("fingerprint file: %w", err)
		}
	}

	return &Fingerprint{Chunks: chunks}, nil
}

// HashAndFingerprint computes both SHA-256 and BLAKE3 fingerprint in a single
// read pass.
func HashAndFingerprint(path string) (string, *Fingerprint, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", nil, fmt.Errorf("hash and fingerprint: %w", err)
	}
	defer f.Close()

	sha := sha256.New()
	var chunks []string
	buf := make([]byte, chunkSize)

	for {
		n, err := f.Read(buf)
		if n > 0 {
			sha.Write(buf[:n])
			h := blake3.Sum256(buf[:n])
			chunks = append(chunks, hex.EncodeToString(h[:hashOutputSize]))
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return "", nil, fmt.Errorf("hash and fingerprint: %w", err)
		}
	}

	hash := hex.EncodeToString(sha.Sum(nil))
	return hash, &Fingerprint{Chunks: chunks}, nil
}
