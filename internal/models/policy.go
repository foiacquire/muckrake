package models

import "fmt"

type ProtectionLevel string

const (
	ProtectionEditable  ProtectionLevel = "editable"
	ProtectionProtected ProtectionLevel = "protected"
	ProtectionImmutable ProtectionLevel = "immutable"
)

func ParseProtectionLevel(s string) (ProtectionLevel, error) {
	switch s {
	case "editable":
		return ProtectionEditable, nil
	case "protected":
		return ProtectionProtected, nil
	case "immutable":
		return ProtectionImmutable, nil
	default:
		return "", fmt.Errorf("unknown protection level: %s", s)
	}
}

func (p ProtectionLevel) Ordinal() int {
	switch p {
	case ProtectionEditable:
		return 0
	case ProtectionProtected:
		return 1
	case ProtectionImmutable:
		return 2
	default:
		return 0
	}
}

// Strictest returns the most restrictive protection level from a slice.
// Defaults to Editable if empty.
func Strictest(levels []ProtectionLevel) ProtectionLevel {
	best := ProtectionEditable
	for _, l := range levels {
		if l.Ordinal() > best.Ordinal() {
			best = l
		}
	}
	return best
}
