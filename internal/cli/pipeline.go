package cli

import (
	"encoding/json"
	"fmt"
	"strings"

	"go.foia.dev/muckrake/internal/models"
)

// extractName pulls the first non-flag positional out of args. The rest
// (including flags and their values) is returned as flagArgs.
func extractName(args []string) (string, []string) {
	var name string
	var rest []string
	for _, a := range args {
		if name == "" && !strings.HasPrefix(a, "-") {
			name = a
		} else {
			rest = append(rest, a)
		}
	}
	return name, rest
}

func parseTransitions(jsonStr string, pl *models.Pipeline) error {
	var trans map[string][]string
	if err := json.Unmarshal([]byte(jsonStr), &trans); err != nil {
		return fmt.Errorf("invalid transitions JSON: %w", err)
	}
	pl.Transitions = trans
	return nil
}
