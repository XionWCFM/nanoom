package affected

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	w "github.com/XionWCFM/nanoom/internal/workspace"
)

func fixtureFile(t *testing.T, root, path, body string) {
	t.Helper()
	file := filepath.Join(root, path)
	if err := os.MkdirAll(filepath.Dir(file), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(file, []byte(body), 0644); err != nil {
		t.Fatal(err)
	}
}
func fixtureGit(t *testing.T, root string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", root}, args...)...)
	result, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %s %v", args, result, err)
	}
	return strings.TrimSpace(string(result))
}
func fixtureCommit(t *testing.T, root, message string) string {
	t.Helper()
	fixtureGit(t, root, "add", ".")
	fixtureGit(t, root, "-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false", "commit", "-qm", message)
	return fixtureGit(t, root, "rev-parse", "HEAD")
}
func TestAffectedActualGitDependencyStructureAndMissingManifest(t *testing.T) {
	root := t.TempDir()
	fixtureGit(t, root, "init", "-q")
	fixtureFile(t, root, "package.json", `{"private":true,"workspaces":["packages/*"]}`)
	fixtureFile(t, root, "nanoom.config.json", `{"group":{"ci":{"tasks":["test"]}}}`)
	fixtureFile(t, root, "packages/core/package.json", `{"name":"core","version":"1.0.0"}`)
	fixtureFile(t, root, "packages/app/package.json", `{"name":"app","dependencies":{"core":"workspace:*"}}`)
	fixtureFile(t, root, "packages/other/package.json", `{"name":"other"}`)
	base := fixtureCommit(t, root, "base")
	config, err := w.LoadConfig(root, "nanoom.config.json")
	if err != nil {
		t.Fatal(err)
	}
	var diagnostics bytes.Buffer
	result, err := Calculate(config, root, base, base, &diagnostics)
	if err != nil || result.HasChange || len(result.Group["ci"].Workspaces) != 0 {
		t.Fatalf("no change: %+v %v", result, err)
	}
	fixtureFile(t, root, "packages/core/source.js", "changed")
	head := fixtureCommit(t, root, "source")
	result, err = Calculate(config, root, base, head, &diagnostics)
	if err != nil {
		t.Fatal(err)
	}
	names := []string{}
	for _, item := range result.Group["ci"].Workspaces {
		names = append(names, item.Name)
	}
	if !reflect.DeepEqual(names, []string{"app", "core"}) {
		t.Fatal(names)
	}
	if result.Diagnostics.Reasons["core"].Kind != "direct" || result.Diagnostics.Reasons["app"].Kind != "transitiveDependent" {
		t.Fatal(result.Diagnostics)
	}
	fixtureGit(t, root, "mv", "packages/core", "packages/renamed")
	renamed := fixtureCommit(t, root, "rename")
	result, err = Calculate(config, root, head, renamed, &diagnostics)
	if err != nil || result.Group["ci"].AffectedWorkspaces != 3 {
		t.Fatalf("structure: %+v %v", result, err)
	}
	for _, reason := range result.Diagnostics.Reasons {
		if reason.Kind != "workspaceManifestStructure" {
			t.Fatal(reason)
		}
	}
	if err := os.Remove(filepath.Join(root, "packages/other/package.json")); err != nil {
		t.Fatal(err)
	}
	if _, err = Calculate(config, root, head, renamed, &diagnostics); err == nil || !strings.Contains(err.Error(), "missing 1 workspace package.json") {
		t.Fatalf("missing manifest not rejected: %v", err)
	}
}

func TestAffectedShallowCloneFetchesCommitHistory(t *testing.T) {
	origin := t.TempDir()
	fixtureGit(t, origin, "init", "-q")
	fixtureFile(t, origin, "package.json", `{"workspaces":["packages/*"]}`)
	fixtureFile(t, origin, "nanoom.config.json", `{"group":{"ci":{"tasks":["test"]}}}`)
	fixtureFile(t, origin, "packages/app/package.json", `{"name":"app"}`)
	base := fixtureCommit(t, origin, "base")
	fixtureFile(t, origin, "packages/app/source.js", "changed")
	head := fixtureCommit(t, origin, "changed")
	clone := filepath.Join(t.TempDir(), "clone")
	fixtureGit(t, origin, "clone", "-q", "--depth=1", "--no-local", origin, clone)
	config, err := w.LoadConfig(clone, "nanoom.config.json")
	if err != nil {
		t.Fatal(err)
	}
	var diagnostics bytes.Buffer
	result, err := Calculate(config, clone, base, head, &diagnostics)
	if err != nil || !result.HasChange || !strings.Contains(diagnostics.String(), "commit-only") {
		t.Fatalf("shallow: %+v %v %s", result, err, diagnostics.String())
	}
}
