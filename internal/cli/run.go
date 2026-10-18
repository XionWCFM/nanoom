package cli

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"path/filepath"
	"strconv"
	"time"

	aff "github.com/XionWCFM/nanoom/internal/affected"
	w "github.com/XionWCFM/nanoom/internal/workspace"
)

type taskExecution struct {
	Workspace   string `json:"workspace"`
	Runner      string `json:"runner"`
	StartedAtMs uint64 `json:"startedAtMs"`
	DurationMs  uint64 `json:"durationMs"`
}
type reportedError struct{ error }

func (e reportedError) Is(target error) bool { return target == reportedFailure }
func contains(values []string, target string) bool {
	for _, value := range values {
		if value == target {
			return true
		}
	}
	return false
}
func runTasks(a arguments, cwd, configPath string, out, diagnostics io.Writer) error {
	if err := a.allow("runner", "filter", "shard", "total-shards", "all", "base", "head", "continue-on-error", "json"); err != nil {
		return err
	}
	if len(a.positionals) != 2 {
		return fmt.Errorf("run requires group and task")
	}
	if a.has("all") && (a.has("base") || a.has("head")) {
		return fmt.Errorf("--all cannot be used with --base or --head")
	}
	if !a.has("all") && !a.has("base") {
		return fmt.Errorf("--base is required unless --all is present")
	}
	if a.has("head") && !a.has("base") {
		return fmt.Errorf("--head requires --base")
	}
	var shard, total *uint64
	if a.has("shard") || a.has("total-shards") {
		index, err := strconv.ParseUint(a.get("shard"), 10, 64)
		if err != nil {
			return fmt.Errorf("--shard and --total-shards must be provided together with 1 <= shard <= total-shards")
		}
		count, err := strconv.ParseUint(a.get("total-shards"), 10, 64)
		if err != nil || index == 0 || index > count {
			return fmt.Errorf("--shard and --total-shards must be provided together with 1 <= shard <= total-shards")
		}
		shard, total = &index, &count
	}
	config, err := w.LoadConfig(cwd, configPath)
	if err != nil {
		return err
	}
	name, task := a.positionals[0], a.positionals[1]
	group, ok := config.Group[name]
	if !ok {
		return fmt.Errorf("Group '%s' not found", name)
	}
	if !contains(group.Tasks, task) {
		return fmt.Errorf("Task '%s' not in group '%s'", task, name)
	}
	cwd, err = filepath.Abs(cwd)
	if err != nil {
		return err
	}
	workspace, err := w.Discover(config, cwd)
	if err != nil {
		return err
	}
	projects := []w.Project{}
	if a.has("all") {
		for _, project := range workspace.Projects {
			ignored, shardMatches := false, shard == nil
			for _, rule := range group.Rules {
				if rule.Name == project.Name {
					ignored = rule.Ignore
					if shard != nil {
						for _, spec := range rule.Shard {
							if spec.Task == task && *shard <= spec.Shard {
								shardMatches = true
							}
						}
					}
					break
				}
			}
			if !ignored && shardMatches && (!a.has("filter") || project.Name == a.get("filter")) {
				projects = append(projects, project)
			}
		}
	} else {
		result, err := aff.Calculate(config, cwd, a.get("base"), a.get("head"), diagnostics)
		if err != nil {
			return err
		}
		for _, item := range result.Group[name].Workspaces {
			if item.Task != task || (a.has("filter") && item.Name != a.get("filter")) || (shard != nil && (item.Shard == nil || *item.Shard != *shard)) {
				continue
			}
			if project, ok := workspace.Project(item.Name); ok {
				projects = append(projects, project)
			}
		}
	}
	runner := a.get("runner")
	if runner == "" {
		runner = "auto"
	}
	if len(projects) == 0 {
		if a.has("all") && a.has("filter") {
			return fmt.Errorf("no workspace matched explicit --all --filter for group '%s' and task '%s'", name, task)
		}
		if a.has("json") {
			return json.NewEncoder(out).Encode(map[string]any{"status": "success", "group": name, "task": task, "projects": []string{}, "runner": runner, "reason": "no workspace matched the requested group, task, filter, and shard constraints", "executions": []taskExecution{}})
		}
		fmt.Fprintf(out, "◆ nanoom run\n  Result: no workspace matched group=%s, task=%s\n", name, task)
		return nil
	}
	ordered := []w.Project{}
	selectedNames := []string{}
	for _, project := range projects {
		selectedNames = append(selectedNames, project.Name)
	}
	for _, project := range w.Topological(workspace.Projects) {
		if contains(selectedNames, project.Name) {
			ordered = append(ordered, project)
		}
	}
	if !a.has("json") {
		fmt.Fprintf(diagnostics, "◆ nanoom run\n  Selection: group=%s, task=%s, projects=%d\n", name, task, len(projects))
	}
	environment := map[string]string{}
	if shard != nil {
		environment["NANOOM_SHARD_INDEX"] = strconv.FormatUint(*shard, 10)
		environment["NANOOM_SHARD_TOTAL"] = strconv.FormatUint(*total, 10)
	}
	executions := []taskExecution{}
	failed, pending, completed := []string{}, []string{}, []string{}
	var firstError error
	for i, project := range ordered {
		if !a.has("json") {
			fmt.Fprintf(diagnostics, "\n--- %s ---\n", project.Name)
		}
		execution, err := runTask(project, task, runner, cwd, environment, a.has("json"), out, diagnostics)
		if err == nil {
			executions = append(executions, execution)
			completed = append(completed, project.Name)
			continue
		}
		fmt.Fprintf(diagnostics, "Error in %s: %v\n", project.Name, err)
		failed = append(failed, project.Name)
		if firstError == nil {
			firstError = err
		}
		if !a.has("continue-on-error") {
			for _, remaining := range ordered[i+1:] {
				pending = append(pending, remaining.Name)
			}
			break
		}
	}
	if firstError != nil {
		if a.has("json") {
			if err := json.NewEncoder(out).Encode(map[string]any{"status": "failure", "group": name, "task": task, "completed": completed, "failed": failed, "pending": pending, "executions": executions, "error": firstError.Error()}); err != nil {
				return err
			}
			return reportedError{firstError}
		}
		return firstError
	}
	if a.has("json") {
		var filter *string
		if a.has("filter") {
			value := a.get("filter")
			filter = &value
		}
		selection, reason := "affected", "affected calculation selected these workspaces"
		if a.has("all") {
			selection = "all"
			reason = "explicit --all selection constrained by matrix arguments"
		}
		return json.NewEncoder(out).Encode(map[string]any{"status": "success", "group": name, "task": task, "projects": selectedNames, "runner": runner, "filter": filter, "shard": shard, "totalShards": total, "selection": selection, "completed": completed, "failed": failed, "pending": pending, "executions": executions, "reason": reason})
	}
	return nil
}
func runTask(project w.Project, task, requested, root string, environment map[string]string, jsonOutput bool, out, diagnostics io.Writer) (taskExecution, error) {
	selected := requested
	if selected == "auto" {
		turbo, nx := isFile(filepath.Join(root, "turbo.json")), isFile(filepath.Join(root, "nx.json"))
		if turbo && nx {
			return taskExecution{}, fmt.Errorf("Invalid runner: both turbo.json and nx.json exist; set monorepoTool explicitly")
		}
		if turbo {
			selected = "turbo"
		} else if nx {
			selected = "nx"
		}
	}
	var program string
	args := []string{}
	cwd := project.Path
	switch selected {
	case "turbo":
		program = "turbo"
		args = []string{"run", task, "--filter", project.Name}
		cwd = root
	case "nx":
		program = "nx"
		args = []string{"run", project.Name + ":" + task}
		cwd = root
	case "pnpm", "yarn", "npm":
		program = requested
		args = []string{"run", task}
	case "auto":
		m, err := w.ReadManifest(project.Path)
		if err != nil {
			return taskExecution{}, err
		}
		if _, ok := m.Scripts[task]; ok {
			program, err = detectPackageManager(root, "")
			if err != nil {
				return taskExecution{}, err
			}
			args = []string{"run", task}
		} else {
			program = task
		}
	default:
		return taskExecution{}, fmt.Errorf("Invalid runner: %s. Supported: turbo, yarn, pnpm, nx", selected)
	}
	fmt.Fprintf(diagnostics, "Executing task command (cwd=%s): %s\n", cwd, displayCommand(program, args))
	at := uint64(time.Now().UnixMilli())
	started := time.Now()
	err := executeTool(program, args, cwd, environment, jsonOutput, out, diagnostics)
	if err != nil {
		var exit *exec.ExitError
		if errors.As(err, &exit) {
			return taskExecution{}, fmt.Errorf("Task failed: project=%s, task=%s, exit code=%d", project.Name, task, exit.ExitCode())
		}
		return taskExecution{}, err
	}
	return taskExecution{project.Name, program, at, uint64(time.Since(started).Milliseconds())}, nil
}
