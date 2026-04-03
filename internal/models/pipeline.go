package models

import (
	"encoding/json"
	"fmt"
)

type Pipeline struct {
	ID          *int64
	Name        string
	States      []string
	Transitions map[string][]string
}

// DefaultTransitions creates linear transitions where each non-initial
// state requires a sign with its own name.
func DefaultTransitions(states []string) map[string][]string {
	t := make(map[string][]string)
	for _, s := range states[1:] {
		t[s] = []string{s}
	}
	return t
}

// Validate checks pipeline structural constraints.
func (p *Pipeline) Validate() error {
	if len(p.States) < 2 {
		return fmt.Errorf("pipeline must have at least 2 states")
	}
	initial := p.States[0]
	if _, ok := p.Transitions[initial]; ok {
		return fmt.Errorf("initial state '%s' must not have a transition entry", initial)
	}
	for target, signs := range p.Transitions {
		found := false
		for _, s := range p.States {
			if s == target {
				found = true
				break
			}
		}
		if !found {
			return fmt.Errorf("transition target '%s' is not a defined state", target)
		}
		if len(signs) == 0 {
			return fmt.Errorf("transition to '%s' has no required signs", target)
		}
	}
	for _, s := range p.States[1:] {
		if _, ok := p.Transitions[s]; !ok {
			return fmt.Errorf("non-initial state '%s' has no transition entry", s)
		}
	}
	return nil
}

// StatesJSON returns the JSON representation of states.
func (p *Pipeline) StatesJSON() string {
	b, _ := json.Marshal(p.States)
	return string(b)
}

// TransitionsJSON returns the JSON representation of transitions.
func (p *Pipeline) TransitionsJSON() string {
	b, _ := json.Marshal(p.Transitions)
	return string(b)
}

type Sign struct {
	ID         *int64
	PipelineID int64
	FileID     int64
	FileHash   string
	SignName   string
	Signer     string
	SignedAt   string
	Signature  *string
	RevokedAt  *string
	Source     *string
}

// IsValid returns true if the sign is not revoked and the file hash matches.
func (s *Sign) IsValid(currentHash string) bool {
	return s.RevokedAt == nil && s.FileHash == currentHash
}

type Subscription struct {
	ID        *int64
	Reference string
	CreatedAt string
}
