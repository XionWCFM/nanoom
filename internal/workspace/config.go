package workspace

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"unicode"

	"github.com/XionWCFM/nanoom/internal/prediction"
	"github.com/gobwas/glob"
	"go.yaml.in/yaml/v3"
)

type Config struct {
	Schema             *string          `json:"$schema"`
	Group              map[string]Group `json:"group" optional:"true"`
	GlobalDependencies []string         `json:"globalDependencies" optional:"true"`
	Workspace          WorkspaceConfig  `json:"workspace" optional:"true"`
	Affected           struct {
		MaxFetchDepth uint64 `json:"maxFetchDepth" optional:"true"`
	} `json:"affected" optional:"true"`
	Checkout struct {
		Always []string `json:"always" optional:"true"`
	} `json:"checkout" optional:"true"`
	SourcePath string `json:"-"`
}
type WorkspaceConfig struct {
	Include []string `json:"include" optional:"true"`
	Exclude []string `json:"exclude" optional:"true"`
}
type Group struct {
	Tasks             []string      `json:"tasks"`
	Rules             []Rule        `json:"rules" optional:"true"`
	Distribution      *Distribution `json:"distribution" optional:"true"`
	RunnerLabels      *[]string     `json:"runnerLabels" optional:"true"`
	TimingEnvironment *string       `json:"timingEnvironment" optional:"true"`
}
type Rule struct {
	Name   string      `json:"name"`
	Ignore bool        `json:"ignore" optional:"true"`
	Shard  []ShardRule `json:"shard" optional:"true"`
}
type ShardRule struct {
	Task  string `json:"task"`
	Shard uint64 `json:"shard"`
}
type Distribution struct {
	Small  Tier `json:"small"`
	Medium Tier `json:"medium"`
	Full   Tier `json:"full"`
}
type Tier struct {
	MaxAffectedPercent float64   `json:"maxAffectedPercent"`
	Concurrency        uint64    `json:"concurrency"`
	RunnerLabels       *[]string `json:"runnerLabels" optional:"true"`
	TimingEnvironment  *string   `json:"timingEnvironment" optional:"true"`
}

func LoadConfig(cwd, path string) (Config, error) {
	c := Config{}
	c.Workspace.Include = []string{"packages/*", "apps/*"}
	c.Affected.MaxFetchDepth = 2048
	file := path
	if !filepath.IsAbs(path) {
		file = filepath.Join(cwd, path)
	}
	b, err := os.ReadFile(file)
	if err != nil {
		return c, err
	}
	var top map[string]json.RawMessage
	if err := json.Unmarshal(b, &top); err != nil {
		return c, err
	}
	if _, explicit := top["workspace"]; explicit {
		c.Workspace.Include = nil
	}
	if err := prediction.Decode(b, &c); err != nil {
		return c, fmt.Errorf("invalid configuration: %w", err)
	}
	var raw struct {
		Workspace map[string]json.RawMessage `json:"workspace" optional:"true"`
	}
	if err := json.Unmarshal(b, &raw); err != nil {
		return c, err
	}
	if _, explicit := raw.Workspace["include"]; !explicit {
		patterns, found, err := repositoryPatterns(cwd)
		if err != nil {
			return c, err
		}
		if found {
			c.Workspace.Include = []string{}
			for _, pattern := range patterns {
				if exclude, ok := strings.CutPrefix(pattern, "!"); ok {
					c.Workspace.Exclude = append(c.Workspace.Exclude, exclude)
				} else {
					c.Workspace.Include = append(c.Workspace.Include, pattern)
				}
			}
			if len(c.Workspace.Include) == 0 {
				return c, fmt.Errorf("workspace manifest must declare a positive workspace pattern")
			}
		}
	}
	if err := c.Validate(); err != nil {
		return c, err
	}
	source, err := filepath.EvalSymlinks(file)
	if err != nil {
		return c, err
	}
	c.SourcePath, err = filepath.Abs(source)
	return c, err
}
func repositoryPatterns(cwd string) ([]string, bool, error) {
	b, err := os.ReadFile(filepath.Join(cwd, "pnpm-workspace.yaml"))
	if err == nil {
		var raw struct {
			Packages *[]string `yaml:"packages"`
		}
		if err := yaml.Unmarshal(b, &raw); err != nil {
			return nil, true, err
		}
		if raw.Packages == nil {
			return nil, true, fmt.Errorf("pnpm-workspace.yaml packages must be an array")
		}
		return *raw.Packages, true, nil
	}
	if !os.IsNotExist(err) {
		return nil, false, err
	}
	b, err = os.ReadFile(filepath.Join(cwd, "package.json"))
	if os.IsNotExist(err) {
		return nil, false, nil
	}
	if err != nil {
		return nil, false, err
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(b, &raw); err != nil {
		return nil, false, err
	}
	w, found := raw["workspaces"]
	if !found {
		return nil, false, nil
	}
	if len(w) > 0 && w[0] == '{' {
		var object map[string]json.RawMessage
		if err := json.Unmarshal(w, &object); err != nil {
			return nil, true, err
		}
		w = object["packages"]
	}
	var patterns []string
	if bytes.Equal(bytes.TrimSpace(w), []byte("null")) {
		return nil, true, fmt.Errorf("package.json workspaces must be an array")
	}
	if err := json.Unmarshal(w, &patterns); err != nil {
		return nil, true, err
	}
	return patterns, true, nil
}
func validText(s string) bool {
	return strings.TrimSpace(s) != "" && strings.IndexFunc(s, unicode.IsControl) < 0
}
func validateRunner(labels *[]string, environment *string) error {
	if environment != nil && !validText(*environment) {
		return fmt.Errorf("timingEnvironment must not be empty or contain control characters")
	}
	if labels != nil {
		if len(*labels) == 0 {
			return fmt.Errorf("runnerLabels must contain non-empty labels")
		}
		seen := map[string]bool{}
		for _, label := range *labels {
			if !validText(label) || seen[label] {
				return fmt.Errorf("runnerLabels must contain unique non-empty labels")
			}
			seen[label] = true
		}
	}
	return nil
}
func (c Config) Validate() error {
	if len(c.Group) == 0 {
		return fmt.Errorf("at least one group must be defined")
	}
	if c.Affected.MaxFetchDepth == 0 {
		return fmt.Errorf("affected.maxFetchDepth must be greater than 0")
	}
	for _, path := range c.Checkout.Always {
		if path == "" || filepath.IsAbs(path) || strings.ContainsAny(path, "*?[]") || hasParent(path) {
			return fmt.Errorf("checkout.always entry %q must be a repository-relative directory without glob syntax", path)
		}
	}
	for name, g := range c.Group {
		if err := validateRunner(g.RunnerLabels, g.TimingEnvironment); err != nil {
			return fmt.Errorf("group %q: %w", name, err)
		}
		if len(g.Tasks) == 0 {
			return fmt.Errorf("group %q must have at least one task", name)
		}
		tasks := map[string]bool{}
		for _, task := range g.Tasks {
			if task == "" || tasks[task] {
				return fmt.Errorf("group %q has an empty or duplicate task", name)
			}
			tasks[task] = true
		}
		if g.Distribution != nil {
			previous := float64(0)
			for _, tier := range []Tier{g.Distribution.Small, g.Distribution.Medium, g.Distribution.Full} {
				if err := validateRunner(tier.RunnerLabels, tier.TimingEnvironment); err != nil {
					return err
				}
				if tier.MaxAffectedPercent <= previous || tier.MaxAffectedPercent > 100 || tier.Concurrency == 0 {
					return fmt.Errorf("group %q has invalid distribution tiers", name)
				}
				previous = tier.MaxAffectedPercent
			}
			if previous != 100 {
				return fmt.Errorf("group %q full distribution must end at 100", name)
			}
		}
		rules := map[string]bool{}
		for _, rule := range g.Rules {
			if rule.Name == "" || rules[rule.Name] {
				return fmt.Errorf("group %q has an empty or duplicate rule", name)
			}
			rules[rule.Name] = true
			for _, shard := range rule.Shard {
				if !tasks[shard.Task] || shard.Shard == 0 {
					return fmt.Errorf("group %q rule %q has invalid shard layout", name, rule.Name)
				}
			}
		}
	}
	patterns := append(append(append([]string{}, c.GlobalDependencies...), c.Workspace.Include...), c.Workspace.Exclude...)
	for _, pattern := range patterns {
		if _, err := glob.Compile(pattern); err != nil {
			return fmt.Errorf("invalid glob %q: %w", pattern, err)
		}
	}
	return nil
}
func hasParent(path string) bool {
	for _, part := range strings.Split(filepath.ToSlash(path), "/") {
		if part == ".." {
			return true
		}
	}
	return false
}
func (c Config) Includes(path string) bool {
	included := len(c.Workspace.Include) == 0
	for _, pattern := range c.Workspace.Include {
		matcher, err := glob.Compile(pattern, '/')
		if err == nil && matcher.Match(filepath.ToSlash(path)) {
			included = true
			break
		}
	}
	if !included {
		return false
	}
	for _, pattern := range c.Workspace.Exclude {
		matcher, err := glob.Compile(pattern, '/')
		if err == nil && matcher.Match(filepath.ToSlash(path)) {
			return false
		}
	}
	return true
}
