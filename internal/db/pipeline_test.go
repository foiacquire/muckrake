package db

import (
	"testing"

	"go.foia.dev/muckrake/internal/models"
)

func makePipeline(name string, states []string) *models.Pipeline {
	return &models.Pipeline{
		Name:        name,
		States:      states,
		Transitions: models.DefaultTransitions(states),
	}
}

func TestPipelineCRUD(t *testing.T) {
	db := testDb(t)
	pl := makePipeline("review", []string{"draft", "done"})

	id, err := db.InsertPipeline(pl)
	if err != nil {
		t.Fatal(err)
	}
	if id <= 0 {
		t.Fatal("expected positive id")
	}

	found, err := db.GetPipelineByName("review")
	if err != nil {
		t.Fatal(err)
	}
	if found == nil || found.Name != "review" {
		t.Fatal("expected to find review")
	}
	if len(found.States) != 2 {
		t.Fatalf("expected 2 states, got %d", len(found.States))
	}

	all, _ := db.ListPipelines()
	if len(all) != 1 {
		t.Fatalf("expected 1 pipeline, got %d", len(all))
	}

	removed, _ := db.RemovePipeline("review")
	if removed != 1 {
		t.Fatalf("expected 1 removed, got %d", removed)
	}
}

func TestSubscriptionCRUD(t *testing.T) {
	db := testDb(t)
	pl := makePipeline("review", []string{"draft", "done"})
	pid, _ := db.InsertPipeline(pl)

	subID, err := db.SubscribePipeline(pid, ":evidence")
	if err != nil {
		t.Fatal(err)
	}
	if subID <= 0 {
		t.Fatal("expected positive sub id")
	}

	subs, _ := db.ListPipelineSubscriptions(pid)
	if len(subs) != 1 || subs[0].Reference != ":evidence" {
		t.Fatalf("unexpected subs: %v", subs)
	}

	removed, _ := db.UnsubscribePipeline(pid, ":evidence")
	if removed != 1 {
		t.Fatalf("expected 1 removed, got %d", removed)
	}

	subs, _ = db.ListPipelineSubscriptions(pid)
	if len(subs) != 0 {
		t.Fatal("expected empty after unsubscribe")
	}
}

func TestMaterializeAndQuery(t *testing.T) {
	db := testDb(t)
	pl := makePipeline("review", []string{"draft", "done"})
	pid, _ := db.InsertPipeline(pl)
	subID, _ := db.SubscribePipeline(pid, ":evidence")

	if err := db.MaterializePipelineFile(pid, "abc123", subID); err != nil {
		t.Fatal(err)
	}

	pipelines, err := db.GetPipelinesForSHA256("abc123")
	if err != nil {
		t.Fatal(err)
	}
	if len(pipelines) != 1 || pipelines[0].Name != "review" {
		t.Fatalf("expected review, got %v", pipelines)
	}

	none, _ := db.GetPipelinesForSHA256("other")
	if len(none) != 0 {
		t.Fatal("expected empty for other hash")
	}
}

func TestMaterializeDedup(t *testing.T) {
	db := testDb(t)
	pl := makePipeline("review", []string{"draft", "done"})
	pid, _ := db.InsertPipeline(pl)
	subID, _ := db.SubscribePipeline(pid, ":evidence")

	db.MaterializePipelineFile(pid, "abc", subID)
	db.MaterializePipelineFile(pid, "abc", subID) // should not fail

	pipelines, _ := db.GetPipelinesForSHA256("abc")
	if len(pipelines) != 1 {
		t.Fatalf("expected 1 after dedup, got %d", len(pipelines))
	}
}

func TestUnsubscribeCascades(t *testing.T) {
	db := testDb(t)
	pl := makePipeline("review", []string{"draft", "done"})
	pid, _ := db.InsertPipeline(pl)
	subID, _ := db.SubscribePipeline(pid, ":evidence")

	db.MaterializePipelineFile(pid, "hash1", subID)
	db.MaterializePipelineFile(pid, "hash2", subID)

	db.UnsubscribePipeline(pid, ":evidence")

	pipelines, _ := db.GetPipelinesForSHA256("hash1")
	if len(pipelines) != 0 {
		t.Fatal("expected empty after unsubscribe cascade")
	}
}

func TestMultiplePipelinesForHash(t *testing.T) {
	db := testDb(t)
	p1 := makePipeline("review", []string{"draft", "done"})
	p2 := makePipeline("classification", []string{"unclassified", "classified"})
	pid1, _ := db.InsertPipeline(p1)
	pid2, _ := db.InsertPipeline(p2)

	sub1, _ := db.SubscribePipeline(pid1, ":evidence")
	sub2, _ := db.SubscribePipeline(pid2, "!classified")

	db.MaterializePipelineFile(pid1, "shared", sub1)
	db.MaterializePipelineFile(pid2, "shared", sub2)

	pipelines, _ := db.GetPipelinesForSHA256("shared")
	if len(pipelines) != 2 {
		t.Fatalf("expected 2 pipelines, got %d", len(pipelines))
	}
}

func TestSignCRUD(t *testing.T) {
	db := testDb(t)
	pl := makePipeline("review", []string{"draft", "done"})
	pid, _ := db.InsertPipeline(pl)

	f := &models.TrackedFile{SHA256: "abc123", Fingerprint: "[]", IngestedAt: "2025-01-01T00:00:00Z"}
	fid, _ := db.InsertFile(f)

	sign := &models.Sign{
		PipelineID: pid,
		FileID:     fid,
		FileHash:   "abc123",
		SignName:    "done",
		Signer:     "alice",
		SignedAt:    "2025-06-01T00:00:00Z",
	}
	sid, err := db.InsertSign(sign)
	if err != nil {
		t.Fatal(err)
	}
	if sid <= 0 {
		t.Fatal("expected positive sign id")
	}

	valid, _ := db.GetValidSignsForFilePipeline(fid, pid, "abc123")
	if len(valid) != 1 {
		t.Fatalf("expected 1 valid sign, got %d", len(valid))
	}

	stale, _ := db.GetValidSignsForFilePipeline(fid, pid, "different")
	if len(stale) != 0 {
		t.Fatal("expected no valid signs for wrong hash")
	}

	db.RevokeSign(sid, "2025-06-02T00:00:00Z")
	valid, _ = db.GetValidSignsForFilePipeline(fid, pid, "abc123")
	if len(valid) != 0 {
		t.Fatal("expected no valid signs after revoke")
	}
}
