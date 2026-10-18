package workspace

import (
	"bufio"
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/go-git/go-git/v5/plumbing/format/gitignore"
)

type ignoreRules struct{ git, custom []gitignore.Pattern }

func ignorePatterns(file string, domain []string) ([]gitignore.Pattern, error) {
	bytes, err := os.ReadFile(file)
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	patterns := []gitignore.Pattern{}
	scanner := bufio.NewScanner(strings.NewReader(string(bytes)))
	scanner.Buffer(make([]byte, 4096), 1024*1024)
	for scanner.Scan() {
		line := strings.TrimSuffix(scanner.Text(), "\r")
		if strings.HasPrefix(line, "#") || strings.TrimSpace(line) == "" {
			continue
		}
		patterns = append(patterns, gitignore.ParsePattern(line, domain))
	}
	return patterns, scanner.Err()
}
func ignoreDomain(root, path string) []string {
	relative, _ := filepath.Rel(root, path)
	if relative == "." {
		return nil
	}
	return strings.Split(filepath.ToSlash(relative), "/")
}
func (r ignoreRules) extend(base, path string) (ignoreRules, error) {
	next := ignoreRules{git: append([]gitignore.Pattern{}, r.git...), custom: append([]gitignore.Pattern{}, r.custom...)}
	domain := ignoreDomain(base, path)
	for _, file := range []string{".gitignore", ".ignore"} {
		patterns, err := ignorePatterns(filepath.Join(path, file), domain)
		if err != nil {
			return next, err
		}
		if file == ".ignore" {
			next.custom = append(next.custom, patterns...)
		} else {
			next.git = append(next.git, patterns...)
		}
	}
	return next, nil
}
func (r ignoreRules) excludes(base, path string, directory bool) bool {
	parts := ignoreDomain(base, path)
	for _, patterns := range [][]gitignore.Pattern{r.custom, r.git} {
		for i := len(patterns) - 1; i >= 0; i-- {
			if result := patterns[i].Match(parts, directory); result != gitignore.NoMatch {
				return result == gitignore.Exclude
			}
		}
	}
	return false
}
func initialIgnore(root string) (string, ignoreRules, error) {
	git := func(args ...string) string {
		cmd := exec.Command("git", append([]string{"-C", root}, args...)...)
		out, err := cmd.Output()
		if err != nil {
			return ""
		}
		return string(bytes.TrimSpace(out))
	}
	base := git("rev-parse", "--show-toplevel")
	if base == "" {
		base = root
	}
	base, err := filepath.EvalSymlinks(base)
	if err != nil {
		return base, ignoreRules{}, err
	}
	canonicalRoot, err := filepath.EvalSymlinks(root)
	if err != nil {
		return base, ignoreRules{}, err
	}
	relativeRoot, err := filepath.Rel(base, canonicalRoot)
	if err != nil {
		return base, ignoreRules{}, err
	}
	// Keep the caller's path spelling (macOS /var and /private/var are aliases).
	base = root
	if relativeRoot != "." {
		for range strings.Split(relativeRoot, string(os.PathSeparator)) {
			base = filepath.Dir(base)
		}
	}
	rules := ignoreRules{}
	global := git("config", "--path", "--get", "core.excludesfile")
	if global == "" {
		configHome := os.Getenv("XDG_CONFIG_HOME")
		if configHome == "" {
			if home, err := os.UserHomeDir(); err == nil {
				configHome = filepath.Join(home, ".config")
			}
		}
		if configHome != "" {
			global = filepath.Join(configHome, "git", "ignore")
		}
	}
	if global != "" {
		if !filepath.IsAbs(global) {
			global = filepath.Join(root, global)
		}
		patterns, err := ignorePatterns(global, nil)
		if err != nil {
			return base, rules, err
		}
		rules.git = append(rules.git, patterns...)
	}
	if exclude := git("rev-parse", "--git-path", "info/exclude"); exclude != "" {
		if !filepath.IsAbs(exclude) {
			exclude = filepath.Join(root, exclude)
		}
		patterns, err := ignorePatterns(exclude, nil)
		if err != nil {
			return base, rules, err
		}
		rules.git = append(rules.git, patterns...)
	}
	relative, err := filepath.Rel(base, root)
	if err != nil {
		return base, rules, err
	}
	if relative != "." {
		path := base
		for _, part := range strings.Split(relative, string(os.PathSeparator)) {
			rules, err = rules.extend(base, path)
			if err != nil {
				return base, rules, err
			}
			path = filepath.Join(path, part)
		}
	}
	return base, rules, nil
}
