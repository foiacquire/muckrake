package generator

import (
	"go.foia.dev/muckrake/internal/context"
	"go.foia.dev/muckrake/internal/db"
	"go.foia.dev/muckrake/internal/models"
	"go.foia.dev/muckrake/internal/reference"
)

// Generator represents a dynamically-registered CLI verb backed by a scope.
type Generator struct {
	Verb        string
	AutoScope   bool
	Executable  bool
	ProjectName string
	ProjectRoot string
	Scope       models.Scope
	IsBuiltin   bool
}

// Collect returns all generators visible from the current context, including
// built-in :mkrk generators.
func Collect(ctx *context.Context) ([]Generator, error) {
	gens := Builtins()

	if ctx.Kind == context.ContextProject {
		projName := ""
		if ctx.ProjectName != nil {
			projName = *ctx.ProjectName
		}
		projGens, err := collectFromProject(ctx.ProjectDb, projName, ctx.ProjectRoot)
		if err != nil {
			return nil, err
		}
		gens = append(gens, projGens...)
	}

	return gens, nil
}

func collectFromProject(pdb *db.ProjectDb, projectName, projectRoot string) ([]Generator, error) {
	rulesets, err := pdb.ListRulesets()
	if err != nil {
		return nil, err
	}

	scopes, err := pdb.ListCategories()
	if err != nil {
		return nil, err
	}
	scopesByName := make(map[string]models.Scope, len(scopes))
	for _, s := range scopes {
		scopesByName[s.Name] = s
	}

	var gens []Generator

	for _, rs := range rulesets {
		if rs.ID == nil {
			continue
		}
		rules, err := pdb.ListRulesForRuleset(*rs.ID)
		if err != nil {
			return nil, err
		}

		var genCmd, execRule *models.RulesetRule
		for i, r := range rules {
			if r.ActionType == models.ActionGenerateCommand {
				genCmd = &rules[i]
			}
			if r.ActionType == models.ActionMakeExecutable {
				execRule = &rules[i]
			}
		}
		if genCmd == nil {
			continue
		}

		subs, err := pdb.ListSubscriptionsForRuleset(*rs.ID)
		if err != nil {
			return nil, err
		}
		for _, sub := range subs {
			scope := resolveSubscriptionScope(sub.Reference, scopesByName)
			if scope == nil {
				continue
			}
			gens = append(gens, Generator{
				Verb:        deref(genCmd.ActionConfig.Verb),
				AutoScope:   boolDeref(genCmd.ActionConfig.AutoScope, true),
				Executable:  execRule != nil,
				ProjectName: projectName,
				ProjectRoot: projectRoot,
				Scope:       *scope,
				IsBuiltin:   false,
			})
		}
	}

	return gens, nil
}

// resolveSubscriptionScope interprets a subscription reference as naming a
// single scope. Matches on the deepest scope level name.
func resolveSubscriptionScope(ref string, scopesByName map[string]models.Scope) *models.Scope {
	r, err := reference.ParseReference(ref)
	if err != nil {
		return nil
	}
	if len(r.Scope) == 0 {
		return nil
	}
	last := r.Scope[len(r.Scope)-1]
	if len(last.Names) != 1 {
		return nil
	}
	if s, ok := scopesByName[last.Names[0]]; ok {
		return &s
	}
	return nil
}

func deref(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

func boolDeref(b *bool, def bool) bool {
	if b == nil {
		return def
	}
	return *b
}
