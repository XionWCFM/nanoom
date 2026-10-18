package cli

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	p "github.com/XionWCFM/nanoom/internal/prediction"
)

func run(t *testing.T, args ...string) (int, string, string) {
	t.Helper()
	var out, err bytes.Buffer
	code := Main(args, &out, &err, "0.9.0")
	return code, out.String(), err.String()
}
func TestStatusObservableContract(t *testing.T) {
	cases := []struct {
		results string
		code    int
		overall string
	}{{"build=success,test=success", 0, "success"}, {"build=success,test=skipped", 0, "success"}, {"build=success,test=cancelled", 1, "failure"}, {"build=success,test=failure", 1, "failure"}}
	for _, c := range cases {
		code, text, _ := run(t, "status", "build,test", "--results", c.results, "--json")
		if code != c.code {
			t.Fatalf("%s: %d %s", c.results, code, text)
		}
		var report struct {
			Overall string                          `json:"overall"`
			Jobs    []struct{ Name, Status string } `json:"jobs"`
		}
		if err := json.Unmarshal([]byte(text), &report); err != nil {
			t.Fatal(err)
		}
		if report.Overall != c.overall || len(report.Jobs) != 2 || report.Jobs[1].Status != strings.TrimPrefix(c.results, "build=success,test=") {
			t.Fatal(text)
		}
	}
	for _, results := range []string{"build=success,test=success,test=failure", "build=success,test=failure,test=success", "build=success", "build=success,test=queued"} {
		code, text, _ := run(t, "status", "build,test", "--results", results, "--json")
		if code != 1 || !strings.Contains(text, `"status":"failure"`) {
			t.Fatalf("invalid status accepted: %s %s", results, text)
		}
	}
}
func TestVersionAndArgumentErrors(t *testing.T) {
	code, out, stderr := run(t, "version", "--json")
	if code != 0 || stderr != "" || out != "{\"name\":\"nanoom\",\"version\":\"0.9.0\"}\n" {
		t.Fatalf("version: %d %s %s", code, out, stderr)
	}
	for _, args := range [][]string{{"cache-key", "--runner", "npm"}, {"status", "test", "--results"}, {"version", "--unknown", "x"}, {"version", "--json=false"}, {"version", "--json", "--json"}} {
		code, _, _ := run(t, args...)
		if code != 1 {
			t.Fatalf("invalid args accepted: %v", args)
		}
	}
}
func TestCacheKeyTracksAllInputs(t *testing.T) {
	dir := t.TempDir()
	args := []string{"cache-key", "--cwd", dir, "--runner", "npm", "--task", "test", "--json"}
	code, text, _ := run(t, args...)
	if code != 0 {
		t.Fatal(text)
	}
	var baseline map[string]any
	if err := json.Unmarshal([]byte(text), &baseline); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"nanoom.config.json", "package.json", "pnpm-workspace.yaml", ".npmrc", ".yarnrc.yml", "pnpm-lock.yaml", "yarn.lock", "package-lock.json", "npm-shrinkwrap.json"} {
		path := filepath.Join(dir, name)
		if err := os.WriteFile(path, []byte("before"), 0644); err != nil {
			t.Fatal(err)
		}
		code, text, _ := run(t, args...)
		if code != 0 {
			t.Fatal(text)
		}
		var changed struct {
			Key         string   `json:"key"`
			HashedFiles []string `json:"hashedFiles"`
		}
		if err := json.Unmarshal([]byte(text), &changed); err != nil {
			t.Fatal(err)
		}
		if changed.Key == baseline["key"] || len(changed.HashedFiles) != 1 || changed.HashedFiles[0] != name {
			t.Fatal(text)
		}
		if err := os.WriteFile(path, []byte("after"), 0644); err != nil {
			t.Fatal(err)
		}
		_, after, _ := run(t, args...)
		if after == text {
			t.Fatal("contents did not change key:", name)
		}
		if err := os.Remove(path); err != nil {
			t.Fatal(err)
		}
		_, reset, _ := run(t, args...)
		var resetReport map[string]any
		if err := json.Unmarshal([]byte(reset), &resetReport); err != nil {
			t.Fatal(err)
		}
		if resetReport["key"] != baseline["key"] {
			t.Fatal("removal did not restore key")
		}
	}
	if err := os.Mkdir(filepath.Join(dir, "yarn.lock"), 0755); err != nil {
		t.Fatal(err)
	}
	if code, _, _ := run(t, args...); code != 1 {
		t.Fatal("read error was ignored")
	}
}
func TestHistoryCLIProducesRustCompatibleArtifactsAndReplay(t *testing.T) {
	bytes, err := os.ReadFile("../prediction/testdata/rust-v3.json")
	if err != nil {
		t.Fatal(err)
	}
	var corpus struct {
		Scenarios []struct {
			Now          uint64                  `json:"now"`
			Measurements []p.MeasurementArtifact `json:"measurements"`
			Model        p.ModelState            `json:"model"`
		} `json:"scenarios"`
	}
	if err := json.Unmarshal(bytes, &corpus); err != nil {
		t.Fatal(err)
	}
	for _, index := range []int{0, 6} {
		c := corpus.Scenarios[index]
		dir := t.TempDir()
		input := filepath.Join(dir, "measurement.json")
		measurement, _ := p.CanonicalBytes(c.Measurements[0])
		if err := os.WriteFile(input, measurement, 0644); err != nil {
			t.Fatal(err)
		}
		args := []string{"history", "--input", input, "--model-output", filepath.Join(dir, "model.json"), "--prediction-output", filepath.Join(dir, "prediction.json"), "--model-artifact-name", "new-model", "--run-id", c.Measurements[0].RunID, "--run-attempt", "1", "--now-ms", strconv.FormatUint(c.Now, 10), "--batch-output-dir", filepath.Join(dir, "batches")}
		if index == 6 {
			second, _ := p.CanonicalBytes(c.Measurements[1])
			path := filepath.Join(dir, "second.json")
			if err := os.WriteFile(path, second, 0644); err != nil {
				t.Fatal(err)
			}
			args = append(args, "--input", path)
		}
		code, text, stderr := run(t, args...)
		if code != 0 {
			t.Fatalf("history failed: %s %s", text, stderr)
		}
		var report map[string]any
		if err := json.Unmarshal([]byte(text), &report); err != nil {
			t.Fatal(err)
		}
		if report["status"] != "success" || report["appliedBatchCount"] != float64(1) || report["emittedBatchCount"] != float64(1) {
			t.Fatal(text)
		}
		var model p.ModelStateBundle
		if err := p.ReadArtifact(filepath.Join(dir, "model.json"), 16*1024*1024, &model); err != nil {
			t.Fatal(err)
		}
		actual, _ := p.CanonicalBytes(model.States[0])
		expected, _ := p.CanonicalBytes(c.Model)
		if string(actual) != string(expected) {
			t.Fatal("CLI model differs from Rust")
		}
		args = append(args, "--previous-model", filepath.Join(dir, "model.json"), "--previous-prediction", filepath.Join(dir, "prediction.json"), "--previous-model-name", "new-model")
		code, text, stderr = run(t, args...)
		if code != 0 {
			t.Fatalf("replay failed: %s %s", text, stderr)
		}
		if err := json.Unmarshal([]byte(text), &report); err != nil {
			t.Fatal(err)
		}
		if report["duplicateBatchCount"] != float64(1) || report["appliedBatchCount"] != float64(0) {
			t.Fatal(text)
		}
		replay, _ := os.ReadFile(filepath.Join(dir, "model.json"))
		original, _ := p.CanonicalBytes(model)
		if string(replay) != string(original) {
			t.Fatal("retry changed stored model bytes")
		}
	}
}
