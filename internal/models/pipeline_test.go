package models

import "testing"

func TestDefaultTransitions(t *testing.T) {
	states := []string{"draft", "review", "published"}
	tr := DefaultTransitions(states)

	if _, ok := tr["draft"]; ok {
		t.Fatal("initial state should not have transition")
	}
	if tr["review"][0] != "review" {
		t.Fatalf("expected review transition, got %v", tr["review"])
	}
	if tr["published"][0] != "published" {
		t.Fatalf("expected published transition, got %v", tr["published"])
	}
}

func TestPipelineValidate(t *testing.T) {
	p := Pipeline{
		Name:        "test",
		States:      []string{"draft", "review", "published"},
		Transitions: DefaultTransitions([]string{"draft", "review", "published"}),
	}
	if err := p.Validate(); err != nil {
		t.Fatalf("valid pipeline failed: %v", err)
	}
}

func TestPipelineValidateTooFewStates(t *testing.T) {
	p := Pipeline{
		Name:        "test",
		States:      []string{"only"},
		Transitions: map[string][]string{},
	}
	if err := p.Validate(); err == nil {
		t.Fatal("expected error for single state pipeline")
	}
}

func TestSignIsValid(t *testing.T) {
	s := Sign{FileHash: "abc123"}
	if !s.IsValid("abc123") {
		t.Fatal("sign should be valid when hash matches")
	}
	if s.IsValid("different") {
		t.Fatal("sign should not be valid when hash differs")
	}

	revoked := "2025-01-01T00:00:00Z"
	s.RevokedAt = &revoked
	if s.IsValid("abc123") {
		t.Fatal("revoked sign should not be valid")
	}
}
