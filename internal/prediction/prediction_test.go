package prediction

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func TestRustCompatibility(t *testing.T) {
	b, err := os.ReadFile("testdata/rust-v3.json")
	if err != nil {
		t.Fatal(err)
	}
	var corpus struct {
		Scope              Scope           `json:"scope"`
		ScopeID            string          `json:"scopeId"`
		PullRequestScope   Scope           `json:"pullRequestScope"`
		PullRequestScopeID string          `json:"pullRequestScopeId"`
		Strings            json.RawMessage `json:"strings"`
		StringsCanonical   string          `json:"stringsCanonical"`
		Scenarios          []struct {
			Now           uint64                `json:"now"`
			Prior         *ModelState           `json:"prior"`
			Measurements  []MeasurementArtifact `json:"measurements"`
			Batch         ObservationBatch      `json:"batch"`
			BatchDigest   string                `json:"batchDigest"`
			Model         ModelState            `json:"model"`
			Prediction    PredictionTable       `json:"prediction"`
			PrunedBuckets uint64                `json:"prunedBuckets"`
		} `json:"scenarios"`
	}
	if err := Decode(b, &corpus); err != nil {
		t.Fatal(err)
	}
	for _, s := range []struct {
		scope Scope
		id    string
	}{{corpus.Scope, corpus.ScopeID}, {corpus.PullRequestScope, corpus.PullRequestScopeID}} {
		got, err := s.scope.ID()
		if err != nil || got != s.id {
			t.Fatalf("scope id %s != %s: %v", got, s.id, err)
		}
	}
	canonical, err := CanonicalBytes(corpus.Strings)
	if err != nil || string(canonical) != corpus.StringsCanonical {
		t.Fatalf("string encoding differs from Rust: %s (%v)", canonical, err)
	}
	for i, c := range corpus.Scenarios {
		t.Run(string(rune('A'+i)), func(t *testing.T) {
			if c.Prior != nil {
				if err := c.Prior.Validate(); err != nil {
					t.Fatal("Rust model rejected:", err)
				}
			}
			batch, err := CompileMeasurements(c.Batch.Scope, c.Batch.RunID, c.Batch.RunAttempt, c.Batch.ProducedAtMs, c.Measurements)
			if err != nil {
				t.Fatal(err)
			}
			assertCanonical(t, "batch", batch, c.Batch)
			digest, err := Digest(batch)
			if err != nil || digest != c.BatchDigest {
				t.Fatalf("batch digest mismatch %s != %s: %v", digest, c.BatchDigest, err)
			}
			before, _ := CanonicalBytes(c.Prior)
			model, outcome, err := ApplyBatch(c.Prior, batch, c.Now)
			if err != nil {
				t.Fatal(err)
			}
			assertCanonical(t, "model", model, c.Model)
			after, _ := CanonicalBytes(c.Prior)
			if string(before) != string(after) {
				t.Fatal("apply mutated prior")
			}
			if outcome.Duplicate || outcome.PrunedBuckets != c.PrunedBuckets {
				t.Fatalf("wrong apply outcome: %+v", outcome)
			}
			table, err := ProjectPredictions(model, c.Now)
			if err != nil {
				t.Fatal(err)
			}
			assertCanonical(t, "prediction", table, c.Prediction)
			replay, outcome, err := ApplyBatch(&model, batch, c.Now)
			if err != nil || !outcome.Duplicate {
				t.Fatalf("retry failed: %+v %v", outcome, err)
			}
			assertCanonical(t, "replay", replay, model)
			altered := batch
			altered.Aggregates = append([]AggregateObservation{}, batch.Aggregates...)
			if len(altered.Aggregates) > 0 {
				altered.Aggregates[0].TotalDurationMs++
			} else {
				children := append([]EnvironmentBatch{}, batch.EnvironmentBatches...)
				child := *children[0].Batch
				child.Aggregates = append([]AggregateObservation{}, child.Aggregates...)
				child.Aggregates[0].TotalDurationMs++
				children[0].Batch = &child
				altered.EnvironmentBatches = children
			}
			if _, _, err := ApplyBatch(&model, altered, c.Now); err == nil {
				t.Fatal("conflicting replay accepted")
			}
			assertCanonical(t, "model after denied conflict", model, c.Model)
		})
	}
}
func assertCanonical(t *testing.T, label string, a, b any) {
	t.Helper()
	got, err := CanonicalBytes(a)
	if err != nil {
		t.Fatal(err)
	}
	want, err := CanonicalBytes(b)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != string(want) {
		t.Fatalf("%s differs\ngot: %s\nwant:%s", label, got, want)
	}
}
func TestStrictInputBoundaries(t *testing.T) {
	valid := `{"repositoryKey":"github-12345","workflowPath":".github/workflows/ci.yml","ref":{"kind":"push","ref":"refs/heads/main"},"group":"ci","taskRunner":"yarn","timingEnvironment":"linux"}`
	cases := []string{strings.Replace(valid, `"group":"ci"`, `"group":"ci","group":"other"`, 1), strings.Replace(valid, `"group":"ci",`, ``, 1), strings.Replace(valid, `"group":"ci"`, `"group":null`, 1), strings.Replace(valid, `"group":"ci"`, `"group":"ci","unknown":1`, 1), strings.Replace(valid, `"kind":"push"`, `"kind":"push","number":42`, 1), valid + `{}`, string([]byte{0xff})}
	for _, input := range cases {
		var scope Scope
		if err := Decode([]byte(input), &scope); err == nil {
			t.Fatalf("invalid input accepted: %s", input)
		}
	}
	var scope Scope
	if err := Decode([]byte(valid), &scope); err != nil {
		t.Fatal(err)
	}
	for _, input := range []string{`["` + strings.Repeat("a", 64) + `",1,1,1,2,3]`, `["a",1,1,1]`, `[null,1,1,1,2]`} {
		var row PredictionRow
		if err := Decode([]byte(input), &row); err == nil {
			t.Fatalf("invalid tuple accepted: %s", input)
		}
	}
}
func TestPreparationKeyCompatibility(t *testing.T) {
	scope := Scope{RepositoryKey: "github-12345", WorkflowPath: ".github/workflows/ci.yml", Ref: ScopeRef{Kind: "push", Ref: "refs/heads/main"}, Group: "ci", TaskRunner: "yarn", TimingEnvironment: "linux-x64-node24"}
	row := PreparationObservation{ExecutionID: "prep:1", ObservedAtMs: 100_000, PackageManager: "pnpm", PackageManagerVersion: "10.0.0", InstallMode: "focused", LockfileDigest: strings.Repeat("a", 64), CheckoutDigest: strings.Repeat("b", 64), WorkspaceSetDigest: strings.Repeat("c", 64), DurationMs: 25_000}
	keys, err := preparationKeys(scope, row)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"33b32e3cced63c6a6145a6cf8b555960a5ff113a9f9bed03a27222ed2f79e609", "752cb84cd99f4670014a68b5b3c61cc5e700ba86e8ff564a6191605ed44f6b7e"}
	for i, key := range keys {
		if key != want[i] {
			t.Fatalf("key differs from Rust: %s != %s", key, want[i])
		}
	}
}

func TestJSONUnicodeAndDepthBoundaries(t *testing.T) {
	for _, input := range []string{`"\ud800"`, `"\udc00"`, `"\ud800\u0041"`, strings.Repeat("[", 129) + "0" + strings.Repeat("]", 129)} {
		var value any
		if err := Decode([]byte(input), &value); err == nil {
			t.Fatalf("invalid JSON accepted: %s", input)
		}
	}
	for _, input := range []string{`"\ud83d\ude00"`, `"\\ud800"`, `"한글"`} {
		var value any
		if err := Decode([]byte(input), &value); err != nil {
			t.Fatalf("valid JSON rejected: %s %v", input, err)
		}
	}
}
