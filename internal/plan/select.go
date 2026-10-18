package plan

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/XionWCFM/nanoom/internal/prediction"
)

func Select(planPath, referencePath, group, id, output string) (AssignmentContext, error) {
	var zero AssignmentContext
	b, err := os.ReadFile(planPath)
	if err != nil {
		return zero, err
	}
	var p Plan
	if err := prediction.Decode(b, &p); err != nil {
		return zero, err
	}
	refBytes, err := os.ReadFile(referencePath)
	if err != nil {
		return zero, err
	}
	var reference PlanReference
	if err := prediction.Decode(refBytes, &reference); err != nil {
		return zero, err
	}
	if err := reference.Validate(p, b); err != nil {
		return zero, err
	}
	var assignment *PlanAssignment
	for _, candidate := range p.Groups[group].Assignments {
		if candidate.AssignmentID == id {
			assignment = &candidate
			break
		}
	}
	if assignment == nil {
		return zero, fmt.Errorf("Plan assignment '%s/%s' does not exist", group, id)
	}
	a := assignment
	context := AssignmentContext{Version: Version, PlanSHA256: reference.SHA256, Provenance: p.Provenance, Current: reference.Current, TaskRunner: p.TaskRunner, ConfigPath: p.ConfigPath, WorkingDirectory: p.WorkingDirectory, Group: group, AssignmentID: id, Items: a.Items, CheckoutPaths: a.CheckoutPaths, PredictedDurationMs: a.PredictedDurationMs, PredictedPreparationMs: a.PredictedPreparationMs, PredictionReason: a.PredictionReason, PreparationPredictionSource: a.PreparationPredictionSource, PreparationSampleCount: a.PreparationSampleCount, SchedulingMode: a.SchedulingMode, RunnerLabels: a.RunnerLabels, TimingEnvironment: a.TimingEnvironment}
	bytes, err := json.MarshalIndent(context, "", "  ")
	if err != nil {
		return zero, err
	}
	if err := os.MkdirAll(output, 0755); err != nil {
		return zero, err
	}
	if err := os.WriteFile(filepath.Join(output, "assignment.json"), bytes, 0644); err != nil {
		return zero, err
	}
	if err := os.WriteFile(filepath.Join(output, "paths.txt"), []byte(strings.Join(a.CheckoutPaths, "\n")+"\n"), 0644); err != nil {
		return zero, err
	}
	return context, nil
}
func SparseCheckout(paths []string, config, directory *string) string {
	patterns := []string{"/*", "!/*/"}
	for _, path := range paths {
		if path == "." {
			return "/*"
		}
	}
	escape := func(path string) string {
		var b strings.Builder
		b.WriteByte('/')
		for _, c := range path {
			if strings.ContainsRune("*?[] ", c) {
				b.WriteByte('\\')
			}
			b.WriteRune(c)
		}
		return b.String()
	}
	if directory != nil {
		prefix := escape(*directory)
		patterns = append(patterns, prefix+"/*", "!"+prefix+"/*/")
	}
	for _, path := range paths {
		patterns = append(patterns, escape(path))
	}
	if config != nil {
		path := *config
		if directory != nil {
			path = *directory + "/" + path
		}
		patterns = append(patterns, escape(path))
	}
	return strings.Join(patterns, "\n")
}
