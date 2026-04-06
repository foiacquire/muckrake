package reference

import "testing"

func TestParseWorkspaceProject(t *testing.T) {
	r, err := ParseReference(":bailey")
	if err != nil {
		t.Fatal(err)
	}
	if r.Kind != KindWorkspace {
		t.Fatalf("expected workspace, got %v", r.Kind)
	}
	if len(r.Scope) != 1 || r.Scope[0].Names[0] != "bailey" {
		t.Fatalf("expected scope [bailey], got %v", r.Scope)
	}
}

func TestParseWorkspaceProjectCategory(t *testing.T) {
	r, err := ParseReference(":bailey.evidence")
	if err != nil {
		t.Fatal(err)
	}
	if len(r.Scope) != 2 {
		t.Fatalf("expected 2 scope levels, got %d", len(r.Scope))
	}
	if r.Scope[0].Names[0] != "bailey" || r.Scope[1].Names[0] != "evidence" {
		t.Fatalf("unexpected scope: %v", r.Scope)
	}
}

func TestParseBraceExpansion(t *testing.T) {
	r, err := ParseReference(":{bailey,george}.evidence")
	if err != nil {
		t.Fatal(err)
	}
	if len(r.Scope) != 2 {
		t.Fatalf("expected 2 scope levels, got %d", len(r.Scope))
	}
	if len(r.Scope[0].Names) != 2 {
		t.Fatalf("expected 2 names in first level, got %d", len(r.Scope[0].Names))
	}
}

func TestParseTagsAnd(t *testing.T) {
	r, err := ParseReference(":george!bailey!classified")
	if err != nil {
		t.Fatal(err)
	}
	if len(r.Tags) != 2 {
		t.Fatalf("expected 2 tag filters (AND), got %d", len(r.Tags))
	}
	if r.Tags[0].Tags[0] != "bailey" || r.Tags[1].Tags[0] != "classified" {
		t.Fatalf("unexpected tags: %v", r.Tags)
	}
}

func TestParseTagsOr(t *testing.T) {
	r, err := ParseReference(":george!bailey,classified")
	if err != nil {
		t.Fatal(err)
	}
	if len(r.Tags) != 1 {
		t.Fatalf("expected 1 tag filter (OR), got %d", len(r.Tags))
	}
	if len(r.Tags[0].Tags) != 2 {
		t.Fatalf("expected 2 tags in OR group, got %d", len(r.Tags[0].Tags))
	}
}

func TestParseGlob(t *testing.T) {
	r, err := ParseReference(":evidence/*.pdf")
	if err != nil {
		t.Fatal(err)
	}
	if r.Glob == nil || *r.Glob != "*.pdf" {
		t.Fatalf("expected glob *.pdf, got %v", r.Glob)
	}
}

func TestParseEmpty(t *testing.T) {
	r, err := ParseReference(":")
	if err != nil {
		t.Fatal(err)
	}
	if r.Kind != KindWorkspace {
		t.Fatalf("expected workspace, got %v", r.Kind)
	}
	if len(r.Scope) != 0 {
		t.Fatalf("expected empty scope, got %v", r.Scope)
	}
}

func TestParseLeadingDot(t *testing.T) {
	r, err := ParseReference(":.sources")
	if err != nil {
		t.Fatal(err)
	}
	if r.Kind != KindWorkspace {
		t.Fatalf("expected workspace, got %v", r.Kind)
	}
	if len(r.Scope) != 1 || r.Scope[0].Names[0] != "sources" {
		t.Fatalf("expected scope [sources], got %v", r.Scope)
	}
}

func TestParseContextBare(t *testing.T) {
	r, err := ParseReference("sources")
	if err != nil {
		t.Fatal(err)
	}
	if r.Kind != KindContext {
		t.Fatalf("expected context, got %v", r.Kind)
	}
	if len(r.Scope) != 1 || r.Scope[0].Names[0] != "sources" {
		t.Fatalf("expected scope [sources], got %v", r.Scope)
	}
}

func TestParseContextDotted(t *testing.T) {
	r, err := ParseReference(".evidence.emails")
	if err != nil {
		t.Fatal(err)
	}
	if r.Kind != KindContext {
		t.Fatalf("expected context, got %v", r.Kind)
	}
	if len(r.Scope) != 2 {
		t.Fatalf("expected 2 scope levels, got %d", len(r.Scope))
	}
}

func TestParseContextWithGlob(t *testing.T) {
	r, err := ParseReference("evidence/*.pdf")
	if err != nil {
		t.Fatal(err)
	}
	if r.Kind != KindContext {
		t.Fatalf("expected context, got %v", r.Kind)
	}
	if r.Glob == nil || *r.Glob != "*.pdf" {
		t.Fatalf("expected glob *.pdf, got %v", r.Glob)
	}
}

func TestParseBarePath(t *testing.T) {
	r, err := ParseReference("./manual_file.pdf")
	if err != nil {
		t.Fatal(err)
	}
	if r.Kind != KindBarePath {
		t.Fatalf("expected bare path, got %v", r.Kind)
	}
	if r.Raw != "./manual_file.pdf" {
		t.Fatalf("expected raw path, got %s", r.Raw)
	}
}

func TestParseBarePathParent(t *testing.T) {
	r, err := ParseReference("../other/file.txt")
	if err != nil {
		t.Fatal(err)
	}
	if r.Kind != KindBarePath {
		t.Fatalf("expected bare path, got %v", r.Kind)
	}
}

func TestParseFullReference(t *testing.T) {
	r, err := ParseReference(":{bailey,george}.evidence!classified/*.pdf")
	if err != nil {
		t.Fatal(err)
	}
	if r.Kind != KindWorkspace {
		t.Fatalf("expected workspace, got %v", r.Kind)
	}
	if len(r.Scope) != 2 {
		t.Fatalf("expected 2 scope levels, got %d", len(r.Scope))
	}
	if len(r.Tags) != 1 || r.Tags[0].Tags[0] != "classified" {
		t.Fatalf("unexpected tags: %v", r.Tags)
	}
	if r.Glob == nil || *r.Glob != "*.pdf" {
		t.Fatalf("expected glob *.pdf, got %v", r.Glob)
	}
}

func TestValidateName(t *testing.T) {
	if err := ValidateName("evidence"); err != nil {
		t.Fatal(err)
	}
	if err := ValidateName("my-project"); err != nil {
		t.Fatal(err)
	}
	if err := ValidateName(""); err == nil {
		t.Fatal("expected error for empty")
	}
	if err := ValidateName("foo:bar"); err == nil {
		t.Fatal("expected error for colon")
	}
	if err := ValidateName("foo.bar"); err == nil {
		t.Fatal("expected error for dot")
	}
	if err := ValidateName("mkrk"); err == nil {
		t.Fatal("expected error for reserved name")
	}
}
