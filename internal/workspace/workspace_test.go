package workspace

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func put(t *testing.T, root, path, body string) {
	t.Helper()
	target := filepath.Join(root, path)
	if err := os.MkdirAll(filepath.Dir(target), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target, []byte(body), 0644); err != nil {
		t.Fatal(err)
	}
}
func TestConfigManifestDiscoveryAndValidation(t *testing.T) {
	root := t.TempDir()
	put(t, root, "package.json", `{"name":"root","workspaces":["services/*","!services/ignored"]}`)
	put(t, root, "nanoom.config.json", `{"group":{"ci":{"tasks":["test"]}}}`)
	c, err := LoadConfig(root, "nanoom.config.json")
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(c.Workspace.Include, []string{"services/*"}) || !reflect.DeepEqual(c.Workspace.Exclude, []string{"services/ignored"}) || c.Affected.MaxFetchDepth != 2048 {
		t.Fatalf("wrong inferred config: %+v", c)
	}
	put(t, root, "pnpm-workspace.yaml", "packages:\n  - packages/**\n  - '!packages/ignored'\n")
	c, err = LoadConfig(root, "nanoom.config.json")
	if err != nil {
		t.Fatal(err)
	}
	if !c.Includes("packages/nested/web") || c.Includes("packages/ignored") || c.Includes("services/web") {
		t.Fatal("pnpm patterns not authoritative")
	}
	put(t, root, "nanoom.config.json", `{"group":{"ci":{"tasks":["test"]}},"workspace":{"include":["explicit/*"]}}`)
	c, err = LoadConfig(root, "nanoom.config.json")
	if err != nil || !c.Includes("explicit/web") || c.Includes("packages/web") {
		t.Fatalf("explicit pattern ignored: %v", err)
	}
	for _, body := range []string{`{"group":{"ci":{"tasks":["test","test"]}}}`, `{"group":{"ci":{"tasks":["test"],"unknown":true}}}`, `{"group":{"ci":{"tasks":["test"]}},"workspace":null}`, `{"group":{"ci":{"tasks":["test"]}},"affected":{"maxFetchDepth":null}}`, `{"group":{"ci":{"tasks":["test"],"rules":[{"name":"web","shard":[{"task":"build","shard":2}]}]}}}`, `{"group":{"ci":{"tasks":["test"]}},"checkout":{"always":["../escape"]}}`} {
		put(t, root, "nanoom.config.json", body)
		if _, err := LoadConfig(root, "nanoom.config.json"); err == nil {
			t.Fatalf("invalid config accepted: %s", body)
		}
	}
}
func TestWorkspaceClosureRootToolingAndSemver(t *testing.T) {
	root := t.TempDir()
	put(t, root, "nanoom.config.json", `{"group":{"ci":{"tasks":["test"]}}}`)
	put(t, root, "package.json", `{"name":"root","workspaces":["packages/*"],"devDependencies":{"tools":"workspace:*"}}`)
	for name, body := range map[string]string{"web": `{"name":"web","dependencies":{"ui":"workspace:*","registry":"^2.0.0"}}`, "ui": `{"name":"ui","dependencies":{"core":"^1.0.0"}}`, "core": `{"name":"core","version":"1.2.0"}`, "registry": `{"name":"registry","version":"1.0.0"}`, "tools": `{"name":"tools","dependencies":{"core":"workspace:*"}}`, "unrelated": `{"name":"unrelated"}`} {
		put(t, root, "packages/"+name+"/package.json", body)
	}
	put(t, root, ".yarnrc.yml", "yarnPath: .yarn/releases/yarn.cjs\nplugins:\n  - path: .yarn/plugins/focus.cjs\n")
	c, err := LoadConfig(root, "nanoom.config.json")
	if err != nil {
		t.Fatal(err)
	}
	w, err := Discover(c, root)
	if err != nil {
		t.Fatal(err)
	}
	if actual := w.Closure("web", root); !reflect.DeepEqual(actual, []string{"packages/core", "packages/ui", "packages/web"}) {
		t.Fatal(actual)
	}
	paths, err := w.InstallationPaths(root)
	if err != nil || !reflect.DeepEqual(paths, []string{".yarn/plugins", ".yarn/releases", "packages/core", "packages/tools"}) {
		t.Fatalf("root tools: %v %v", paths, err)
	}
	core, _ := w.Project("core")
	if !reflect.DeepEqual(core.Dependents, []string{"tools", "ui"}) {
		t.Fatal(core.Dependents)
	}
	put(t, root, ".yarnrc.yml", "yarnPath: ../outside/yarn.cjs\n")
	if _, err := w.InstallationPaths(root); err == nil {
		t.Fatal("escaping Yarn asset accepted")
	}
	put(t, root, "packages/unrelated/package.json", `{"name":"web"}`)
	if _, err := Discover(c, root); err == nil {
		t.Fatal("duplicate workspace accepted")
	}
}
func TestTopologicalOrderAndCycles(t *testing.T) {
	projects := []Project{{Name: "web", Dependencies: []string{"ui"}}, {Name: "ui", Dependencies: []string{"core"}}, {Name: "core"}, {Name: "cycle-a", Dependencies: []string{"cycle-b"}}, {Name: "cycle-b", Dependencies: []string{"cycle-a"}}}
	ordered := Topological(projects)
	names := []string{}
	for _, p := range ordered {
		names = append(names, p.Name)
	}
	if !reflect.DeepEqual(names, []string{"core", "ui", "web", "cycle-a", "cycle-b"}) {
		t.Fatal(names)
	}
}
