package cli

import (
	"flag"
	"fmt"
	"os"
	"strconv"
	"strings"

	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/models"
)

// addRule adds a single rule to an existing ruleset.
//
//	mkrk add rule <ruleset> <action-type> [config-flags...]
func addRule(ctx *context.Context, args []string) error {
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}
	if len(args) < 2 {
		return fmt.Errorf("usage: mkrk add rule <ruleset> <action-type> [flags]")
	}
	rulesetName := args[0]
	actionStr := args[1]
	flagArgs := args[2:]

	rs, err := ctx.ProjectDb.GetRulesetByName(rulesetName)
	if err != nil {
		return err
	}
	if rs == nil || rs.ID == nil {
		return fmt.Errorf("ruleset '%s' not found", rulesetName)
	}

	action, err := models.ParseRulesetActionType(actionStr)
	if err != nil {
		return err
	}

	cfg, err := parseRuleActionConfig(action, flagArgs)
	if err != nil {
		return err
	}

	priority, err := ctx.ProjectDb.NextRulePriority(*rs.ID)
	if err != nil {
		return err
	}

	rule := &models.RulesetRule{
		RulesetID:    *rs.ID,
		Priority:     priority,
		ActionType:   action,
		ActionConfig: cfg,
	}
	id, err := ctx.ProjectDb.InsertRulesetRule(rule)
	if err != nil {
		return err
	}
	fmt.Fprintf(os.Stderr, "Added rule %s to ruleset '%s' (priority %d, id %d)\n",
		actionStr, rulesetName, priority, id)
	return nil
}

func removeRule(ctx *context.Context, args []string) error {
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}
	if len(args) < 2 {
		return fmt.Errorf("usage: mkrk remove rule <ruleset> <priority>")
	}
	rulesetName := args[0]
	priorityStr := args[1]

	priority, err := strconv.Atoi(priorityStr)
	if err != nil {
		return fmt.Errorf("priority must be a number")
	}

	rs, err := ctx.ProjectDb.GetRulesetByName(rulesetName)
	if err != nil {
		return err
	}
	if rs == nil || rs.ID == nil {
		return fmt.Errorf("ruleset '%s' not found", rulesetName)
	}

	rules, err := ctx.ProjectDb.ListRulesForRuleset(*rs.ID)
	if err != nil {
		return err
	}
	for _, r := range rules {
		if r.Priority == priority && r.ID != nil {
			if _, err := ctx.ProjectDb.RemoveRulesetRule(*r.ID); err != nil {
				return err
			}
			fmt.Fprintf(os.Stderr, "Removed rule %s from '%s' (priority %d)\n",
				r.ActionType, rulesetName, priority)
			return nil
		}
	}
	return fmt.Errorf("no rule with priority %d in ruleset '%s'", priority, rulesetName)
}

func addSubscription(ctx *context.Context, args []string) error {
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}
	if len(args) < 2 {
		return fmt.Errorf("usage: mkrk add subscription <ruleset> <reference>")
	}
	rulesetName := args[0]
	ref := args[1]

	rs, err := ctx.ProjectDb.GetRulesetByName(rulesetName)
	if err != nil {
		return err
	}
	if rs == nil || rs.ID == nil {
		return fmt.Errorf("ruleset '%s' not found", rulesetName)
	}

	id, err := ctx.ProjectDb.SubscribeRuleset(*rs.ID, ref)
	if err != nil {
		return err
	}
	fmt.Fprintf(os.Stderr, "Subscribed '%s' to %s (id %d)\n", rulesetName, ref, id)
	return nil
}

func removeSubscription(ctx *context.Context, args []string) error {
	if ctx.Kind != context.ContextProject {
		return fmt.Errorf("not in a project")
	}
	if len(args) < 2 {
		return fmt.Errorf("usage: mkrk remove subscription <ruleset> <reference>")
	}
	rulesetName := args[0]
	ref := args[1]

	rs, err := ctx.ProjectDb.GetRulesetByName(rulesetName)
	if err != nil {
		return err
	}
	if rs == nil || rs.ID == nil {
		return fmt.Errorf("ruleset '%s' not found", rulesetName)
	}

	removed, err := ctx.ProjectDb.UnsubscribeRuleset(*rs.ID, ref)
	if err != nil {
		return err
	}
	if removed == 0 {
		return fmt.Errorf("no subscription to %s on ruleset '%s'", ref, rulesetName)
	}
	fmt.Fprintf(os.Stderr, "Unsubscribed '%s' from %s\n", rulesetName, ref)
	return nil
}

// parseRuleActionConfig builds the per-action config from CLI flags. Each
// action type only consumes the flags it cares about.
func parseRuleActionConfig(action models.RulesetActionType, args []string) (models.RulesetActionConfig, error) {
	fs := flag.NewFlagSet("rule-config", flag.ContinueOnError)
	verb := fs.String("verb", "", "command verb (generate_command)")
	autoScope := fs.Bool("auto-scope", true, "auto-scope first arg into generating scope (generate_command)")
	verbSet := false
	autoScopeSet := false

	protection := fs.String("protection-level", "", "immutable|protected|editable (apply_policy)")
	command := fs.String("command", "", "shell command template (dispatch_tool)")
	fileType := fs.String("file-type", "", "file extension filter (dispatch_tool)")
	env := fs.String("env", "", "comma-separated KEY=VAL pairs (dispatch_tool)")
	quiet := fs.Bool("quiet", false, "suppress runtime privacy notice (dispatch_tool)")
	quietSet := false

	tag := fs.String("tag", "", "tag name (add_tag/remove_tag)")
	pipeline := fs.String("pipeline", "", "pipeline name (attach_pipeline*, sign, unsign)")
	signName := fs.String("sign-name", "", "sign name (sign, unsign)")

	if err := fs.Parse(args); err != nil {
		return models.RulesetActionConfig{}, err
	}
	fs.Visit(func(f *flag.Flag) {
		switch f.Name {
		case "verb":
			verbSet = true
		case "auto-scope":
			autoScopeSet = true
		case "quiet":
			quietSet = true
		}
	})

	cfg := models.RulesetActionConfig{}
	switch action {
	case models.ActionMakeExecutable:
		// no config
	case models.ActionGenerateCommand:
		if !verbSet || *verb == "" {
			return cfg, fmt.Errorf("generate_command requires --verb")
		}
		cfg.Verb = verb
		if autoScopeSet {
			cfg.AutoScope = autoScope
		} else {
			t := true
			cfg.AutoScope = &t
		}
	case models.ActionApplyPolicy:
		if *protection == "" {
			return cfg, fmt.Errorf("apply_policy requires --protection-level")
		}
		if _, err := models.ParseProtectionLevel(*protection); err != nil {
			return cfg, err
		}
		cfg.ProtectionLevel = protection
	case models.ActionDispatchTool:
		if *command == "" {
			return cfg, fmt.Errorf("dispatch_tool requires --command")
		}
		cfg.Command = command
		if *fileType != "" {
			cfg.FileType = fileType
		}
		if *env != "" {
			cfg.Env = env
		}
		if quietSet {
			cfg.Quiet = quiet
		}
	case models.ActionAddTag, models.ActionRemoveTag:
		if *tag == "" {
			return cfg, fmt.Errorf("%s requires --tag", action)
		}
		cfg.Tag = tag
	case models.ActionAttachPipeline, models.ActionAttachPipelineVirtual:
		if *pipeline == "" {
			return cfg, fmt.Errorf("%s requires --pipeline", action)
		}
		cfg.Pipeline = pipeline
	case models.ActionSign, models.ActionUnsign:
		if *pipeline == "" || *signName == "" {
			return cfg, fmt.Errorf("%s requires --pipeline and --sign-name", action)
		}
		cfg.Pipeline = pipeline
		cfg.SignName = signName
	default:
		return cfg, fmt.Errorf("unsupported action: %s", action)
	}
	return cfg, nil
}

// formatRuleSummary renders a rule for status output.
func formatRuleSummary(r *models.RulesetRule) string {
	parts := []string{string(r.ActionType)}
	if r.ActionConfig.Verb != nil {
		parts = append(parts, "verb="+*r.ActionConfig.Verb)
	}
	if r.ActionConfig.ProtectionLevel != nil {
		parts = append(parts, "level="+*r.ActionConfig.ProtectionLevel)
	}
	if r.ActionConfig.Tag != nil {
		parts = append(parts, "tag="+*r.ActionConfig.Tag)
	}
	if r.ActionConfig.Pipeline != nil {
		parts = append(parts, "pipeline="+*r.ActionConfig.Pipeline)
	}
	if r.ActionConfig.Command != nil {
		parts = append(parts, "command="+*r.ActionConfig.Command)
	}
	return strings.Join(parts, " ")
}
