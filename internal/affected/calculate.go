package affected

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"

	w "github.com/XionWCFM/nanoom/internal/workspace"
	"github.com/gobwas/glob"
)

type Output struct {
	Group       map[string]GroupOutput `json:"group"`
	HasChange   bool                   `json:"has_change"`
	Diagnostics *Diagnostics           `json:"diagnostics,omitempty"`
}
type Diagnostics struct {
	Comparison   Comparison        `json:"comparison"`
	ChangedFiles []string          `json:"changedFiles"`
	Reasons      map[string]Reason `json:"reasons"`
}
type Comparison struct {
	Mode          string `json:"mode"`
	RequestedBase string `json:"requestedBase"`
	RequestedHead string `json:"requestedHead"`
	BaseCommit    string `json:"baseCommit"`
	HeadCommit    string `json:"headCommit"`
}
type Reason struct {
	Kind           string   `json:"kind"`
	ChangedFiles   []string `json:"changedFiles,omitempty"`
	DependencyPath []string `json:"dependencyPath,omitempty"`
}
type Entry struct {
	Group         string   `json:"group"`
	Name          string   `json:"name"`
	Path          string   `json:"path"`
	Task          string   `json:"task"`
	Shard         *uint64  `json:"shard,omitempty"`
	TotalShards   *uint64  `json:"totalShards,omitempty"`
	CheckoutPaths []string `json:"-"`
}
type GroupOutput struct {
	Label              string        `json:"label"`
	Workspaces         []Entry       `json:"workspaces"`
	TotalWorkspaces    uint64        `json:"totalWorkspaces"`
	AffectedWorkspaces uint64        `json:"affectedWorkspaces"`
	AffectedPercent    float64       `json:"affectedPercent"`
	Distribution       *SelectedTier `json:"distribution,omitempty"`
	RunnerLabels       *[]string     `json:"runnerLabels,omitempty"`
	TimingEnvironment  *string       `json:"timingEnvironment,omitempty"`
}
type SelectedTier struct {
	Name               string    `json:"name"`
	MaxAffectedPercent float64   `json:"maxAffectedPercent"`
	Concurrency        uint64    `json:"concurrency"`
	Candidates         []uint64  `json:"-"`
	RunnerLabels       *[]string `json:"runnerLabels,omitempty"`
	TimingEnvironment  *string   `json:"timingEnvironment,omitempty"`
}

func Git(cwd string, args ...string) ([]byte, error) {
	cmd := exec.Command("git", append([]string{"-C", cwd}, args...)...)
	var diagnostics bytes.Buffer
	cmd.Stderr = &diagnostics
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("Git error: git %s failed: %s", strings.Join(args, " "), strings.TrimSpace(diagnostics.String()))
	}
	return out, nil
}
func GitString(cwd string, args ...string) (string, error) {
	b, err := Git(cwd, args...)
	return strings.TrimSpace(string(b)), err
}
func canonical(path string) (string, error) {
	path, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	return filepath.EvalSymlinks(path)
}
func within(root, path string) bool {
	relative, err := filepath.Rel(root, path)
	return err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(os.PathSeparator)) && !filepath.IsAbs(relative)
}
func SelectTier(d w.Distribution, percent float64) SelectedTier {
	name, tier := "full", d.Full
	if percent <= d.Small.MaxAffectedPercent {
		name, tier = "small", d.Small
	} else if percent <= d.Medium.MaxAffectedPercent {
		name, tier = "medium", d.Medium
	}
	environment := tier.TimingEnvironment
	if environment == nil && tier.RunnerLabels != nil {
		value := LabelsEnvironment(*tier.RunnerLabels)
		environment = &value
	}
	return SelectedTier{name, tier.MaxAffectedPercent, tier.Concurrency, []uint64{d.Small.Concurrency, d.Medium.Concurrency, d.Full.Concurrency}, tier.RunnerLabels, environment}
}
func Calculate(config w.Config, cwd, base, head string, diagnostics io.Writer) (Output, error) {
	output := Output{Group: map[string]GroupOutput{}}
	if base == "" {
		return output, fmt.Errorf("affected requires --base; GitHub event context must be resolved by the action")
	}
	if head == "" {
		head = "HEAD"
	}
	if strings.HasPrefix(base, "-") || strings.HasPrefix(head, "-") {
		return output, fmt.Errorf("Git error: invalid revision")
	}
	cwd, err := canonical(cwd)
	if err != nil {
		return output, err
	}
	gitRoot, err := GitString(cwd, "rev-parse", "--show-toplevel")
	if err != nil {
		return output, err
	}
	gitRoot, err = canonical(gitRoot)
	if err != nil {
		return output, err
	}
	mode := "merge-base"
	var baseCommit string
	resolveBase := func() (string, error) { return GitString(gitRoot, "merge-base", base+"^{commit}", head+"^{commit}") }
	if os.Getenv("COMPARISON") == "tip" {
		mode = "tip"
		baseCommit, err = GitString(gitRoot, "rev-parse", "--verify", "--end-of-options", base+"^{commit}")
	} else {
		baseCommit, err = resolveBase()
		depth := min(uint64(32), config.Affected.MaxFetchDepth)
		for err != nil {
			shallow, shallowErr := GitString(gitRoot, "rev-parse", "--is-shallow-repository")
			if shallowErr != nil {
				return output, shallowErr
			}
			if shallow != "true" {
				return output, fmt.Errorf("No common ancestor found between %s and %s. Try fetching more history or set COMPARISON=tip", base, head)
			}
			fmt.Fprintf(diagnostics, "affected: shallow repository; fetching commit-only origin history to depth %d (base=%s head=%s)\n", depth, base, head)
			if _, fetchErr := Git(gitRoot, "fetch", "--no-tags", "--filter=tree:0", fmt.Sprintf("--depth=%d", depth), "origin", base, head); fetchErr != nil {
				return output, fetchErr
			}
			baseCommit, err = resolveBase()
			if err != nil && depth == config.Affected.MaxFetchDepth {
				return output, fmt.Errorf("Shallow repository still has no common ancestor after fetching %d commits without trees or blobs; increase affected.maxFetchDepth", depth)
			}
			if depth > config.Affected.MaxFetchDepth/4 {
				depth = config.Affected.MaxFetchDepth
			} else {
				depth *= 4
			}
		}
	}
	if err != nil {
		return output, err
	}
	headCommit, err := GitString(gitRoot, "rev-parse", "--verify", "--end-of-options", head+"^{commit}")
	if err != nil {
		return output, err
	}
	raw, err := Git(gitRoot, "diff", "--name-status", "-z", baseCommit+".."+headCommit)
	if err != nil {
		return output, err
	}
	fields := bytes.Split(raw, []byte{0})
	changed, structural := []string{}, []string{}
	add := func(field []byte, structure bool) {
		path := filepath.Join(gitRoot, string(field))
		if !within(cwd, path) {
			return
		}
		relative, _ := filepath.Rel(cwd, path)
		relative = filepath.ToSlash(relative)
		changed = append(changed, relative)
		if structure && filepath.Base(path) == "package.json" && config.Includes(filepath.ToSlash(filepath.Dir(relative))) {
			structural = append(structural, relative)
		}
	}
	for i := 0; i < len(fields) && len(fields[i]) > 0; {
		status := fields[i][0]
		i++
		if i >= len(fields) {
			return output, fmt.Errorf("invalid git diff output")
		}
		switch status {
		case 'R':
			add(fields[i], true)
			i++
			if i >= len(fields) {
				return output, fmt.Errorf("invalid rename output")
			}
			add(fields[i], true)
			i++
		case 'C':
			i++
			if i >= len(fields) {
				return output, fmt.Errorf("invalid copy output")
			}
			add(fields[i], false)
			i++
		default:
			add(fields[i], status == 'D')
			i++
		}
	}
	manifests, manifestErr := Git(cwd, "ls-files", "-z", "--", "*/package.json")
	if manifestErr == nil {
		missing := []string{}
		for _, field := range bytes.Split(manifests, []byte{0}) {
			if len(field) == 0 {
				continue
			}
			relative := string(field)
			if config.Includes(filepath.ToSlash(filepath.Dir(relative))) {
				if _, err := os.Stat(filepath.Join(cwd, relative)); os.IsNotExist(err) {
					missing = append(missing, relative)
				}
			}
		}
		if len(missing) > 0 {
			sort.Strings(missing)
			return output, fmt.Errorf("affected sparse checkout is missing %d workspace package.json file(s): %s", len(missing), strings.Join(missing[:min(20, len(missing))], ", "))
		}
	}
	workspace, err := w.Discover(config, cwd)
	if err != nil {
		return output, err
	}
	installationPaths, err := workspace.InstallationPaths(cwd)
	if err != nil {
		return output, err
	}
	patterns := append([]string{}, config.GlobalDependencies...)
	for _, path := range installationPaths {
		patterns = append(patterns, path+"/**")
	}
	patterns = append(patterns, "package.json", "nanoom.config.json", "pnpm-workspace.yaml", "pnpm-lock.yaml", "yarn.lock", "package-lock.json", "npm-shrinkwrap.json", ".npmrc", ".yarnrc.yml", "turbo.json", "nx.json", "tsconfig*.json", ".yarn/releases/**", ".yarn/plugins/**")
	global := []string{}
	for _, file := range changed {
		matches := false
		for _, pattern := range patterns {
			matcher, e := glob.Compile(pattern)
			if e == nil && matcher.Match(file) {
				matches = true
				break
			}
		}
		if config.SourcePath != "" && within(cwd, config.SourcePath) {
			relative, _ := filepath.Rel(cwd, config.SourcePath)
			matches = matches || filepath.ToSlash(relative) == file
		}
		if matches {
			global = append(global, file)
		}
	}
	reasons := map[string]Reason{}
	direct := map[string][]string{}
	selected := map[string]bool{}
	for _, project := range workspace.Projects {
		for _, file := range changed {
			absolute := filepath.Join(cwd, filepath.FromSlash(file))
			if within(project.Path, absolute) {
				direct[project.Name] = append(direct[project.Name], file)
				selected[project.Name] = true
			}
		}
	}
	queue := []string{}
	for _, project := range workspace.Projects {
		if selected[project.Name] {
			queue = append(queue, project.Name)
		}
	}
	for len(queue) > 0 {
		name := queue[0]
		queue = queue[1:]
		project, _ := workspace.Project(name)
		for _, dep := range project.Dependents {
			if !selected[dep] {
				selected[dep] = true
				queue = append(queue, dep)
			}
		}
	}
	for _, project := range workspace.Projects {
		switch {
		case len(structural) > 0:
			reasons[project.Name] = Reason{Kind: "workspaceManifestStructure", ChangedFiles: structural}
			selected[project.Name] = true
		case len(global) > 0:
			reasons[project.Name] = Reason{Kind: "globalDependency", ChangedFiles: global}
			selected[project.Name] = true
		case len(direct[project.Name]) > 0:
			reasons[project.Name] = Reason{Kind: "direct", ChangedFiles: direct[project.Name]}
		case selected[project.Name]:
			if path := dependencyPath(workspace, project.Name, direct); path != nil {
				reasons[project.Name] = Reason{Kind: "transitiveDependent", DependencyPath: path}
			}
		}
	}
	for name, group := range config.Group {
		projects := []w.Project{}
		for _, project := range workspace.Projects {
			if !selected[project.Name] {
				continue
			}
			ignored := false
			for _, rule := range group.Rules {
				if rule.Name == project.Name {
					ignored = rule.Ignore
					break
				}
			}
			if !ignored {
				projects = append(projects, project)
			}
		}
		percent := float64(0)
		if len(workspace.Projects) > 0 {
			percent = float64(len(projects)) * 100 / float64(len(workspace.Projects))
		}
		result := GroupOutput{Label: name, Workspaces: []Entry{}, TotalWorkspaces: uint64(len(workspace.Projects)), AffectedWorkspaces: uint64(len(projects)), AffectedPercent: percent, RunnerLabels: group.RunnerLabels, TimingEnvironment: group.TimingEnvironment}
		if group.Distribution != nil {
			tier := SelectTier(*group.Distribution, percent)
			if len(projects) > 0 && tier.TimingEnvironment == nil {
				tier.TimingEnvironment = group.TimingEnvironment
				if tier.TimingEnvironment == nil && group.RunnerLabels != nil {
					env := LabelsEnvironment(*group.RunnerLabels)
					tier.TimingEnvironment = &env
				}
			}
			result.Distribution = &tier
		}
		for _, task := range group.Tasks {
			for _, project := range projects {
				path, _ := filepath.Rel(cwd, project.Path)
				paths := append(workspace.Closure(project.Name, cwd), installationPaths...)
				paths = append(paths, config.Checkout.Always...)
				var shards uint64
				for _, rule := range group.Rules {
					if rule.Name == project.Name {
						for _, spec := range rule.Shard {
							if spec.Task == task {
								shards = spec.Shard
								break
							}
						}
						break
					}
				}
				item := Entry{Group: name, Name: project.Name, Path: filepath.ToSlash(path), Task: task, CheckoutPaths: paths}
				if shards > 0 {
					for index := uint64(1); index <= shards; index++ {
						entry := item
						entry.Shard = new(uint64)
						*entry.Shard = index
						entry.TotalShards = &shards
						result.Workspaces = append(result.Workspaces, entry)
					}
				} else {
					result.Workspaces = append(result.Workspaces, item)
				}
			}
		}
		output.HasChange = output.HasChange || len(projects) > 0
		output.Group[name] = result
	}
	output.Diagnostics = &Diagnostics{Comparison: Comparison{mode, base, head, baseCommit, headCommit}, ChangedFiles: changed, Reasons: reasons}
	return output, nil
}
func dependencyPath(workspace w.Workspace, start string, direct map[string][]string) []string {
	queue := [][]string{{start}}
	seen := map[string]bool{start: true}
	for len(queue) > 0 {
		path := queue[0]
		queue = queue[1:]
		project, ok := workspace.Project(path[len(path)-1])
		if !ok {
			continue
		}
		deps := append([]string{}, project.Dependencies...)
		sort.Strings(deps)
		for _, dep := range deps {
			if _, ok := workspace.Project(dep); !ok || seen[dep] {
				continue
			}
			seen[dep] = true
			next := append(append([]string{}, path...), dep)
			if len(direct[dep]) > 0 {
				return next
			}
			queue = append(queue, next)
		}
	}
	return nil
}
