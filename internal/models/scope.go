package models

import (
	"fmt"
	"path/filepath"
	"strings"
)

type ScopeType string

const (
	ScopeTypeCategory ScopeType = "category"
	ScopeTypeTag      ScopeType = "tag"
	ScopeTypeProject  ScopeType = "project"
)

// ValidateScopeName checks that a name contains no reserved characters.
func ValidateScopeName(name string) error {
	if name == "" {
		return fmt.Errorf("name must not be empty")
	}
	for _, ch := range []byte{':', '.', '/', '!', '{', '}', ','} {
		if strings.ContainsRune(name, rune(ch)) {
			return fmt.Errorf("name '%s' contains reserved character '%c'", name, ch)
		}
	}
	return nil
}

func ParseScopeType(s string) (ScopeType, error) {
	switch s {
	case "category":
		return ScopeTypeCategory, nil
	case "tag":
		return ScopeTypeTag, nil
	case "project":
		return ScopeTypeProject, nil
	default:
		return "", fmt.Errorf("unknown scope type: %s", s)
	}
}

type CategoryType string

const (
	CategoryTypeFiles CategoryType = "files"
	CategoryTypeTools CategoryType = "tools"
	CategoryTypeInbox CategoryType = "inbox"
)

func ParseCategoryType(s string) (CategoryType, error) {
	switch s {
	case "files":
		return CategoryTypeFiles, nil
	case "tools":
		return CategoryTypeTools, nil
	case "inbox":
		return CategoryTypeInbox, nil
	default:
		return "", fmt.Errorf("unknown category type: %s", s)
	}
}

type Scope struct {
	ID           *int64
	Name         string
	ScopeType    ScopeType
	Pattern      *string
	CategoryType *CategoryType
	Description  *string
	CreatedAt    *string
}

// Matches checks if a file path matches this scope's glob pattern.
// Supports ** for recursive matching (e.g., "evidence/**").
func (s *Scope) Matches(path string) (bool, error) {
	if s.Pattern == nil {
		return false, nil
	}
	return GlobMatch(*s.Pattern, path)
}

// GlobMatch matches a path against a pattern supporting **.
// "**" matches everything.
// "evidence/**" matches "evidence/doc.pdf" and "evidence/sub/doc.pdf".
func GlobMatch(pattern, path string) (bool, error) {
	if pattern == "**" {
		return true, nil
	}
	if strings.Contains(pattern, "**") {
		base := strings.TrimSuffix(pattern, "/**")
		if base == pattern {
			base = strings.TrimSuffix(pattern, "/**/*")
		}
		if path == base {
			return false, nil
		}
		return strings.HasPrefix(path, base+"/"), nil
	}
	return filepath.Match(pattern, path)
}

// NameFromPattern strips trailing /** or /* from a glob pattern.
func NameFromPattern(pattern string) string {
	if s := strings.TrimSuffix(pattern, "/**"); s != pattern {
		return s
	}
	if s := strings.TrimSuffix(pattern, "/*"); s != pattern {
		return s
	}
	return pattern
}
