package plan

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/XionWCFM/nanoom/internal/prediction"
)

func fixture() Plan {
	return Plan{Version: Version, Provenance: PlanProvenance{Repository: "owner/repo", Workflow: ".github/workflows/ci.yml", RunID: "123", ProducerAttempt: 1, PlanningJob: "affected", Base: strings.Repeat("a", 40), Head: strings.Repeat("b", 40)}, TaskRunner: "npm", PredictionReason: "cold", HasChange: true, Groups: map[string]PlanGroup{"ci": {Assignments: []PlanAssignment{{AssignmentID: "ci-1", Items: []PlanItem{{Group: "ci", Name: "app", Path: "packages/app", Task: "test"}}, CheckoutPaths: []string{"packages/app", "packages/shared"}, PredictionReason: "cold"}}}}, AssignmentCount: 1, ItemCount: 1}
}
func reference(p Plan, b []byte) PlanReference {
	name, _ := p.Provenance.ArtifactName()
	return PlanReference{Version: Version, ArtifactName: name, SHA256: prediction.HexDigest(b), Provenance: p.Provenance, Current: CurrentRun{Repository: p.Provenance.Repository, Workflow: p.Provenance.Workflow, RunID: p.Provenance.RunID, Attempt: p.Provenance.ProducerAttempt, Base: p.Provenance.Base, Head: p.Provenance.Head}}
}
func TestSelectPreservesAuthorityAndHasNoWritesOnInvalidPlan(t *testing.T) {
	dir := t.TempDir()
	p := fixture()
	bytes, _ := json.Marshal(p)
	r := reference(p, bytes)
	refBytes, _ := json.Marshal(r)
	planPath, refPath := filepath.Join(dir, "plan.json"), filepath.Join(dir, "reference.json")
	if err := os.WriteFile(planPath, bytes, 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(refPath, refBytes, 0644); err != nil {
		t.Fatal(err)
	}
	selected, err := Select(planPath, refPath, "ci", "ci-1", filepath.Join(dir, "selected"))
	if err != nil {
		t.Fatal(err)
	}
	if selected.PlanSHA256 != r.SHA256 || len(selected.Items) != 1 || selected.Items[0].Name != "app" || selected.CheckoutPaths[1] != "packages/shared" {
		t.Fatalf("wrong assignment: %+v", selected)
	}
	paths, err := os.ReadFile(filepath.Join(dir, "selected", "paths.txt"))
	if err != nil || string(paths) != "packages/app\npackages/shared\n" {
		t.Fatalf("wrong checkout paths: %s %v", paths, err)
	}
	for _, mutate := range []func(*Plan){func(p *Plan) { p.ItemCount = 2 }, func(p *Plan) { p.HasChange = false }, func(p *Plan) { p.TaskRunner = "" }, func(p *Plan) {
		a := p.Groups["ci"].Assignments
		a[0].CheckoutPaths = []string{"packages/shared", "packages/app"}
	}, func(p *Plan) { p.Groups["ci"].Assignments[0].Items[0].Path = "../escape" }, func(p *Plan) { p.Groups["ci"].Assignments[0].Items[0].Path = "C:/escape" }, func(p *Plan) { v := uint64(0); p.Groups["ci"].Assignments[0].Items[0].Shard = &v }, func(p *Plan) { v := "automatic"; p.Groups["ci"].Assignments[0].PreparationPredictionSource = &v }, func(p *Plan) {
		a := p.Groups["ci"]
		a.Assignments = append(a.Assignments, a.Assignments[0])
		p.Groups["ci"] = a
		p.AssignmentCount = 2
		p.ItemCount = 2
	}} {
		invalid := fixture()
		mutate(&invalid)
		bytes, _ := json.Marshal(invalid)
		r := reference(invalid, bytes)
		refBytes, _ := json.Marshal(r)
		if err := os.WriteFile(planPath, bytes, 0644); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(refPath, refBytes, 0644); err != nil {
			t.Fatal(err)
		}
		output := filepath.Join(dir, "denied")
		if _, err := Select(planPath, refPath, "ci", "ci-1", output); err == nil {
			t.Fatal("invalid Plan accepted")
		}
		if _, err := os.Stat(output); !os.IsNotExist(err) {
			t.Fatal("denied selection wrote output")
		}
	}
}
func TestReferenceDigestAndCurrentRunBindings(t *testing.T) {
	p := fixture()
	bytes, _ := json.Marshal(p)
	for _, mutate := range []func(*PlanReference){func(r *PlanReference) { r.SHA256 = strings.Repeat("0", 64) }, func(r *PlanReference) { r.Current.Head = strings.Repeat("c", 40) }, func(r *PlanReference) { r.Current.Attempt = 0 }, func(r *PlanReference) { r.Current.Workflow = "other" }, func(r *PlanReference) { r.ArtifactName = "other" }} {
		r := reference(p, bytes)
		mutate(&r)
		if err := r.Validate(p, bytes); err == nil {
			t.Fatal("invalid reference accepted")
		}
	}
	r := reference(p, bytes)
	r.Current.Attempt = 2
	if err := r.Validate(p, bytes); err != nil {
		t.Fatal("valid retry denied:", err)
	}
}
func TestSparseCheckoutEscapesAndNestedWorkingDirectory(t *testing.T) {
	directory, config := "project", "configs/custom.json"
	got := SparseCheckout([]string{"project/packages/a b", "project/packages/c[1]"}, &config, &directory)
	want := "/*\n!/*/\n/project/*\n!/project/*/\n/project/packages/a\\ b\n/project/packages/c\\[1\\]\n/project/configs/custom.json"
	if got != want {
		t.Fatalf("sparse patterns:\n%s", got)
	}
	if got := SparseCheckout([]string{"."}, nil, nil); got != "/*" {
		t.Fatal(got)
	}
}
