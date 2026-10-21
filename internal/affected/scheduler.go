package affected

import (
	"encoding/json"
	"fmt"
	"math"
	"sort"
	"strings"

	pl "github.com/XionWCFM/nanoom/internal/plan"
	p "github.com/XionWCFM/nanoom/internal/prediction"
	w "github.com/XionWCFM/nanoom/internal/workspace"
)

type Checkout struct {
	ConeMode       bool   `json:"coneMode"`
	SparseCheckout string `json:"sparseCheckout"`
}
type Assignment struct {
	AssignmentID                string               `json:"assignmentId"`
	Items                       []Entry              `json:"items"`
	PredictedDurationMs         uint64               `json:"predictedDurationMs"`
	CheckoutPathCount           uint64               `json:"checkoutPathCount"`
	PredictionSources           pl.PredictionSources `json:"predictionSources"`
	Reason                      string               `json:"reason"`
	Checkout                    Checkout             `json:"checkout"`
	PredictedPreparationMs      *uint64              `json:"predictedPreparationMs,omitempty"`
	PreparationPredictionSource *string              `json:"preparationPredictionSource,omitempty"`
	PreparationSampleCount      *uint64              `json:"preparationSampleCount,omitempty"`
	SchedulingMode              *string              `json:"schedulingMode,omitempty"`
	RunnerLabels                *[]string            `json:"runnerLabels,omitempty"`
	TimingEnvironment           *string              `json:"timingEnvironment,omitempty"`
}
type forecast struct {
	duration, count uint64
	source          string
	uncertainty     *pl.EnvironmentUncertainty
}

func sortedUnique(values []string) []string {
	values = w.Unique(values)
	sort.Strings(values)
	return values
}
func checkout(paths []string) Checkout {
	return Checkout{true, strings.Join(sortedUnique(paths), "\n")}
}
func LabelsEnvironment(labels []string) string {
	labels = append([]string{}, labels...)
	sort.Strings(labels)
	b, _ := p.CanonicalBytes(labels)
	return "runner-labels:" + string(b)
}
func itemID(item Entry) string {
	var shard, total uint64
	if item.Shard != nil {
		shard = *item.Shard
	}
	if item.TotalShards != nil {
		total = *item.TotalShards
	}
	return fmt.Sprintf("%s:%s:%s:%d:%d", item.Group, item.Name, item.Task, shard, total)
}
func predict(item Entry, index p.Index, context *p.PredictionContext, runner, environment string, now uint64) forecast {
	key := map[string]any{"kind": "taskExact", "group": item.Group, "workspace": item.Name, "task": item.Task, "shard": item.Shard, "totalShards": item.TotalShards, "taskRunner": runner, "timingEnvironment": environment}
	for _, source := range []string{"exact", "group"} {
		id, err := p.Digest(key)
		if err == nil {
			if duration, count, ok := index.Estimate(context, item.Group, runner, environment, id, now); ok {
				f := forecast{duration: duration, count: count, source: source}
				profiles, low, high := index.EnvironmentRange(context, item.Group, runner, environment, id, now)
				if profiles > 1 {
					f.uncertainty = &pl.EnvironmentUncertainty{TaskCount: 1, MinimumTaskSumMs: low, MaximumTaskSumMs: high}
				}
				return f
			}
		}
		key["kind"] = "taskFallback"
		delete(key, "workspace")
	}
	return forecast{duration: 1, source: "cold"}
}
func assign(group string, items []Entry, concurrency uint64, index p.Index, context *p.PredictionContext, runner, environment string, labels *[]string, timing *string, now uint64) []Assignment {
	if len(items) == 0 {
		return []Assignment{}
	}
	if timing != nil {
		environment = *timing
	}
	type weighted struct {
		item       Entry
		prediction forecast
		id         string
	}
	weightedItems := make([]weighted, 0, len(items))
	for _, item := range items {
		weightedItems = append(weightedItems, weighted{item, predict(item, index, context, runner, environment, now), itemID(item)})
	}
	sort.Slice(weightedItems, func(i, j int) bool {
		a, b := weightedItems[i], weightedItems[j]
		if a.prediction.duration != b.prediction.duration {
			return a.prediction.duration > b.prediction.duration
		}
		return a.id < b.id
	})
	count := min(concurrency, uint64(len(items)))
	buckets := make([]Assignment, count)
	for i := range buckets {
		buckets[i] = Assignment{AssignmentID: fmt.Sprintf("%s-%d", group, i+1), Items: []Entry{}, Reason: "minimized predicted runtime makespan, then total sparse checkout paths", Checkout: checkout(nil), RunnerLabels: labels, TimingEnvironment: timing}
	}
	for _, row := range weightedItems {
		var checkoutTotal uint64
		for _, bucket := range buckets {
			checkoutTotal += bucket.CheckoutPathCount
		}
		best := -1
		var bestScore [3]uint64
		for i, bucket := range buckets {
			target := bucket.PredictedDurationMs + row.prediction.duration
			makespan := target
			for j, other := range buckets {
				if j != i {
					makespan = max(makespan, other.PredictedDurationMs)
				}
			}
			paths := append([]string{}, row.item.CheckoutPaths...)
			for _, item := range bucket.Items {
				paths = append(paths, item.CheckoutPaths...)
			}
			score := [3]uint64{makespan, checkoutTotal - bucket.CheckoutPathCount + uint64(len(w.Unique(paths))), target}
			less := best < 0
			for j := 0; best >= 0 && j < 3; j++ {
				if score[j] != bestScore[j] {
					less = score[j] < bestScore[j]
					break
				}
				if j == 2 {
					less = bucket.AssignmentID < buckets[best].AssignmentID
				}
			}
			if less {
				best = i
				bestScore = score
			}
		}
		bucket := &buckets[best]
		bucket.PredictedDurationMs += row.prediction.duration
		paths := append([]string{}, row.item.CheckoutPaths...)
		for _, item := range bucket.Items {
			paths = append(paths, item.CheckoutPaths...)
		}
		paths = sortedUnique(paths)
		bucket.Checkout = checkout(paths)
		bucket.CheckoutPathCount = uint64(len(paths))
		switch row.prediction.source {
		case "exact":
			bucket.PredictionSources.Exact++
		case "group":
			bucket.PredictionSources.Group++
		default:
			bucket.PredictionSources.Cold++
		}
		bucket.PredictionSources.SampleCount += row.prediction.count
		if u := row.prediction.uncertainty; u != nil {
			if bucket.PredictionSources.EnvironmentUncertainty == nil {
				bucket.PredictionSources.EnvironmentUncertainty = &pl.EnvironmentUncertainty{}
			}
			target := bucket.PredictionSources.EnvironmentUncertainty
			target.TaskCount++
			target.MinimumTaskSumMs += u.MinimumTaskSumMs
			target.MaximumTaskSumMs += u.MaximumTaskSumMs
		}
		bucket.Items = append(bucket.Items, row.item)
	}
	result := []Assignment{}
	for _, bucket := range buckets {
		if len(bucket.Items) > 0 {
			result = append(result, bucket)
		}
	}
	return result
}
func preparation(assignment *Assignment, group string, index p.Index, context *p.PredictionContext, prep *p.PreparationContext, runner, environment string, now uint64) bool {
	if context == nil || prep == nil || prep.Validate() != nil {
		return false
	}
	paths := []string{}
	for _, path := range strings.Split(assignment.Checkout.SparseCheckout, "\n") {
		if path != "" {
			paths = append(paths, path)
		}
	}
	names := []string{}
	for _, item := range assignment.Items {
		names = append(names, item.Name)
	}
	checkoutDigest, _ := p.Digest(sortedUnique(paths))
	workspaceDigest, _ := p.Digest(sortedUnique(names))
	key := map[string]any{"kind": "preparationExact", "group": group, "taskRunner": runner, "timingEnvironment": environment, "packageManager": prep.PackageManager, "packageManagerVersion": prep.PackageManagerVersion, "installMode": prep.InstallMode, "lockfileDigest": prep.LockfileDigest, "checkoutDigest": checkoutDigest, "workspaceSetDigest": workspaceDigest}
	for _, source := range []string{"exact", "group"} {
		id, err := p.Digest(key)
		if err == nil {
			if duration, count, ok := index.Estimate(context, group, runner, environment, id, now); ok {
				assignment.PredictedPreparationMs = &duration
				assignment.PreparationPredictionSource = new(string)
				*assignment.PreparationPredictionSource = source
				assignment.PreparationSampleCount = &count
				return true
			}
		}
		key["kind"] = "preparationFallback"
		delete(key, "checkoutDigest")
		delete(key, "workspaceSetDigest")
	}
	return false
}
func saturated(a, b uint64) uint64 {
	if a > math.MaxUint64-b {
		return math.MaxUint64
	}
	return a + b
}

type score struct {
	numbers [4]uint64
	stable  string
}

func assignmentScore(assignments []Assignment) score {
	s := score{}
	identities := []string{}
	for _, a := range assignments {
		duration := a.PredictedDurationMs
		if a.PredictedPreparationMs != nil {
			duration = saturated(duration, *a.PredictedPreparationMs)
		}
		s.numbers[0] = max(s.numbers[0], duration)
		s.numbers[1] = saturated(s.numbers[1], duration)
		s.numbers[2] += a.CheckoutPathCount
		ids := []string{}
		for _, item := range a.Items {
			ids = append(ids, itemID(item))
		}
		sort.Strings(ids)
		identities = append(identities, a.AssignmentID+"="+strings.Join(ids, ","))
	}
	s.numbers[3] = uint64(len(assignments))
	s.stable = strings.Join(identities, "|")
	return s
}
func scoreLess(a, b score) bool {
	for i := range a.numbers {
		if a.numbers[i] != b.numbers[i] {
			return a.numbers[i] < b.numbers[i]
		}
	}
	return a.stable < b.stable
}
func AssignAuto(group string, items []Entry, tier SelectedTier, index p.Index, context *p.PredictionContext, runner, environment string, labels *[]string, timing *string, prep *p.PreparationContext, now uint64) []Assignment {
	if len(items) == 0 {
		return []Assignment{}
	}
	cap := min(tier.Concurrency, uint64(len(items)))
	candidates := map[uint64]bool{1: true, cap: true}
	for power := uint64(2); power <= cap; power *= 2 {
		candidates[power] = true
		if power > math.MaxUint64/2 {
			break
		}
	}
	for _, candidate := range tier.Candidates {
		if candidate > 0 && candidate <= cap {
			candidates[candidate] = true
		}
	}
	counts := []uint64{}
	for candidate := range candidates {
		counts = append(counts, candidate)
	}
	sort.Slice(counts, func(i, j int) bool { return counts[i] < counts[j] })
	cold, allPreparation := false, prep != nil
	var best []Assignment
	var bestScore score
	preparationEnvironment := environment
	if timing != nil {
		preparationEnvironment = *timing
	}
	for _, count := range counts {
		assignments := assign(group, items, count, index, context, runner, environment, labels, timing, now)
		for i := range assignments {
			cold = cold || assignments[i].PredictionSources.Cold > 0
			known := preparation(&assignments[i], group, index, context, prep, runner, preparationEnvironment, now)
			allPreparation = allPreparation && known
		}
		score := assignmentScore(assignments)
		if best == nil || scoreLess(score, bestScore) {
			best = assignments
			bestScore = score
		}
	}
	if cold || !allPreparation {
		assignments := assign(group, items, cap, index, context, runner, environment, labels, timing, now)
		reason := "one or more candidate preparation estimates are unknown"
		if cold {
			reason = "task history is cold"
		}
		for i := range assignments {
			preparation(&assignments[i], group, index, context, prep, runner, preparationEnvironment, now)
			mode := "cold-cap"
			assignments[i].SchedulingMode = &mode
			assignments[i].Reason = fmt.Sprintf("cold-cap: retained selected concurrency %d; %s", cap, reason)
		}
		return assignments
	}
	for i := range best {
		mode := "automatic"
		best[i].SchedulingMode = &mode
		best[i].Reason = fmt.Sprintf("automatic concurrency: selected %d assignments by preparation + task makespan", len(best))
	}
	return best
}
func Matrix(output Output, index p.Index, context *p.PredictionContext, runner, environment string, prep *p.PreparationContext, now uint64) map[string]map[string]any {
	matrix := map[string]map[string]any{}
	for name, group := range output.Group {
		if tier := group.Distribution; tier != nil {
			labels := tier.RunnerLabels
			if labels == nil {
				labels = group.RunnerLabels
			}
			timing := tier.TimingEnvironment
			if timing == nil {
				timing = group.TimingEnvironment
			}
			matrix[name] = map[string]any{"include": AssignAuto(name, group.Workspaces, *tier, index, context, runner, environment, labels, timing, prep, now)}
			continue
		}
		include := []map[string]any{}
		for _, item := range group.Workspaces {
			row := map[string]any{"group": name, "label": group.Label, "name": item.Name, "path": item.Path, "task": item.Task, "checkout": checkout(item.CheckoutPaths), "checkoutPathCount": len(w.Unique(item.CheckoutPaths))}
			if item.Shard != nil {
				row["shard"] = *item.Shard
			}
			if item.TotalShards != nil {
				row["totalShards"] = *item.TotalShards
			}
			if group.RunnerLabels != nil {
				row["runnerLabels"] = *group.RunnerLabels
			}
			if group.TimingEnvironment != nil {
				row["timingEnvironment"] = *group.TimingEnvironment
			} else if group.RunnerLabels != nil {
				row["timingEnvironment"] = LabelsEnvironment(*group.RunnerLabels)
			}
			include = append(include, row)
		}
		matrix[name] = map[string]any{"include": include}
	}
	return matrix
}

// Rows preserves uint64 values while providing the public matrix object shape.
func Rows(matrix map[string]map[string]any) (map[string][]map[string]json.RawMessage, error) {
	b, err := json.Marshal(matrix)
	if err != nil {
		return nil, err
	}
	var groups map[string]struct {
		Include []map[string]json.RawMessage `json:"include"`
	}
	if err := json.Unmarshal(b, &groups); err != nil {
		return nil, err
	}
	out := map[string][]map[string]json.RawMessage{}
	for name, group := range groups {
		out[name] = group.Include
	}
	return out, nil
}
