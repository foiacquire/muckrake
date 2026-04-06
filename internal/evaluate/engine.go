package evaluate

import (
	"strings"

	"go.foia.dev/muckrake/internal/db"
	"go.foia.dev/muckrake/internal/models"
)

// EvaluationResult collects outputs from evaluating all rulesets for a file.
type EvaluationResult struct {
	Protection      models.ProtectionLevel
	ToolDispatches  []ToolDispatch
	TagsToAdd       []string
	TagsToRemove    []string
	PipelinesToAttach []string
	VirtualPipelines  []string
}

// ToolDispatch describes a tool to invoke for a file.
type ToolDispatch struct {
	Command     string
	Env         *string
	Quiet       bool
	FileType    string
	RulesetName string
}

// EvalContext carries file metadata for condition matching.
type EvalContext struct {
	SHA256   string
	MimeType *string
	FileType *string
}

// EvaluateForFile loads all materialized rulesets for a file hash,
// evaluates their rules, and returns collected action outputs.
func EvaluateForFile(pdb *db.ProjectDb, ctx *EvalContext) (*EvaluationResult, error) {
	rulesets, err := pdb.GetRulesetsForSHA256(ctx.SHA256)
	if err != nil {
		return nil, err
	}

	result := &EvaluationResult{Protection: models.ProtectionEditable}

	for i := range rulesets {
		rs := &rulesets[i]
		rules, err := pdb.ListRulesForRuleset(*rs.ID)
		if err != nil {
			return nil, err
		}
		evaluateRules(rules, rs, ctx, result)
	}

	return result, nil
}

// ResolveProtectionByHash resolves protection level for a file using
// materialized rulesets.
func ResolveProtectionByHash(pdb *db.ProjectDb, sha256 string) (models.ProtectionLevel, error) {
	ctx := &EvalContext{SHA256: sha256}
	result, err := EvaluateForFile(pdb, ctx)
	if err != nil {
		return models.ProtectionEditable, err
	}
	return result.Protection, nil
}

func evaluateRules(rules []models.RulesetRule, rs *models.Ruleset, ctx *EvalContext, result *EvaluationResult) {
	for i := range rules {
		rule := &rules[i]
		if !matchesCondition(rule, ctx) {
			continue
		}
		applyAction(rule.ActionType, &rule.ActionConfig, rs, result)
	}
}

func matchesCondition(rule *models.RulesetRule, ctx *EvalContext) bool {
	if rule.Condition == nil {
		return true
	}
	cond := rule.Condition

	if cond.MimeType != nil {
		fileMime := ""
		if ctx.MimeType != nil {
			fileMime = *ctx.MimeType
		}
		if !mimeMatches(*cond.MimeType, fileMime) {
			return false
		}
	}

	if cond.FileType != nil {
		fileFt := ""
		if ctx.FileType != nil {
			fileFt = *ctx.FileType
		}
		if *cond.FileType != fileFt && *cond.FileType != "*" {
			return false
		}
	}

	return true
}

func mimeMatches(pattern, actual string) bool {
	if pattern == "*" || pattern == actual {
		return true
	}
	if prefix, ok := strings.CutSuffix(pattern, "/*"); ok {
		return strings.HasPrefix(actual, prefix+"/")
	}
	return false
}

func applyAction(actionType models.RulesetActionType, config *models.RulesetActionConfig, rs *models.Ruleset, result *EvaluationResult) {
	switch actionType {
	case models.ActionApplyPolicy:
		if config.ProtectionLevel != nil {
			if level, err := models.ParseProtectionLevel(*config.ProtectionLevel); err == nil {
				result.Protection = models.Strictest([]models.ProtectionLevel{result.Protection, level})
			}
		}
	case models.ActionDispatchTool:
		if config.Command != nil {
			ft := "*"
			if config.FileType != nil {
				ft = *config.FileType
			}
			result.ToolDispatches = append(result.ToolDispatches, ToolDispatch{
				Command:     *config.Command,
				Env:         config.Env,
				Quiet:       config.Quiet != nil && *config.Quiet,
				FileType:    ft,
				RulesetName: rs.Name,
			})
		}
	case models.ActionAddTag:
		if config.Tag != nil {
			result.TagsToAdd = append(result.TagsToAdd, *config.Tag)
		}
	case models.ActionRemoveTag:
		if config.Tag != nil {
			result.TagsToRemove = append(result.TagsToRemove, *config.Tag)
		}
	case models.ActionAttachPipeline:
		if config.Pipeline != nil {
			result.PipelinesToAttach = append(result.PipelinesToAttach, *config.Pipeline)
		}
	case models.ActionAttachPipelineVirtual:
		if config.Pipeline != nil {
			result.VirtualPipelines = append(result.VirtualPipelines, *config.Pipeline)
		}
	}
}
