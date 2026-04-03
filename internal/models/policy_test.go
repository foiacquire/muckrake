package models

import "testing"

func TestProtectionLevelRoundtrip(t *testing.T) {
	for _, l := range []ProtectionLevel{ProtectionEditable, ProtectionProtected, ProtectionImmutable} {
		parsed, err := ParseProtectionLevel(string(l))
		if err != nil {
			t.Fatalf("ParseProtectionLevel(%q) error: %v", l, err)
		}
		if parsed != l {
			t.Fatalf("expected %v, got %v", l, parsed)
		}
	}
}

func TestStrictestImmutableWins(t *testing.T) {
	got := Strictest([]ProtectionLevel{ProtectionEditable, ProtectionImmutable, ProtectionProtected})
	if got != ProtectionImmutable {
		t.Fatalf("expected immutable, got %v", got)
	}
}

func TestStrictestEmptyDefaultsEditable(t *testing.T) {
	got := Strictest(nil)
	if got != ProtectionEditable {
		t.Fatalf("expected editable, got %v", got)
	}
}
