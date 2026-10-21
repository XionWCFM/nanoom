package cli

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func cliFile(t *testing.T, root, path, body string) {
	t.Helper()
	file := filepath.Join(root, path)
	if err := os.MkdirAll(filepath.Dir(file), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(file, []byte(body), 0644); err != nil {
		t.Fatal(err)
	}
}
func TestRunRealWorkspacesFailurePendingAndShardEnvironment(t *testing.T) {
	if _, err := exec.LookPath("npm"); err != nil {
		t.Fatal("npm is required for the user-path test")
	}
	root := t.TempDir()
	cliFile(t, root, "package.json", `{"private":true,"packageManager":"npm@11.16.0","workspaces":["packages/*"]}`)
	cliFile(t, root, "nanoom.config.json", `{"group":{"ci":{"tasks":["test"],"rules":[{"name":"a","shard":[{"task":"test","shard":4}]}]}}}`)
	cliFile(t, root, "task.cjs", `const fs=require('node:fs'),path=require('node:path');fs.appendFileSync(process.env.NANOOM_TASK_LOG,JSON.stringify({name:path.basename(process.cwd()),shard:process.env.NANOOM_SHARD_INDEX||null,total:process.env.NANOOM_SHARD_TOTAL||null})+'\n');console.log('::group::child');console.log('actual task output');console.log('::endgroup::');if(path.basename(process.cwd())==='b')process.exit(7)`)
	for name, body := range map[string]string{
		"a": `{"name":"a","scripts":{"test":"node ../../task.cjs"}}`,
		"b": `{"name":"b","dependencies":{"a":"workspace:*"},"scripts":{"test":"node ../../task.cjs"}}`,
		"c": `{"name":"c","dependencies":{"b":"workspace:*"},"scripts":{"test":"node ../../task.cjs"}}`,
	} {
		cliFile(t, root, "packages/"+name+"/package.json", body)
	}
	log := filepath.Join(root, "task.log")
	t.Setenv("NANOOM_TASK_LOG", log)
	args := []string{"run", "ci", "test", "--all", "--cwd", root, "--json"}
	code, text, stderr := run(t, args...)
	var report struct {
		Status                     string
		Completed, Failed, Pending []string
		Executions                 []struct {
			Workspace   string
			StartedAtMs uint64
		}
	}
	if err := json.Unmarshal([]byte(text), &report); err != nil {
		t.Fatalf("single result JSON: %v %s", err, text)
	}
	if code != 1 || report.Status != "failure" || !reflect.DeepEqual(report.Completed, []string{"a"}) || !reflect.DeepEqual(report.Failed, []string{"b"}) || !reflect.DeepEqual(report.Pending, []string{"c"}) {
		t.Fatalf("result: %d %s %s", code, text, stderr)
	}
	if len(report.Executions) != 1 || report.Executions[0].Workspace != "a" || report.Executions[0].StartedAtMs == 0 {
		t.Fatal(report)
	}
	if !strings.Contains(stderr, "actual task output") || strings.Contains(stderr, "::group::") || strings.Contains(stderr, "::endgroup::") {
		t.Fatal(stderr)
	}
	content, err := os.ReadFile(log)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Count(string(content), "\n") != 2 || strings.Contains(string(content), `"name":"c"`) {
		t.Fatal(string(content))
	}
	code, text, _ = run(t, append(args, "--continue-on-error")...)
	if err := json.Unmarshal([]byte(text), &report); err != nil {
		t.Fatal(err)
	}
	if code != 1 || !reflect.DeepEqual(report.Completed, []string{"a", "c"}) || len(report.Pending) != 0 {
		t.Fatalf("continue: %d %s", code, text)
	}
	code, text, stderr = run(t, append(args, "--filter", "a", "--shard", "2", "--total-shards", "4")...)
	if code != 0 {
		t.Fatalf("shard: %s %s", text, stderr)
	}
	content, err = os.ReadFile(log)
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimSpace(string(content)), "\n")
	var last struct{ Name, Shard, Total string }
	if err := json.Unmarshal([]byte(lines[len(lines)-1]), &last); err != nil {
		t.Fatal(err)
	}
	if last.Name != "a" || last.Shard != "2" || last.Total != "4" {
		t.Fatal(last)
	}
}
