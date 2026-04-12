package db

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"time"

	"go.foia.dev/muckrake/internal/models"
)

// --- Ruleset CRUD ---

func (p *ProjectDb) InsertRuleset(rs *models.Ruleset) (int64, error) {
	res, err := p.db.Exec(
		`INSERT INTO rulesets (name, description) VALUES (?, ?)`,
		rs.Name, rs.Description,
	)
	if err != nil {
		return 0, fmt.Errorf("insert ruleset: %w", err)
	}
	return res.LastInsertId()
}

func (p *ProjectDb) GetRulesetByName(name string) (*models.Ruleset, error) {
	var rs models.Ruleset
	var id int64
	err := p.db.QueryRow(
		`SELECT id, name, description FROM rulesets WHERE name = ?`, name,
	).Scan(&id, &rs.Name, &rs.Description)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	rs.ID = &id
	return &rs, nil
}

func (p *ProjectDb) ListRulesets() ([]models.Ruleset, error) {
	rows, err := p.db.Query(`SELECT id, name, description FROM rulesets ORDER BY name`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var rulesets []models.Ruleset
	for rows.Next() {
		var rs models.Ruleset
		var id int64
		if err := rows.Scan(&id, &rs.Name, &rs.Description); err != nil {
			return nil, err
		}
		rs.ID = &id
		rulesets = append(rulesets, rs)
	}
	return rulesets, rows.Err()
}

func (p *ProjectDb) RemoveRuleset(name string) (int64, error) {
	rs, err := p.GetRulesetByName(name)
	if err != nil || rs == nil {
		return 0, err
	}
	rsID := *rs.ID

	p.db.Exec(`DELETE FROM ruleset_files WHERE ruleset_id = ?`, rsID)
	p.db.Exec(`DELETE FROM ruleset_subscriptions WHERE ruleset_id = ?`, rsID)
	p.db.Exec(`DELETE FROM ruleset_rules WHERE ruleset_id = ?`, rsID)

	res, err := p.db.Exec(`DELETE FROM rulesets WHERE id = ?`, rsID)
	if err != nil {
		return 0, err
	}
	return res.RowsAffected()
}

// --- Ruleset Rules ---

func (p *ProjectDb) InsertRulesetRule(rule *models.RulesetRule) (int64, error) {
	var condJSON *string
	if rule.Condition != nil {
		b, _ := json.Marshal(rule.Condition)
		s := string(b)
		condJSON = &s
	}
	configJSON, _ := json.Marshal(rule.ActionConfig)

	res, err := p.db.Exec(
		`INSERT INTO ruleset_rules (ruleset_id, priority, condition, action_type, action_config)
		 VALUES (?, ?, ?, ?, ?)`,
		rule.RulesetID, rule.Priority, condJSON, string(rule.ActionType), string(configJSON),
	)
	if err != nil {
		return 0, fmt.Errorf("insert ruleset rule: %w", err)
	}
	return res.LastInsertId()
}

func (p *ProjectDb) ListRulesForRuleset(rulesetID int64) ([]models.RulesetRule, error) {
	rows, err := p.db.Query(
		`SELECT id, ruleset_id, priority, condition, action_type, action_config
		 FROM ruleset_rules WHERE ruleset_id = ? ORDER BY priority`, rulesetID,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var rules []models.RulesetRule
	for rows.Next() {
		var r models.RulesetRule
		var id int64
		var condJSON sql.NullString
		var actionType, configJSON string
		if err := rows.Scan(&id, &r.RulesetID, &r.Priority, &condJSON, &actionType, &configJSON); err != nil {
			return nil, err
		}
		r.ID = &id
		r.ActionType = models.RulesetActionType(actionType)
		json.Unmarshal([]byte(configJSON), &r.ActionConfig)
		if condJSON.Valid {
			var cond models.RuleCondition
			json.Unmarshal([]byte(condJSON.String), &cond)
			r.Condition = &cond
		}
		rules = append(rules, r)
	}
	return rules, rows.Err()
}

// --- Subscriptions ---

func (p *ProjectDb) SubscribeRuleset(rulesetID int64, reference string) (int64, error) {
	now := time.Now().UTC().Format(time.RFC3339)
	res, err := p.db.Exec(
		`INSERT INTO ruleset_subscriptions (ruleset_id, reference, created_at) VALUES (?, ?, ?)`,
		rulesetID, reference, now,
	)
	if err != nil {
		return 0, fmt.Errorf("subscribe ruleset: %w", err)
	}
	return res.LastInsertId()
}

func (p *ProjectDb) UnsubscribeRuleset(rulesetID int64, reference string) (int64, error) {
	var subID int64
	err := p.db.QueryRow(
		`SELECT id FROM ruleset_subscriptions WHERE ruleset_id = ? AND reference = ?`,
		rulesetID, reference,
	).Scan(&subID)
	if err == sql.ErrNoRows {
		return 0, nil
	}
	if err != nil {
		return 0, err
	}

	p.db.Exec(`DELETE FROM ruleset_files WHERE subscription_id = ?`, subID)
	res, err := p.db.Exec(`DELETE FROM ruleset_subscriptions WHERE id = ?`, subID)
	if err != nil {
		return 0, err
	}
	return res.RowsAffected()
}

func (p *ProjectDb) ListSubscriptionsForRuleset(rulesetID int64) ([]models.Subscription, error) {
	rows, err := p.db.Query(
		`SELECT id, reference, created_at FROM ruleset_subscriptions WHERE ruleset_id = ?`,
		rulesetID,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var subs []models.Subscription
	for rows.Next() {
		var s models.Subscription
		var id int64
		if err := rows.Scan(&id, &s.Reference, &s.CreatedAt); err != nil {
			return nil, err
		}
		s.ID = &id
		subs = append(subs, s)
	}
	return subs, rows.Err()
}

func (p *ProjectDb) ListAllRulesetSubscriptions() ([]RulesetSubscription, error) {
	rows, err := p.db.Query(
		`SELECT ruleset_id, id, reference, created_at FROM ruleset_subscriptions`,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var results []RulesetSubscription
	for rows.Next() {
		var rs RulesetSubscription
		var id int64
		if err := rows.Scan(&rs.RulesetID, &id, &rs.Sub.Reference, &rs.Sub.CreatedAt); err != nil {
			return nil, err
		}
		rs.Sub.ID = &id
		results = append(results, rs)
	}
	return results, rows.Err()
}

// RulesetSubscription pairs a ruleset ID with its subscription.
type RulesetSubscription struct {
	RulesetID int64
	Sub       models.Subscription
}

// --- Materialization ---

func (p *ProjectDb) MaterializeRulesetFile(rulesetID int64, sha256 string, subID int64) error {
	now := time.Now().UTC().Format(time.RFC3339)
	_, err := p.db.Exec(
		`INSERT INTO ruleset_files (ruleset_id, sha256, subscription_id, attached_at)
		 VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING`,
		rulesetID, sha256, subID, now,
	)
	return err
}

func (p *ProjectDb) GetRulesetsForSHA256(sha256 string) ([]models.Ruleset, error) {
	rows, err := p.db.Query(
		`SELECT r.id, r.name, r.description
		 FROM rulesets r
		 INNER JOIN ruleset_files rf ON rf.ruleset_id = r.id
		 WHERE rf.sha256 = ?`, sha256,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var rulesets []models.Ruleset
	for rows.Next() {
		var rs models.Ruleset
		var id int64
		if err := rows.Scan(&id, &rs.Name, &rs.Description); err != nil {
			return nil, err
		}
		rs.ID = &id
		rulesets = append(rulesets, rs)
	}
	return rulesets, rows.Err()
}
