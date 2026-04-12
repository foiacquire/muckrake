package generator

import "go.foia.dev/muckrake/internal/models"

// BuiltinProjectName is the name of the virtual project providing built-in
// generators. It is reserved — no user project may use this name.
const BuiltinProjectName = "mkrk"

// Builtins returns generators provided by muckrake itself. These exist without
// any database entry and have no filesystem root; execution of their members
// is handled by a Go-side registry (see internal/builtins).
func Builtins() []Generator {
	toolsPattern := "tools/**"
	toolsScope := models.Scope{
		Name:      "tools",
		ScopeType: models.ScopeTypeCategory,
		Pattern:   &toolsPattern,
	}
	return []Generator{
		{
			Verb:        "tool",
			AutoScope:   true,
			Executable:  true,
			ProjectName: BuiltinProjectName,
			ProjectRoot: "",
			Scope:       toolsScope,
			IsBuiltin:   true,
		},
	}
}
