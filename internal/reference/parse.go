package reference

import (
	"fmt"
	"strings"
)

var reservedChars = []byte{':', '.', '/', '!', '{', '}', ','}
var reservedNames = []string{"mkrk"}

// ValidateName checks that a scope/project/category name contains no
// reserved characters and is not a reserved word.
func ValidateName(name string) error {
	if name == "" {
		return fmt.Errorf("name must not be empty")
	}
	for _, ch := range reservedChars {
		if strings.ContainsRune(name, rune(ch)) {
			return fmt.Errorf("name '%s' contains reserved character '%c'", name, ch)
		}
	}
	for _, r := range reservedNames {
		if name == r {
			return fmt.Errorf("name '%s' is reserved for internal use", name)
		}
	}
	return nil
}

// IsReservedName returns true if the name is reserved.
func IsReservedName(name string) bool {
	for _, r := range reservedNames {
		if name == r {
			return true
		}
	}
	return false
}

// ParseReference parses a reference string into a structured Reference.
func ParseReference(input string) (*Reference, error) {
	if strings.HasPrefix(input, ":") {
		return parseStructured(input, input[1:], KindWorkspace)
	}
	if strings.HasPrefix(input, "./") || strings.HasPrefix(input, "../") {
		return &Reference{Kind: KindBarePath, Raw: input}, nil
	}
	return parseStructured(input, input, KindContext)
}

func parseStructured(original, rest string, kind ReferenceKind) (*Reference, error) {
	pos := 0
	workspaceWide := false
	if pos < len(rest) && rest[pos] == '.' {
		pos++
		if kind == KindWorkspace {
			workspaceWide = true
		}
	}

	scope, err := parseScope(rest, &pos)
	if err != nil {
		return nil, err
	}
	tags, err := parseTags(rest, &pos)
	if err != nil {
		return nil, err
	}
	glob := parseGlob(rest, &pos)

	if pos < len(rest) {
		return nil, fmt.Errorf("unexpected character '%c' at position %d in reference '%s'",
			rest[pos], pos+1, original)
	}

	return &Reference{
		Kind:          kind,
		WorkspaceWide: workspaceWide,
		Scope:         scope,
		Tags:          tags,
		Glob:          glob,
	}, nil
}

func parseScope(input string, pos *int) ([]ScopeLevel, error) {
	var levels []ScopeLevel

	for *pos < len(input) {
		ch := input[*pos]
		if ch == '!' || ch == '/' {
			break
		}

		level, err := parseScopeLevel(input, pos)
		if err != nil {
			return nil, err
		}
		levels = append(levels, level)

		if *pos < len(input) && input[*pos] == '.' {
			*pos++
		} else {
			break
		}
	}

	return levels, nil
}

func parseScopeLevel(input string, pos *int) (ScopeLevel, error) {
	if *pos >= len(input) {
		return ScopeLevel{}, fmt.Errorf("expected scope name")
	}

	if input[*pos] == '{' {
		*pos++
		var names []string
		for {
			name := parseName(input, pos)
			if name == "" {
				return ScopeLevel{}, fmt.Errorf("empty name in brace expansion")
			}
			names = append(names, name)

			if *pos >= len(input) {
				return ScopeLevel{}, fmt.Errorf("unclosed brace expansion")
			}
			if input[*pos] == '}' {
				*pos++
				break
			}
			if input[*pos] == ',' {
				*pos++
			} else {
				return ScopeLevel{}, fmt.Errorf("expected ',' or '}' in brace expansion")
			}
		}
		return ScopeLevel{Names: names}, nil
	}

	name := parseName(input, pos)
	if name == "" {
		return ScopeLevel{}, fmt.Errorf("expected scope name")
	}
	return ScopeLevel{Names: []string{name}}, nil
}

func parseName(input string, pos *int) string {
	start := *pos
	for *pos < len(input) {
		ch := input[*pos]
		if ch == '.' || ch == '!' || ch == '/' || ch == '{' || ch == '}' || ch == ',' {
			break
		}
		*pos++
	}
	return input[start:*pos]
}

func parseTags(input string, pos *int) ([]TagFilter, error) {
	var filters []TagFilter

	for *pos < len(input) && input[*pos] == '!' {
		*pos++
		var tags []string

		for {
			tag := parseName(input, pos)
			if tag == "" {
				return nil, fmt.Errorf("empty tag name")
			}
			tags = append(tags, tag)

			if *pos < len(input) && input[*pos] == ',' {
				*pos++
			} else {
				break
			}
		}

		filters = append(filters, TagFilter{Tags: tags})
	}

	return filters, nil
}

func parseGlob(input string, pos *int) *string {
	if *pos < len(input) && input[*pos] == '/' {
		*pos++
		glob := input[*pos:]
		*pos = len(input)
		return &glob
	}
	return nil
}
