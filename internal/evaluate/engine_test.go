package evaluate

import (
	"path/filepath"
	"testing"

	"go.foia.dev/muckrake/internal/db"
	"go.foia.dev/muckrake/internal/models"
)

func setupDb(t *testing.T) *db.ProjectDb {
	t.Helper()
	pdb, err := db.CreateProject(filepath.Join(t.TempDir(), ".mkrk"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { pdb.Close() })
	return pdb
}

func strPtr(s string) *string { return &s }
func boolPtr(b bool) *bool   { return &b }

func TestEvaluatePolicyStrictestWins(t *testing.T) {
	pdb := setupDb(t)

	rs := &models.Ruleset{Name: "test-policy"}
	rsID, _ := pdb.InsertRuleset(rs)

	pdb.InsertRulesetRule(&models.RulesetRule{
		RulesetID:  rsID,
		Priority:   0,
		ActionType: models.ActionApplyPolicy,
		ActionConfig: models.RulesetActionConfig{
			ProtectionLevel: strPtr("editable"),
		},
	})
	pdb.InsertRulesetRule(&models.RulesetRule{
		RulesetID:  rsID,
		Priority:   1,
		ActionType: models.ActionApplyPolicy,
		ActionConfig: models.RulesetActionConfig{
			ProtectionLevel: strPtr("immutable"),
		},
	})

	subID, _ := pdb.SubscribeRuleset(rsID, ":evidence")
	pdb.MaterializeRulesetFile(rsID, "abc123", subID)

	result, err := EvaluateForFile(pdb, &EvalContext{SHA256: "abc123"})
	if err != nil {
		t.Fatal(err)
	}
	if result.Protection != models.ProtectionImmutable {
		t.Fatalf("expected immutable, got %v", result.Protection)
	}
}

func TestEvaluateToolDispatch(t *testing.T) {
	pdb := setupDb(t)

	rs := &models.Ruleset{Name: "test-tools"}
	rsID, _ := pdb.InsertRuleset(rs)

	pdb.InsertRulesetRule(&models.RulesetRule{
		RulesetID:  rsID,
		Priority:   0,
		ActionType: models.ActionDispatchTool,
		ActionConfig: models.RulesetActionConfig{
			Command:  strPtr("ocr"),
			Quiet:    boolPtr(true),
			FileType: strPtr("*"),
		},
	})

	subID, _ := pdb.SubscribeRuleset(rsID, ":evidence")
	pdb.MaterializeRulesetFile(rsID, "abc123", subID)

	result, err := EvaluateForFile(pdb, &EvalContext{SHA256: "abc123"})
	if err != nil {
		t.Fatal(err)
	}
	if len(result.ToolDispatches) != 1 || result.ToolDispatches[0].Command != "ocr" {
		t.Fatalf("expected ocr dispatch, got %v", result.ToolDispatches)
	}
}

func TestConditionFiltersMime(t *testing.T) {
	pdb := setupDb(t)

	rs := &models.Ruleset{Name: "pdf-only"}
	rsID, _ := pdb.InsertRuleset(rs)

	pdb.InsertRulesetRule(&models.RulesetRule{
		RulesetID: rsID,
		Priority:  0,
		Condition: &models.RuleCondition{
			MimeType: strPtr("application/pdf"),
		},
		ActionType: models.ActionDispatchTool,
		ActionConfig: models.RulesetActionConfig{
			Command: strPtr("pdf-tool"),
		},
	})

	subID, _ := pdb.SubscribeRuleset(rsID, ":evidence")
	pdb.MaterializeRulesetFile(rsID, "abc123", subID)

	// Should not match image/png
	result, _ := EvaluateForFile(pdb, &EvalContext{
		SHA256:   "abc123",
		MimeType: strPtr("image/png"),
	})
	if len(result.ToolDispatches) != 0 {
		t.Fatal("should not dispatch for wrong mime")
	}

	// Should match application/pdf
	result, _ = EvaluateForFile(pdb, &EvalContext{
		SHA256:   "abc123",
		MimeType: strPtr("application/pdf"),
	})
	if len(result.ToolDispatches) != 1 {
		t.Fatal("should dispatch for matching mime")
	}
}

func TestConditionWildcardMime(t *testing.T) {
	pdb := setupDb(t)

	rs := &models.Ruleset{Name: "images"}
	rsID, _ := pdb.InsertRuleset(rs)

	pdb.InsertRulesetRule(&models.RulesetRule{
		RulesetID: rsID,
		Priority:  0,
		Condition: &models.RuleCondition{
			MimeType: strPtr("image/*"),
		},
		ActionType: models.ActionDispatchTool,
		ActionConfig: models.RulesetActionConfig{
			Command: strPtr("viewer"),
		},
	})

	subID, _ := pdb.SubscribeRuleset(rsID, ":evidence")
	pdb.MaterializeRulesetFile(rsID, "abc", subID)

	result, _ := EvaluateForFile(pdb, &EvalContext{
		SHA256:   "abc",
		MimeType: strPtr("image/png"),
	})
	if len(result.ToolDispatches) != 1 {
		t.Fatal("image/* should match image/png")
	}
}

func TestResolveProtectionByHash(t *testing.T) {
	pdb := setupDb(t)

	rs := &models.Ruleset{Name: "evidence-policy"}
	rsID, _ := pdb.InsertRuleset(rs)

	pdb.InsertRulesetRule(&models.RulesetRule{
		RulesetID:  rsID,
		Priority:   0,
		ActionType: models.ActionApplyPolicy,
		ActionConfig: models.RulesetActionConfig{
			ProtectionLevel: strPtr("immutable"),
		},
	})

	subID, _ := pdb.SubscribeRuleset(rsID, ":evidence")
	pdb.MaterializeRulesetFile(rsID, "hash123", subID)

	level, err := ResolveProtectionByHash(pdb, "hash123")
	if err != nil {
		t.Fatal(err)
	}
	if level != models.ProtectionImmutable {
		t.Fatalf("expected immutable, got %v", level)
	}

	level, _ = ResolveProtectionByHash(pdb, "unknown")
	if level != models.ProtectionEditable {
		t.Fatalf("expected editable for unknown hash, got %v", level)
	}
}
