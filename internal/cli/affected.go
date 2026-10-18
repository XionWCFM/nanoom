package cli

import (
	"encoding/json"
	"fmt"
	"io"
	"path/filepath"
	"sort"
	"strings"
	"time"
	"unicode/utf16"

	aff "github.com/XionWCFM/nanoom/internal/affected"
	pl "github.com/XionWCFM/nanoom/internal/plan"
	p "github.com/XionWCFM/nanoom/internal/prediction"
	w "github.com/XionWCFM/nanoom/internal/workspace"
)

func resolvePath(cwd, path string) string {
	if filepath.IsAbs(path) {
		return path
	}
	return filepath.Join(cwd, path)
}
func timingRunner(cwd, requested string) (string, error) {
	if requested != "" && requested != "auto" {
		return requested, nil
	}
	turbo, nx := isFile(filepath.Join(cwd, "turbo.json")), isFile(filepath.Join(cwd, "nx.json"))
	if turbo && nx {
		return "", fmt.Errorf("both turbo.json and nx.json exist; set timingRunner explicitly")
	}
	if turbo {
		return "turbo", nil
	}
	if nx {
		return "nx", nil
	}
	return detectPackageManager(cwd, "")
}
func affected(a arguments, cwd, configPath string, out, diagnostics io.Writer) error {
	if err := a.allow("base", "head", "json", "prediction", "prediction-table", "prediction-context", "preparation-context", "history", "history-status", "timing-runner", "timing-environment", "plan-output", "plan-context"); err != nil {
		return err
	}
	if len(a.positionals) > 0 {
		return fmt.Errorf("unexpected affected positional argument")
	}
	if a.has("plan-output") != a.has("plan-context") {
		return fmt.Errorf("--plan-output and --plan-context must be supplied together")
	}
	if a.has("json") && a.has("plan-output") {
		return fmt.Errorf("--json full reports cannot be combined with bounded Plan outputs")
	}
	if (a.has("prediction") || a.has("prediction-table")) && !a.has("prediction-context") {
		return fmt.Errorf("--prediction-context is required with --prediction or --prediction-table")
	}
	if a.has("history-status") {
		switch a.get("history-status") {
		case "disabled", "fallback", "corrupt":
		default:
			return fmt.Errorf("invalid --history-status")
		}
	}
	cwd, err := filepath.Abs(cwd)
	if err != nil {
		return err
	}
	cwd, err = filepath.EvalSymlinks(cwd)
	if err != nil {
		return err
	}
	config, err := w.LoadConfig(cwd, configPath)
	if err != nil {
		return err
	}
	runner, err := timingRunner(cwd, a.get("timing-runner"))
	if err != nil {
		return err
	}
	environment := a.get("timing-environment")
	if environment == "" {
		environment = "default"
	}
	result, err := aff.Calculate(config, cwd, a.get("base"), a.get("head"), diagnostics)
	if err != nil {
		return err
	}
	needed := false
	for _, group := range result.Group {
		needed = needed || (len(group.Workspaces) > 1 && group.Distribution != nil && group.Distribution.Concurrency > 1)
	}
	now := uint64(time.Now().UnixMilli())
	historyStatus := a.get("history-status")
	if historyStatus == "" {
		historyStatus = "disabled"
	}
	index, _ := p.NewIndex(nil, nil)
	var context *p.PredictionContext
	if needed && a.has("prediction-context") {
		var value p.PredictionContext
		if p.ReadArtifact(resolvePath(cwd, a.get("prediction-context")), 4096, &value) == nil && value.Validate() == nil {
			context = &value
		}
	}
	if needed {
		if a.has("history") && !a.has("prediction") && !a.has("prediction-table") {
			fmt.Fprintln(diagnostics, "legacy raw timing history is ignored; using cold scheduling")
			if !a.has("history-status") {
				historyStatus = "fallback"
			}
		}
		if (a.has("prediction") || a.has("prediction-table")) && historyStatus != "corrupt" {
			load := func() (p.Index, error) {
				bundles := []p.PredictionArtifactBundle{}
				for _, path := range a.options["prediction"] {
					var bundle p.PredictionArtifactBundle
					if err := p.ReadArtifact(resolvePath(cwd, path), 8*1024*1024, &bundle); err != nil {
						return p.Index{}, err
					}
					if err := bundle.Validate(); err != nil {
						return p.Index{}, err
					}
					bundles = append(bundles, bundle)
				}
				tables := []p.PredictionTable{}
				for _, path := range a.options["prediction-table"] {
					var table p.PredictionTable
					if err := p.ReadArtifact(resolvePath(cwd, path), 8*1024*1024, &table); err != nil {
						return p.Index{}, err
					}
					if err := table.Validate(); err != nil {
						return p.Index{}, err
					}
					tables = append(tables, table)
				}
				if context == nil {
					return p.Index{}, fmt.Errorf("PredictionContext is unavailable")
				}
				allowed := map[string]bool{}
				for name, group := range result.Group {
					if len(group.Workspaces) > 1 && group.Distribution != nil && group.Distribution.Concurrency > 1 {
						for _, scope := range context.Candidates(name, runner, environment) {
							id, err := scope.ID()
							if err != nil {
								return p.Index{}, err
							}
							allowed[id] = true
						}
					}
				}
				for _, table := range tables {
					id, _ := table.Scope.ID()
					if !allowed[id] {
						return p.Index{}, fmt.Errorf("server PredictionTable scope does not match the request")
					}
				}
				return p.NewIndex(bundles, tables)
			}
			loaded, err := load()
			if err != nil {
				fmt.Fprintln(diagnostics, "PredictionArtifact v3 unavailable; using cold scheduling:", err)
				historyStatus = "corrupt"
			} else if loaded.HasValidRows(now) {
				index = loaded
				historyStatus = "loaded"
			} else {
				historyStatus = "fallback"
			}
		}
	} else {
		historyStatus = "history_not_needed"
	}
	scopes := []map[string]any{}
	if needed && context != nil {
		names := []string{}
		for name := range result.Group {
			names = append(names, name)
		}
		sort.Strings(names)
		for _, name := range names {
			group := result.Group[name]
			if len(group.Workspaces) > 1 && group.Distribution != nil && group.Distribution.Concurrency > 1 {
				for _, scope := range context.Candidates(name, runner, environment) {
					id, err := scope.ID()
					if err != nil {
						return err
					}
					scopes = append(scopes, map[string]any{"repositoryKey": scope.RepositoryKey, "group": name, "scopeId": id})
				}
			}
		}
	}
	var prep *p.PreparationContext
	if a.has("preparation-context") {
		var value p.PreparationContext
		if p.ReadArtifact(resolvePath(cwd, a.get("preparation-context")), 4096, &value) == nil && value.Validate() == nil {
			prep = &value
		}
	}
	matrix := aff.Matrix(result, index, context, runner, environment, prep, now)
	rows, err := aff.Rows(matrix)
	if err != nil {
		return err
	}
	var total uint64
	uniquePaths := map[string]bool{}
	sources := pl.PredictionSources{}
	var automatic, coldCap, prepExact, prepGroup, prepUnknown uint64
	for _, group := range result.Group {
		for _, item := range group.Workspaces {
			for _, path := range item.CheckoutPaths {
				uniquePaths[path] = true
			}
		}
	}
	for _, group := range rows {
		for _, row := range group {
			var count uint64
			_ = json.Unmarshal(row["checkoutPathCount"], &count)
			total += count
			var value pl.PredictionSources
			if b, ok := row["predictionSources"]; ok {
				if err := json.Unmarshal(b, &value); err != nil {
					return err
				}
				sources.Exact += value.Exact
				sources.Group += value.Group
				sources.Cold += value.Cold
				sources.SampleCount += value.SampleCount
			}
			var mode, source string
			_ = json.Unmarshal(row["schedulingMode"], &mode)
			_ = json.Unmarshal(row["preparationPredictionSource"], &source)
			if mode == "automatic" {
				automatic++
			} else if mode == "cold-cap" {
				coldCap++
			} else {
				continue
			}
			switch source {
			case "exact":
				prepExact++
			case "group":
				prepGroup++
			default:
				prepUnknown++
			}
		}
	}
	concurrency := map[string]any{"automaticAssignmentCount": automatic, "coldCapAssignmentCount": coldCap, "preparationPredictionSources": map[string]uint64{"exact": prepExact, "group": prepGroup, "unknown": prepUnknown}}
	if a.has("json") {
		duplicate := uint64(0)
		if total > uint64(len(uniquePaths)) {
			duplicate = total - uint64(len(uniquePaths))
		}
		return json.NewEncoder(out).Encode(map[string]any{"affected": result, "matrix": matrix, "scheduling": map[string]any{"historyStatus": historyStatus, "historyNeeded": needed, "historyScopes": scopes, "timingRunner": runner, "timingEnvironment": environment, "objective": []string{"predictedPreparationPlusTaskMakespanMs", "totalRunnerMs", "totalCheckoutPathCount", "assignmentCount", "stableAssignmentOrder"}, "totalCheckoutPathCount": total, "uniqueCheckoutPathCount": len(uniquePaths), "duplicatedCheckoutPathCount": duplicate, "predictionSources": map[string]uint64{"exact": sources.Exact, "group": sources.Group, "cold": sources.Cold, "sampleCount": sources.SampleCount}, "concurrency": concurrency}})
	}
	if a.has("plan-output") {
		compact, err := writeAffectedPlan(result, rows, runner, cwd, resolvePath(cwd, a.get("plan-context")), resolvePath(cwd, a.get("plan-output")), config.SourcePath)
		if err != nil {
			return err
		}
		summary := compact["result"].(map[string]any)
		summary["timingRunner"] = runner
		summary["timingEnvironment"] = environment
		summary["historyStatus"] = historyStatus
		summary["historyNeeded"] = needed
		summary["historyScopes"] = scopes
		summary["concurrency"] = concurrency
		return json.NewEncoder(out).Encode(compact)
	}
	fmt.Fprintln(out, "◆ nanoom affected")
	resultText := "no changes found"
	if result.HasChange {
		resultText = "changes found"
	}
	fmt.Fprintln(out, "  Result:", resultText)
	fmt.Fprintf(out, "  Comparison: %s\n  Commits: %s -> %s\n  Changed files: %d\n", result.Diagnostics.Comparison.Mode, result.Diagnostics.Comparison.BaseCommit, result.Diagnostics.Comparison.HeadCommit, len(result.Diagnostics.ChangedFiles))
	for _, path := range result.Diagnostics.ChangedFiles {
		fmt.Fprintln(out, "  -", path)
	}
	names := []string{}
	for name := range result.Group {
		names = append(names, name)
	}
	sort.Strings(names)
	for _, name := range names {
		group := result.Group[name]
		fmt.Fprintf(out, "\n  Matrix group: %s (%d entries)\n", name, len(group.Workspaces))
		for _, item := range group.Workspaces {
			fmt.Fprintf(out, "    - %s / %s [%s]\n", item.Name, item.Task, item.Path)
			if reason, ok := result.Diagnostics.Reasons[item.Name]; ok {
				why := "transitive dependency: " + strings.Join(reason.DependencyPath, " -> ")
				switch reason.Kind {
				case "direct":
					why = "direct change: " + strings.Join(reason.ChangedFiles, ", ")
				case "globalDependency":
					why = "global dependency: " + strings.Join(reason.ChangedFiles, ", ")
				case "workspaceManifestStructure":
					why = "workspace manifest deleted or renamed: " + strings.Join(reason.ChangedFiles, ", ")
				}
				fmt.Fprintln(out, "      why:", why)
			}
			if item.Shard != nil {
				fmt.Fprintf(out, "     (shard %d)\n", *item.Shard)
			}
		}
	}
	return nil
}
func writeAffectedPlan(result aff.Output, rows map[string][]map[string]json.RawMessage, runner, cwd, contextPath, outputPath, configPath string) (map[string]any, error) {
	var context pl.PlanContext
	if err := p.ReadArtifact(contextPath, 16*1024*1024, &context); err != nil {
		return nil, err
	}
	provenance := pl.PlanProvenance{Repository: context.Repository, Workflow: context.Workflow, RunID: context.RunID, ProducerAttempt: context.ProducerAttempt, PlanningJob: context.PlanningJob, Base: context.Base, Head: context.Head}
	if err := provenance.Validate(); err != nil {
		return nil, err
	}
	if context.TaskRunner != runner {
		return nil, fmt.Errorf("plan context taskRunner '%s' does not match resolved affected runner '%s'", context.TaskRunner, runner)
	}
	if !strings.EqualFold(context.Base, result.Diagnostics.Comparison.BaseCommit) || !strings.EqualFold(context.Head, result.Diagnostics.Comparison.HeadCommit) {
		return nil, fmt.Errorf("plan context base/head do not match the resolved affected commits")
	}
	gitRoot, err := aff.GitString(cwd, "rev-parse", "--show-toplevel")
	if err != nil {
		return nil, err
	}
	gitRoot, err = filepath.EvalSymlinks(gitRoot)
	if err != nil {
		return nil, err
	}
	relative, err := filepath.Rel(gitRoot, cwd)
	if err != nil {
		return nil, err
	}
	plan := pl.Plan{Version: pl.Version, Provenance: provenance, TaskRunner: runner, PredictionReason: context.PredictionReason, PredictionArtifact: context.PredictionArtifact, ModelArtifact: context.ModelArtifact, Groups: map[string]pl.PlanGroup{}}
	if relative != "." {
		directory := filepath.ToSlash(relative)
		if !pl.ValidRelativePath(directory) {
			return nil, fmt.Errorf("Plan working directory must be inside the repository")
		}
		plan.WorkingDirectory = &directory
	}
	relativeConfig, err := filepath.Rel(cwd, configPath)
	if err != nil {
		return nil, err
	}
	relativeConfig = filepath.ToSlash(relativeConfig)
	if !pl.ValidRelativePath(relativeConfig) {
		return nil, fmt.Errorf("Plan configuration must be inside the checkout")
	}
	if relativeConfig != "nanoom.config.json" {
		plan.ConfigPath = &relativeConfig
	}
	for name, groupRows := range rows {
		group := pl.PlanGroup{Assignments: []pl.PlanAssignment{}}
		for i, row := range groupRows {
			var assignment pl.PlanAssignment
			assignment.AssignmentID = fmt.Sprintf("%s-%04d", name, i+1)
			assignment.PredictionReason = "static assignment without prediction"
			if raw, ok := row["items"]; ok {
				if err := json.Unmarshal(raw, &assignment.Items); err != nil {
					return nil, err
				}
				if err := json.Unmarshal(row["assignmentId"], &assignment.AssignmentID); err != nil {
					return nil, fmt.Errorf("matrix assignment in group '%s' has no id", name)
				}
			} else {
				var item pl.PlanItem
				data, _ := json.Marshal(row)
				if err := json.Unmarshal(data, &item); err != nil {
					return nil, err
				}
				if item.Group == "" {
					item.Group = name
				}
				assignment.Items = []pl.PlanItem{item}
			}
			var checkout aff.Checkout
			if err := json.Unmarshal(row["checkout"], &checkout); err != nil {
				return nil, fmt.Errorf("matrix assignment in group '%s' has no checkout", name)
			}
			assignment.CheckoutPaths = []string{}
			if checkout.SparseCheckout != "" {
				assignment.CheckoutPaths = strings.Split(checkout.SparseCheckout, "\n")
			}
			data, _ := json.Marshal(row)
			var metadata struct {
				PredictedDurationMs         *uint64               `json:"predictedDurationMs"`
				PredictedPreparationMs      *uint64               `json:"predictedPreparationMs"`
				PredictionSources           *pl.PredictionSources `json:"predictionSources"`
				PreparationPredictionSource *string               `json:"preparationPredictionSource"`
				PreparationSampleCount      *uint64               `json:"preparationSampleCount"`
				SchedulingMode              *string               `json:"schedulingMode"`
				Reason                      *string               `json:"reason"`
				RunnerLabels                *[]string             `json:"runnerLabels"`
				TimingEnvironment           *string               `json:"timingEnvironment"`
			}
			if err := json.Unmarshal(data, &metadata); err != nil {
				return nil, err
			}
			assignment.PredictedDurationMs = metadata.PredictedDurationMs
			assignment.PredictedPreparationMs = metadata.PredictedPreparationMs
			assignment.PredictionSources = metadata.PredictionSources
			assignment.PreparationPredictionSource = metadata.PreparationPredictionSource
			assignment.PreparationSampleCount = metadata.PreparationSampleCount
			assignment.SchedulingMode = metadata.SchedulingMode
			assignment.RunnerLabels = metadata.RunnerLabels
			assignment.TimingEnvironment = metadata.TimingEnvironment
			if metadata.Reason != nil {
				assignment.PredictionReason = *metadata.Reason
			}
			if plan.WorkingDirectory != nil {
				prefix := *plan.WorkingDirectory
				for i, path := range assignment.CheckoutPaths {
					if path == "." {
						assignment.CheckoutPaths[i] = prefix
					} else {
						assignment.CheckoutPaths[i] = prefix + "/" + path
					}
				}
				for i, item := range assignment.Items {
					if item.Path == "." {
						assignment.Items[i].Path = prefix
					} else {
						assignment.Items[i].Path = prefix + "/" + item.Path
					}
				}
			}
			group.Assignments = append(group.Assignments, assignment)
			plan.AssignmentCount++
			plan.ItemCount += uint64(len(assignment.Items))
		}
		plan.Groups[name] = group
	}
	plan.HasChange = plan.ItemCount > 0
	if err := plan.Validate(); err != nil {
		return nil, err
	}
	bytes, err := json.MarshalIndent(plan, "", "  ")
	if err != nil {
		return nil, err
	}
	name, err := provenance.ArtifactName()
	if err != nil {
		return nil, err
	}
	reference := pl.PlanReference{Version: pl.Version, ArtifactName: name, SHA256: p.HexDigest(bytes), Provenance: provenance, Current: pl.CurrentRun{Repository: provenance.Repository, Workflow: provenance.Workflow, RunID: provenance.RunID, Attempt: provenance.ProducerAttempt, Base: provenance.Base, Head: provenance.Head}}
	compact, err := compactPlan(plan, reference)
	if err != nil {
		return nil, err
	}
	if err := writeFile(outputPath, bytes); err != nil {
		return nil, err
	}
	return compact, nil
}
func compactPlan(plan pl.Plan, reference pl.PlanReference) (map[string]any, error) {
	groups := map[string]any{}
	for name, group := range plan.Groups {
		if len(group.Assignments) > 256 {
			return nil, fmt.Errorf("group '%s' compact matrix has %d rows; maximum is 256", name, len(group.Assignments))
		}
		include := []map[string]any{}
		for _, assignment := range group.Assignments {
			row := map[string]any{"group": name, "assignmentId": assignment.AssignmentID, "checkout": map[string]string{"ref": plan.Provenance.Head, "sparseCheckout": pl.SparseCheckout(assignment.CheckoutPaths, plan.ConfigPath, plan.WorkingDirectory)}}
			display := ""
			if len(assignment.Items) == 1 {
				item := assignment.Items[0]
				shard := ""
				if item.Shard != nil {
					total := uint64(1)
					if item.TotalShards != nil {
						total = *item.TotalShards
					}
					shard = fmt.Sprintf(" · shard %d/%d", *item.Shard, total)
				}
				display = fmt.Sprintf("%s · %s%s · [%s]", item.Name, item.Task, shard, assignment.AssignmentID)
			} else {
				tasks := []string{}
				for _, item := range assignment.Items {
					tasks = append(tasks, item.Task)
				}
				tasks = w.Unique(tasks)
				sort.Strings(tasks)
				display = fmt.Sprintf("%s · %s · %d tasks · [%s]", name, strings.Join(tasks, "/"), len(assignment.Items), assignment.AssignmentID)
			}
			row["displayName"] = display
			if assignment.RunnerLabels != nil {
				row["runnerLabels"] = *assignment.RunnerLabels
			}
			if assignment.TimingEnvironment != nil {
				row["timingEnvironment"] = *assignment.TimingEnvironment
			}
			if assignment.PredictedDurationMs != nil {
				row["predictedDurationMs"] = *assignment.PredictedDurationMs
			}
			if assignment.PredictedPreparationMs != nil {
				row["predictedPreparationMs"] = *assignment.PredictedPreparationMs
			}
			if assignment.PredictionSources != nil {
				row["predictionSources"] = assignment.PredictionSources
			}
			if assignment.PreparationPredictionSource != nil {
				row["preparationPredictionSource"] = *assignment.PreparationPredictionSource
			}
			if assignment.PreparationSampleCount != nil {
				row["preparationSampleCount"] = *assignment.PreparationSampleCount
			}
			if assignment.SchedulingMode != nil {
				row["schedulingMode"] = *assignment.SchedulingMode
				row["predictionReason"] = assignment.PredictionReason
			}
			include = append(include, row)
		}
		groups[name] = map[string]any{"include": include}
	}
	compact := map[string]any{"has_change": plan.HasChange, "plan": reference, "groups": groups, "result": map[string]any{"status": "success", "hasChange": plan.HasChange, "groupCount": len(plan.Groups), "assignmentCount": plan.AssignmentCount, "itemCount": plan.ItemCount, "reason": plan.PredictionReason}}
	bytes, err := json.Marshal(compact)
	if err != nil {
		return nil, err
	}
	size := len(utf16.Encode([]rune(string(bytes))))*2 + 2
	if size > 1024*1024 {
		return nil, fmt.Errorf("compact Plan outputs exceed the 1 MiB UTF-16 limit: %d bytes", size)
	}
	return compact, nil
}
