package db

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"time"

	"go.foia.dev/muckrake/internal/models"
)

// --- Pipeline CRUD ---

func (p *ProjectDb) InsertPipeline(pl *models.Pipeline) (int64, error) {
	statesJSON, _ := json.Marshal(pl.States)
	transJSON, _ := json.Marshal(pl.Transitions)
	res, err := p.db.Exec(
		`INSERT INTO pipelines (name, states, transitions) VALUES (?, ?, ?)`,
		pl.Name, string(statesJSON), string(transJSON),
	)
	if err != nil {
		return 0, fmt.Errorf("insert pipeline: %w", err)
	}
	return res.LastInsertId()
}

func (p *ProjectDb) GetPipelineByName(name string) (*models.Pipeline, error) {
	var pl models.Pipeline
	var id int64
	var statesJSON, transJSON string
	err := p.db.QueryRow(
		`SELECT id, name, states, transitions FROM pipelines WHERE name = ?`, name,
	).Scan(&id, &pl.Name, &statesJSON, &transJSON)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	pl.ID = &id
	json.Unmarshal([]byte(statesJSON), &pl.States)
	json.Unmarshal([]byte(transJSON), &pl.Transitions)
	return &pl, nil
}

func (p *ProjectDb) ListPipelines() ([]models.Pipeline, error) {
	rows, err := p.db.Query(`SELECT id, name, states, transitions FROM pipelines ORDER BY name`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var pipelines []models.Pipeline
	for rows.Next() {
		var pl models.Pipeline
		var id int64
		var statesJSON, transJSON string
		if err := rows.Scan(&id, &pl.Name, &statesJSON, &transJSON); err != nil {
			return nil, err
		}
		pl.ID = &id
		json.Unmarshal([]byte(statesJSON), &pl.States)
		json.Unmarshal([]byte(transJSON), &pl.Transitions)
		pipelines = append(pipelines, pl)
	}
	return pipelines, rows.Err()
}

func (p *ProjectDb) RemovePipeline(name string) (int64, error) {
	pl, err := p.GetPipelineByName(name)
	if err != nil || pl == nil {
		return 0, err
	}
	pid := *pl.ID

	p.db.Exec(`DELETE FROM pipeline_files WHERE pipeline_id = ?`, pid)
	p.db.Exec(`DELETE FROM pipeline_subscriptions WHERE pipeline_id = ?`, pid)
	p.db.Exec(`DELETE FROM signs WHERE pipeline_id = ?`, pid)

	res, err := p.db.Exec(`DELETE FROM pipelines WHERE id = ?`, pid)
	if err != nil {
		return 0, err
	}
	return res.RowsAffected()
}

// --- Subscriptions ---

func (p *ProjectDb) SubscribePipeline(pipelineID int64, reference string) (int64, error) {
	now := time.Now().UTC().Format(time.RFC3339)
	res, err := p.db.Exec(
		`INSERT INTO pipeline_subscriptions (pipeline_id, reference, created_at) VALUES (?, ?, ?)`,
		pipelineID, reference, now,
	)
	if err != nil {
		return 0, fmt.Errorf("subscribe pipeline: %w", err)
	}
	return res.LastInsertId()
}

func (p *ProjectDb) UnsubscribePipeline(pipelineID int64, reference string) (int64, error) {
	var subID int64
	err := p.db.QueryRow(
		`SELECT id FROM pipeline_subscriptions WHERE pipeline_id = ? AND reference = ?`,
		pipelineID, reference,
	).Scan(&subID)
	if err == sql.ErrNoRows {
		return 0, nil
	}
	if err != nil {
		return 0, err
	}

	p.db.Exec(`DELETE FROM pipeline_files WHERE subscription_id = ?`, subID)
	res, err := p.db.Exec(`DELETE FROM pipeline_subscriptions WHERE id = ?`, subID)
	if err != nil {
		return 0, err
	}
	return res.RowsAffected()
}

func (p *ProjectDb) ListPipelineSubscriptions(pipelineID int64) ([]models.Subscription, error) {
	rows, err := p.db.Query(
		`SELECT id, reference, created_at FROM pipeline_subscriptions
		 WHERE pipeline_id = ? ORDER BY reference`, pipelineID,
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

func (p *ProjectDb) ListAllPipelineSubscriptions() ([]PipelineSubscription, error) {
	rows, err := p.db.Query(
		`SELECT pipeline_id, id, reference, created_at FROM pipeline_subscriptions`,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var results []PipelineSubscription
	for rows.Next() {
		var ps PipelineSubscription
		var id int64
		if err := rows.Scan(&ps.PipelineID, &id, &ps.Sub.Reference, &ps.Sub.CreatedAt); err != nil {
			return nil, err
		}
		ps.Sub.ID = &id
		results = append(results, ps)
	}
	return results, rows.Err()
}

// PipelineSubscription pairs a pipeline ID with its subscription.
type PipelineSubscription struct {
	PipelineID int64
	Sub        models.Subscription
}

// --- Materialization ---

func (p *ProjectDb) MaterializePipelineFile(pipelineID int64, sha256 string, subID int64) error {
	now := time.Now().UTC().Format(time.RFC3339)
	_, err := p.db.Exec(
		`INSERT INTO pipeline_files (pipeline_id, sha256, subscription_id, attached_at)
		 VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING`,
		pipelineID, sha256, subID, now,
	)
	return err
}

func (p *ProjectDb) GetPipelinesForSHA256(sha256 string) ([]models.Pipeline, error) {
	rows, err := p.db.Query(
		`SELECT p.id, p.name, p.states, p.transitions
		 FROM pipelines p
		 INNER JOIN pipeline_files pf ON pf.pipeline_id = p.id
		 WHERE pf.sha256 = ?`, sha256,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var pipelines []models.Pipeline
	for rows.Next() {
		var pl models.Pipeline
		var id int64
		var statesJSON, transJSON string
		if err := rows.Scan(&id, &pl.Name, &statesJSON, &transJSON); err != nil {
			return nil, err
		}
		pl.ID = &id
		json.Unmarshal([]byte(statesJSON), &pl.States)
		json.Unmarshal([]byte(transJSON), &pl.Transitions)
		pipelines = append(pipelines, pl)
	}
	return pipelines, rows.Err()
}

// --- Signs ---

func (p *ProjectDb) InsertSign(s *models.Sign) (int64, error) {
	res, err := p.db.Exec(
		`INSERT INTO signs (pipeline_id, file_id, file_hash, sign_name, signer, signed_at, signature)
		 VALUES (?, ?, ?, ?, ?, ?, ?)`,
		s.PipelineID, s.FileID, s.FileHash, s.SignName, s.Signer, s.SignedAt, s.Signature,
	)
	if err != nil {
		return 0, fmt.Errorf("insert sign: %w", err)
	}
	return res.LastInsertId()
}

func (p *ProjectDb) RevokeSign(signID int64, revokedAt string) (int64, error) {
	res, err := p.db.Exec(
		`UPDATE signs SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL`,
		revokedAt, signID,
	)
	if err != nil {
		return 0, err
	}
	return res.RowsAffected()
}

func (p *ProjectDb) GetValidSignsForFilePipeline(fileID, pipelineID int64, currentHash string) ([]models.Sign, error) {
	rows, err := p.db.Query(
		`SELECT id, pipeline_id, file_id, file_hash, sign_name, signer, signed_at, signature, revoked_at
		 FROM signs
		 WHERE file_id = ? AND pipeline_id = ? AND file_hash = ? AND revoked_at IS NULL
		 ORDER BY signed_at`, fileID, pipelineID, currentHash,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanSigns(rows)
}

func (p *ProjectDb) GetSignsForFile(fileID int64) ([]models.Sign, error) {
	rows, err := p.db.Query(
		`SELECT id, pipeline_id, file_id, file_hash, sign_name, signer, signed_at, signature, revoked_at
		 FROM signs WHERE file_id = ? ORDER BY pipeline_id, signed_at`, fileID,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanSigns(rows)
}

func scanSigns(rows *sql.Rows) ([]models.Sign, error) {
	var signs []models.Sign
	for rows.Next() {
		var s models.Sign
		var id int64
		if err := rows.Scan(&id, &s.PipelineID, &s.FileID, &s.FileHash,
			&s.SignName, &s.Signer, &s.SignedAt, &s.Signature, &s.RevokedAt); err != nil {
			return nil, err
		}
		s.ID = &id
		signs = append(signs, s)
	}
	return signs, rows.Err()
}
