package models

import "fmt"

type TriggerEvent string

const (
	TriggerIngest         TriggerEvent = "ingest"
	TriggerTag            TriggerEvent = "tag"
	TriggerUntag          TriggerEvent = "untag"
	TriggerSign           TriggerEvent = "sign"
	TriggerStateChange    TriggerEvent = "state_change"
	TriggerProjectEnter   TriggerEvent = "project_enter"
	TriggerWorkspaceEnter TriggerEvent = "workspace_enter"
)

func ParseTriggerEvent(s string) (TriggerEvent, error) {
	switch s {
	case "ingest":
		return TriggerIngest, nil
	case "tag":
		return TriggerTag, nil
	case "untag":
		return TriggerUntag, nil
	case "sign":
		return TriggerSign, nil
	case "state_change":
		return TriggerStateChange, nil
	case "project_enter":
		return TriggerProjectEnter, nil
	case "workspace_enter":
		return TriggerWorkspaceEnter, nil
	default:
		return "", fmt.Errorf("unknown trigger event: %s", s)
	}
}

type ActionType string

const (
	EventActionRunTool        ActionType = "run_tool"
	EventActionAddTag         ActionType = "add_tag"
	EventActionRemoveTag      ActionType = "remove_tag"
	EventActionSign           ActionType = "sign"
	EventActionUnsign         ActionType = "unsign"
	EventActionAttachPipeline ActionType = "attach_pipeline"
	EventActionDetachPipeline ActionType = "detach_pipeline"
)

type TriggerFilter struct {
	TagName  *string `json:"tag_name,omitempty"`
	Category *string `json:"category,omitempty"`
	MimeType *string `json:"mime_type,omitempty"`
	FileType *string `json:"file_type,omitempty"`
	Pipeline *string `json:"pipeline,omitempty"`
	SignName *string `json:"sign_name,omitempty"`
	State    *string `json:"state,omitempty"`
}

func (f *TriggerFilter) IsEmpty() bool {
	return f.TagName == nil && f.Category == nil && f.MimeType == nil &&
		f.FileType == nil && f.Pipeline == nil && f.SignName == nil && f.State == nil
}

type ActionConfig struct {
	Tool     *string `json:"tool,omitempty"`
	Tag      *string `json:"tag,omitempty"`
	Pipeline *string `json:"pipeline,omitempty"`
	SignName *string `json:"sign_name,omitempty"`
	Category *string `json:"category,omitempty"`
}

type Rule struct {
	ID            *int64
	Name          string
	Enabled       bool
	TriggerEvent  TriggerEvent
	TriggerFilter TriggerFilter
	ActionType    ActionType
	ActionConfig  ActionConfig
	Priority      int
	CreatedAt     string
}
