package models

import "fmt"

type Ruleset struct {
	ID          *int64
	Name        string
	Description *string
}

type RulesetActionType string

const (
	ActionApplyPolicy           RulesetActionType = "apply_policy"
	ActionDispatchTool          RulesetActionType = "dispatch_tool"
	ActionAddTag                RulesetActionType = "add_tag"
	ActionRemoveTag             RulesetActionType = "remove_tag"
	ActionSign                  RulesetActionType = "sign"
	ActionUnsign                RulesetActionType = "unsign"
	ActionAttachPipeline        RulesetActionType = "attach_pipeline"
	ActionAttachPipelineVirtual RulesetActionType = "attach_pipeline_virtual"
)

func ParseRulesetActionType(s string) (RulesetActionType, error) {
	switch s {
	case "apply_policy":
		return ActionApplyPolicy, nil
	case "dispatch_tool":
		return ActionDispatchTool, nil
	case "add_tag":
		return ActionAddTag, nil
	case "remove_tag":
		return ActionRemoveTag, nil
	case "sign":
		return ActionSign, nil
	case "unsign":
		return ActionUnsign, nil
	case "attach_pipeline":
		return ActionAttachPipeline, nil
	case "attach_pipeline_virtual":
		return ActionAttachPipelineVirtual, nil
	default:
		return "", fmt.Errorf("unknown ruleset action type: %s", s)
	}
}

type RuleCondition struct {
	MimeType *string `json:"mime_type,omitempty"`
	FileType *string `json:"file_type,omitempty"`
}

type RulesetActionConfig struct {
	ProtectionLevel *string `json:"protection_level,omitempty"`
	Command         *string `json:"command,omitempty"`
	Env             *string `json:"env,omitempty"`
	Quiet           *bool   `json:"quiet,omitempty"`
	FileType        *string `json:"file_type,omitempty"`
	Tag             *string `json:"tag,omitempty"`
	Pipeline        *string `json:"pipeline,omitempty"`
	SignName        *string `json:"sign_name,omitempty"`
}

type RulesetRule struct {
	ID           *int64
	RulesetID    int64
	Priority     int
	Condition    *RuleCondition
	ActionType   RulesetActionType
	ActionConfig RulesetActionConfig
}
