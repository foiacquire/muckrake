package integrity

import (
	"fmt"
	"os/exec"
	"runtime"
	"strings"
)

// SetImmutable sets the filesystem immutable flag on a file.
func SetImmutable(path string) error {
	if runtime.GOOS != "linux" {
		return fmt.Errorf("immutable flag not supported on %s", runtime.GOOS)
	}
	out, err := exec.Command("chattr", "+i", path).CombinedOutput()
	if err != nil {
		return fmt.Errorf("chattr +i: %s", strings.TrimSpace(string(out)))
	}
	return nil
}

// ClearImmutable removes the filesystem immutable flag from a file.
func ClearImmutable(path string) error {
	if runtime.GOOS != "linux" {
		return fmt.Errorf("immutable flag not supported on %s", runtime.GOOS)
	}
	out, err := exec.Command("chattr", "-i", path).CombinedOutput()
	if err != nil {
		return fmt.Errorf("chattr -i: %s", strings.TrimSpace(string(out)))
	}
	return nil
}

// IsImmutable checks whether the filesystem immutable flag is set.
func IsImmutable(path string) (bool, error) {
	if runtime.GOOS != "linux" {
		return false, nil
	}
	out, err := exec.Command("lsattr", "-d", path).CombinedOutput()
	if err != nil {
		return false, fmt.Errorf("lsattr: %s", strings.TrimSpace(string(out)))
	}
	attrs := strings.Fields(string(out))
	if len(attrs) == 0 {
		return false, nil
	}
	return strings.Contains(attrs[0], "i"), nil
}
