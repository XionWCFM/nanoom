package workspace

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"unicode"

	"github.com/Masterminds/semver/v3"
	"go.yaml.in/yaml/v3"
)

type Project struct {
	Name, Path, Version string
	Dependencies        []string
	Specs               map[string]string
	Dependents          []string
}
type Workspace struct {
	Projects []Project
	byName   map[string]int
}
type Manifest struct {
	Name                 *string           `json:"name"`
	Version              *string           `json:"version"`
	Dependencies         map[string]string `json:"dependencies"`
	DevDependencies      map[string]string `json:"devDependencies"`
	PeerDependencies     map[string]string `json:"peerDependencies"`
	OptionalDependencies map[string]string `json:"optionalDependencies"`
	Scripts              map[string]string `json:"scripts"`
	PackageManager       *string           `json:"packageManager"`
}

func ReadManifest(path string) (Manifest, error) {
	var m Manifest
	b, err := os.ReadFile(filepath.Join(path, "package.json"))
	if err != nil {
		return m, err
	}
	err = json.Unmarshal(b, &m)
	return m, err
}
func readProject(path string) (Project, error) {
	m, err := ReadManifest(path)
	if err != nil {
		return Project{}, err
	}
	p := Project{Name: filepath.Base(path), Path: path, Specs: map[string]string{}}
	if m.Name != nil {
		p.Name = *m.Name
	}
	if m.Version != nil {
		p.Version = *m.Version
	}
	for _, specs := range []map[string]string{m.Dependencies, m.DevDependencies, m.PeerDependencies, m.OptionalDependencies} {
		for name, spec := range specs {
			p.Specs[name] = spec
		}
	}
	for name := range p.Specs {
		p.Dependencies = append(p.Dependencies, name)
	}
	sort.Strings(p.Dependencies)
	return p, nil
}
func Discover(config Config, cwd string) (Workspace, error) {
	root, err := filepath.Abs(cwd)
	if err != nil {
		return Workspace{}, err
	}
	candidates := []string{}
	ignoreBase, initial, err := initialIgnore(root)
	if err != nil {
		return Workspace{}, err
	}
	rulesByDirectory := map[string]ignoreRules{}
	err = filepath.WalkDir(root, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rules := initial
		if path != root {
			rules = rulesByDirectory[filepath.Dir(path)]
		}
		if entry.IsDir() {
			if path != root && (entry.Name() == "node_modules" || entry.Name() == ".git" || rules.excludes(ignoreBase, path, true)) {
				return filepath.SkipDir
			}
			next, err := rules.extend(ignoreBase, path)
			if err != nil {
				return err
			}
			rulesByDirectory[path] = next
			return nil
		}
		if rules.excludes(ignoreBase, path, false) {
			return nil
		}
		if entry.Name() != "package.json" {
			return nil
		}
		parent := filepath.Dir(path)
		relative, err := filepath.Rel(root, parent)
		if err != nil {
			return err
		}
		if config.Includes(relative) {
			candidates = append(candidates, parent)
		}
		return nil
	})
	if err != nil {
		return Workspace{}, err
	}

	sort.Strings(candidates)
	w := Workspace{Projects: []Project{}, byName: map[string]int{}}
	for _, path := range candidates {
		project, err := readProject(path)
		if err != nil {
			return w, err
		}
		if _, exists := w.byName[project.Name]; exists {
			return w, fmt.Errorf("duplicate workspace name %q", project.Name)
		}
		w.byName[project.Name] = len(w.Projects)
		w.Projects = append(w.Projects, project)
	}
	for i, p := range w.Projects {
		for _, dep := range p.Dependencies {
			if index, ok := w.byName[dep]; ok && index != i && internalLink(p.Specs[dep], w.Projects[index].Version) {
				w.Projects[index].Dependents = append(w.Projects[index].Dependents, p.Name)
			}
		}
	}
	return w, nil
}
func internalLink(spec, version string) bool {
	if strings.HasPrefix(spec, "workspace:") || strings.HasPrefix(spec, "link:") || strings.HasPrefix(spec, "file:") || version == "" {
		return true
	}
	if spec == "" || spec == "*" || spec == "x" || spec == "X" || spec == "latest" {
		return true
	}
	constraint, err := semver.NewConstraint(spec)
	if err != nil {
		return false
	}
	v, err := semver.NewVersion(version)
	return err == nil && constraint.Check(v)
}
func (w Workspace) Project(name string) (Project, bool) {
	i, ok := w.byName[name]
	if !ok {
		return Project{}, false
	}
	return w.Projects[i], true
}
func Unique(values []string) []string {
	seen := map[string]bool{}
	out := []string{}
	for _, v := range values {
		if !seen[v] {
			out = append(out, v)
			seen[v] = true
		}
	}
	return out
}
func (w Workspace) Closure(name, cwd string) []string {
	pending := []string{name}
	seen := map[string]bool{}
	paths := []string{}
	for len(pending) > 0 {
		current := pending[len(pending)-1]
		pending = pending[:len(pending)-1]
		if seen[current] {
			continue
		}
		seen[current] = true
		p, ok := w.Project(current)
		if !ok {
			continue
		}
		relative, _ := filepath.Rel(cwd, p.Path)
		paths = append(paths, filepath.ToSlash(relative))
		for _, dep := range p.Dependencies {
			if local, ok := w.Project(dep); ok && internalLink(p.Specs[dep], local.Version) {
				pending = append(pending, dep)
			}
		}
	}
	sort.Strings(paths)
	return Unique(paths)
}
func (w Workspace) InstallationPaths(cwd string) ([]string, error) {
	paths := []string{}
	if _, err := os.Stat(filepath.Join(cwd, "package.json")); err == nil {
		root, err := readProject(cwd)
		if err != nil {
			return nil, err
		}
		for name, spec := range root.Specs {
			if local, ok := w.Project(name); ok && internalLink(spec, local.Version) {
				paths = append(paths, w.Closure(name, cwd)...)
			}
		}
	}
	b, err := os.ReadFile(filepath.Join(cwd, ".yarnrc.yml"))
	if err == nil {
		var config struct {
			YarnPath *string `yaml:"yarnPath"`
			Plugins  []struct {
				Path string `yaml:"path"`
			} `yaml:"plugins"`
		}
		if err := yaml.Unmarshal(b, &config); err != nil {
			return nil, err
		}
		assets := []string{}
		if config.YarnPath != nil {
			assets = append(assets, *config.YarnPath)
		}
		for _, plugin := range config.Plugins {
			assets = append(assets, plugin.Path)
		}
		for _, asset := range assets {
			asset = strings.TrimPrefix(asset, "${PROJECT_CWD}/")
			if asset == "" || strings.HasPrefix(asset, "/") || strings.ContainsAny(asset, "\\:") || strings.IndexFunc(asset, unicode.IsControl) >= 0 || filepath.IsAbs(asset) || hasParent(asset) {
				return nil, fmt.Errorf(".yarnrc.yml asset %q must be a repository-relative path", asset)
			}
			parent := filepath.ToSlash(filepath.Dir(filepath.Clean(asset)))
			if parent != "." {
				paths = append(paths, parent)
			}
		}
	} else if !os.IsNotExist(err) {
		return nil, err
	}
	sort.Strings(paths)
	return Unique(paths), nil
}
func Topological(projects []Project) []Project {
	byName := map[string]int{}
	for i, p := range projects {
		byName[p.Name] = i
	}
	degree := make([]int, len(projects))
	edges := make([][]int, len(projects))
	for i, p := range projects {
		for _, name := range p.Dependencies {
			if dep, ok := byName[name]; ok && i != dep {
				degree[i]++
				edges[dep] = append(edges[dep], i)
			}
		}
	}
	queue := []int{}
	for i, d := range degree {
		if d == 0 {
			queue = append(queue, i)
		}
	}
	sort.Slice(queue, func(i, j int) bool { return projects[queue[i]].Name < projects[queue[j]].Name })
	order := []Project{}
	seen := map[int]bool{}
	for len(queue) > 0 {
		index := queue[0]
		queue = queue[1:]
		order = append(order, projects[index])
		seen[index] = true
		next := []int{}
		for _, dependent := range edges[index] {
			degree[dependent]--
			if degree[dependent] == 0 {
				next = append(next, dependent)
			}
		}
		sort.Slice(next, func(i, j int) bool { return projects[next[i]].Name < projects[next[j]].Name })
		queue = append(queue, next...)
	}
	for i, p := range projects {
		if !seen[i] {
			order = append(order, p)
		}
	}
	return order
}
