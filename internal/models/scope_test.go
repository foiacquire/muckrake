package models

import "testing"

func TestScopeTypeRoundtrip(t *testing.T) {
	for _, st := range []ScopeType{ScopeTypeCategory, ScopeTypeTag, ScopeTypeProject} {
		parsed, err := ParseScopeType(string(st))
		if err != nil {
			t.Fatalf("ParseScopeType(%q) error: %v", st, err)
		}
		if parsed != st {
			t.Fatalf("expected %v, got %v", st, parsed)
		}
	}
}

func TestScopeTypeInvalid(t *testing.T) {
	_, err := ParseScopeType("bogus")
	if err == nil {
		t.Fatal("expected error for bogus scope type")
	}
}

func TestCategoryTypeRoundtrip(t *testing.T) {
	for _, ct := range []CategoryType{CategoryTypeFiles, CategoryTypeTools, CategoryTypeInbox} {
		parsed, err := ParseCategoryType(string(ct))
		if err != nil {
			t.Fatalf("ParseCategoryType(%q) error: %v", ct, err)
		}
		if parsed != ct {
			t.Fatalf("expected %v, got %v", ct, parsed)
		}
	}
}

func TestGlobMatch(t *testing.T) {
	tests := []struct {
		pattern string
		path    string
		want    bool
	}{
		{"evidence/**", "evidence/doc.pdf", true},
		{"evidence/**", "evidence/sub/doc.pdf", true},
		{"evidence/**", "notes/doc.pdf", false},
		{"evidence/**", "evidence", false},
		{"notes/**", "notes/daily.md", true},
		{"notes/**", "evidence/file.pdf", false},
	}

	for _, tt := range tests {
		got, err := GlobMatch(tt.pattern, tt.path)
		if err != nil {
			t.Fatalf("GlobMatch(%q, %q) error: %v", tt.pattern, tt.path, err)
		}
		if got != tt.want {
			t.Errorf("GlobMatch(%q, %q) = %v, want %v", tt.pattern, tt.path, got, tt.want)
		}
	}
}

func TestNameFromPattern(t *testing.T) {
	tests := []struct {
		pattern string
		want    string
	}{
		{"evidence/**", "evidence"},
		{"tools/*", "tools"},
		{"inbox", "inbox"},
		{"evidence/financial/**", "evidence/financial"},
	}

	for _, tt := range tests {
		got := NameFromPattern(tt.pattern)
		if got != tt.want {
			t.Errorf("NameFromPattern(%q) = %q, want %q", tt.pattern, got, tt.want)
		}
	}
}
