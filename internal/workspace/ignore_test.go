package workspace

import (
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"runtime"
	"testing"
)

func TestDiscoverIgnorePrecedenceAndNestedWorkingDirectory(t *testing.T) {
	root := t.TempDir()
	command := exec.Command("git", "init", "-q", root)
	if output, err := command.CombinedOutput(); err != nil {
		t.Fatalf("git init: %s %v", output, err)
	}
	t.Setenv("GIT_CONFIG_GLOBAL", filepath.Join(root, "no-global-config"))
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "config"))
	put(t, root, "config/git/ignore", "global/\n")
	put(t, root, ".git/info/exclude", "local/\n")
	put(t, root, ".gitignore", "ignored/\nrestored/\n")
	put(t, root, ".ignore", "!restored/\ncustom/\n")
	put(t, root, "nested/.gitignore", "inner/\n")
	for _, name := range []string{"visible", ".hidden", "ignored", "restored", "custom", "local", "global", "nested/inner", "nested/keep"} {
		put(t, root, name+"/package.json", `{"name":"`+name+`"}`)
	}
	config := Config{}
	config.Workspace.Include = []string{"**"}
	ws, err := Discover(config, root)
	if err != nil {
		t.Fatal(err)
	}
	names := []string{}
	for _, p := range ws.Projects {
		names = append(names, p.Name)
	}
	if !reflect.DeepEqual(names, []string{".hidden", "nested/keep", "restored", "visible"}) {
		t.Fatal(names)
	}
	ws, err = Discover(config, filepath.Join(root, "nested"))
	if err != nil || len(ws.Projects) != 1 || ws.Projects[0].Name != "nested/keep" {
		t.Fatalf("nested: %+v %v", ws, err)
	}
}

func TestDiscoverManifestSymlinkWithoutFollowingDirectorySymlink(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlinks require Windows developer mode; covered on Unix")
	}
	root := t.TempDir()
	put(t, root, "source/manifest.json", `{"name":"linked"}`)
	if err := os.MkdirAll(filepath.Join(root, "packages/app"), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink("../../source/manifest.json", filepath.Join(root, "packages/app/package.json")); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink("app", filepath.Join(root, "packages/alias")); err != nil {
		t.Fatal(err)
	}
	config := Config{}
	config.Workspace.Include = []string{"packages/*"}
	ws, err := Discover(config, root)
	if err != nil || len(ws.Projects) != 1 || ws.Projects[0].Name != "linked" {
		t.Fatalf("symlink: %+v %v", ws, err)
	}
}
