package cli

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"unicode"

	w "github.com/XionWCFM/nanoom/internal/workspace"
)

func isFile(path string) bool { s, err := os.Stat(path); return err == nil && s.Mode().IsRegular() }
func detectPackageManager(cwd, requested string) (string, error) {
	if requested != "" && requested != "auto" {
		switch requested {
		case "pnpm", "yarn", "npm":
			return requested, nil
		default:
			return "", fmt.Errorf("Package manager not found: %s", requested)
		}
	}
	b, err := os.ReadFile(filepath.Join(cwd, "package.json"))
	if err == nil {
		var raw map[string]json.RawMessage
		if err := json.Unmarshal(b, &raw); err != nil {
			return "", err
		}
		if data, ok := raw["packageManager"]; ok {
			var declaration string
			if err := json.Unmarshal(data, &declaration); err != nil || string(data) == "null" {
				return "", fmt.Errorf("package.json packageManager must be a string")
			}
			name, _, _ := strings.Cut(declaration, "@")
			switch name {
			case "pnpm", "yarn", "npm":
				return name, nil
			default:
				return "", fmt.Errorf("unsupported packageManager %q", declaration)
			}
		}
	} else if !os.IsNotExist(err) {
		return "", err
	}
	managers := []string{}
	if isFile(filepath.Join(cwd, "pnpm-lock.yaml")) {
		managers = append(managers, "pnpm")
	}
	if isFile(filepath.Join(cwd, "yarn.lock")) {
		managers = append(managers, "yarn")
	}
	if isFile(filepath.Join(cwd, "package-lock.json")) || isFile(filepath.Join(cwd, "npm-shrinkwrap.json")) {
		managers = append(managers, "npm")
	}
	if len(managers) > 1 {
		return "", fmt.Errorf("multiple package-manager lockfiles exist without a packageManager declaration; set packageManager or an explicit override")
	}
	if len(managers) == 0 {
		return "npm", nil
	}
	return managers[0], nil
}
func yarnBerry(cwd string) bool {
	config, _ := os.ReadFile(filepath.Join(cwd, ".yarnrc.yml"))
	if strings.Contains(string(config), "yarnPath:") {
		return true
	}
	m, err := w.ReadManifest(cwd)
	return err == nil && m.PackageManager != nil && strings.HasPrefix(*m.PackageManager, "yarn@") && !strings.HasPrefix(*m.PackageManager, "yarn@1.")
}
func install(a arguments, cwd, configPath string, out, diagnostics io.Writer) error {
	if err := a.allow("package-manager", "filter", "filter-file", "json"); err != nil {
		return err
	}
	if len(a.positionals) > 0 {
		return fmt.Errorf("unexpected install positional argument")
	}
	if a.has("filter") && a.has("filter-file") {
		return fmt.Errorf("--filter cannot be used with --filter-file")
	}
	config, err := w.LoadConfig(cwd, configPath)
	if err != nil {
		return err
	}
	filters := a.options["filter"]
	if a.has("filter-file") {
		path := a.get("filter-file")
		if !filepath.IsAbs(path) {
			path = filepath.Join(cwd, path)
		}
		b, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		var raw []any
		if err := json.Unmarshal(b, &raw); err != nil {
			return fmt.Errorf("--filter-file must contain a JSON string array")
		}
		if len(raw) == 0 {
			return fmt.Errorf("--filter-file must contain at least one workspace filter")
		}
		for _, v := range raw {
			value, ok := v.(string)
			if !ok || strings.TrimSpace(value) == "" || strings.IndexFunc(value, unicode.IsControl) >= 0 {
				return fmt.Errorf("--filter-file entries must be non-empty strings without control characters")
			}
			filters = append(filters, value)
		}
	}
	filters = w.Unique(filters)
	pm, err := detectPackageManager(cwd, a.get("package-manager"))
	if err != nil {
		return err
	}
	fmt.Fprintf(diagnostics, "◆ nanoom install\n  Inputs: package manager=%s, cwd=%s\n", pm, cwd)
	command := []string{}
	root := "."
	if len(filters) > 0 {
		switch pm {
		case "yarn":
			if !yarnBerry(cwd) {
				return fmt.Errorf("Yarn Classic focused install is unsupported; use Yarn Berry, pnpm, or npm")
			}
			m, err := w.ReadManifest(cwd)
			if err != nil {
				return err
			}
			if m.Name == nil {
				return fmt.Errorf("Root package.json must define a name for Yarn focus")
			}
			root = *m.Name
			command = append([]string{"workspaces", "focus", root}, filters...)
		case "pnpm", "npm":
			workspace, err := w.Discover(config, cwd)
			if err != nil {
				return err
			}
			paths, err := workspace.InstallationPaths(cwd)
			if err != nil {
				return err
			}
			installFilters := append([]string{}, filters...)
			if pm == "npm" {
				for _, filter := range filters {
					if _, ok := workspace.Project(filter); !ok {
						return fmt.Errorf("Workspace not found: %s", filter)
					}
					paths = append(paths, workspace.Closure(filter, cwd)...)
				}
			}
			if pm == "pnpm" {
				command = []string{"install", "--frozen-lockfile", "--prod=false", "--filter", "."}
			} else {
				command = []string{"ci", "--include-workspace-root", "--include=dev"}
			}
			set := map[string]bool{}
			for _, path := range paths {
				set[path] = true
			}
			for _, project := range workspace.Projects {
				path, _ := filepath.Rel(cwd, project.Path)
				if set[filepath.ToSlash(path)] {
					if pm == "pnpm" {
						installFilters = append(installFilters, project.Name)
					} else {
						command = append(command, "--workspace", project.Name)
					}
				}
			}
			if pm == "pnpm" {
				for _, filter := range w.Unique(installFilters) {
					command = append(command, "--filter", filter+"...")
				}
			}
		}
	} else {
		command = []string{"install"}
		switch pm {
		case "pnpm":
			if isFile(filepath.Join(cwd, "pnpm-lock.yaml")) {
				command = append(command, "--frozen-lockfile")
			}
		case "yarn":
			if isFile(filepath.Join(cwd, "yarn.lock")) {
				if yarnBerry(cwd) {
					command = append(command, "--immutable")
				} else {
					command = append(command, "--frozen-lockfile")
				}
			}
		case "npm":
			if isFile(filepath.Join(cwd, "package-lock.json")) || isFile(filepath.Join(cwd, "npm-shrinkwrap.json")) {
				command = []string{"ci"}
			}
		}
	}
	fmt.Fprintf(diagnostics, "Executing install command (cwd=%s): %s\n", cwd, displayCommand(pm, command))
	if err := executeTool(pm, command, cwd, nil, a.has("json"), out, diagnostics); err != nil {
		return fmt.Errorf("Command failed: %s (exit: %w)", displayCommand(pm, command), err)
	}
	if a.has("json") {
		report := map[string]any{"status": "success", "packageManager": pm, "cwd": cwd, "scope": map[string]any{"root": true}, "reason": "installed the monorepo root from its lockfile"}
		if len(filters) > 0 {
			report["command"] = displayCommand(pm, command)
			report["scope"] = map[string]any{"root": root, "workspaces": filters, "dependencyClosure": true, "devDependencies": true}
			report["reason"] = "focused install includes root tooling, the selected workspaces, and their dependency closure"
		}
		return json.NewEncoder(out).Encode(report)
	}
	return nil
}
