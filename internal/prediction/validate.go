package prediction

import (
	"fmt"
	"reflect"
	"regexp"
	"strings"
	"unicode"
)

const (
	Version                   = 3
	DayMs              uint64 = 86_400_000
	MaxSafeInteger     uint64 = 9_007_199_254_740_991
	maxDuration        uint64 = 604_800_000
	futureTolerance    uint64 = 300_000
	batchMaxAge               = 7 * DayMs
	maxKeys                   = 50_000
	maxModelBytes             = 16 * 1024 * 1024
	maxPredictionBytes        = 8 * 1024 * 1024
)

var repositoryPattern = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,63}$`)
var digestPattern = regexp.MustCompile(`^[a-f0-9]{64}$`)
var decimalPattern = regexp.MustCompile(`^[1-9][0-9]{0,19}$`)

func invalid(message string) error     { return fmt.Errorf("%s", message) }
func textValid(s string, max int) bool { return len(s) > 0 && len(s) <= max && !hasControl(s) }
func hasControl(s string) bool {
	for _, c := range s {
		if unicode.IsControl(c) {
			return true
		}
	}
	return false
}
func sub(a, b uint64) uint64 {
	if b > a {
		return 0
	}
	return a - b
}
func sizeValid(v any, max int) error {
	b, err := CanonicalBytes(v)
	if err != nil {
		return err
	}
	if len(b) > max {
		return invalid("artifact exceeds byte limit")
	}
	return nil
}
func branchValid(s string) bool {
	if !strings.HasPrefix(s, "refs/heads/") || len(s) <= 11 || len(s) > 512 || hasControl(s) || strings.Contains(s, " ") || strings.Contains(s, "..") || strings.Contains(s, "@{") || strings.HasSuffix(s, "/") || strings.HasSuffix(s, ".") || strings.HasSuffix(s, ".lock") {
		return false
	}
	for _, part := range strings.Split(s, "/") {
		if part == "" || strings.HasPrefix(part, ".") || strings.HasSuffix(part, ".") {
			return false
		}
	}
	return true
}
func (s Scope) Validate() error {
	if !repositoryPattern.MatchString(s.RepositoryKey) {
		return invalid("invalid repositoryKey")
	}
	file, ok := strings.CutPrefix(s.WorkflowPath, ".github/workflows/")
	if !ok || file == "" || strings.Contains(file, "/") || hasControl(s.WorkflowPath) || !(strings.HasSuffix(file, ".yml") || strings.HasSuffix(file, ".yaml")) {
		return invalid("invalid workflowPath")
	}
	if !textValid(s.Group, 128) || !textValid(s.TaskRunner, 32) || !textValid(s.TimingEnvironment, 256) {
		return invalid("invalid scope field")
	}
	switch s.TaskRunner {
	case "pnpm", "yarn", "npm", "nx", "turbo":
	default:
		return invalid("unsupported taskRunner")
	}
	switch s.Ref.Kind {
	case "push":
		if !branchValid(s.Ref.Ref) {
			return invalid("invalid Git branch ref")
		}
	case "pull_request":
		if s.Ref.Number == 0 || s.Ref.Number > MaxSafeInteger || len(s.Ref.HeadRepositoryID) == 0 || len(s.Ref.HeadRepositoryID) > 20 || strings.IndexFunc(s.Ref.HeadRepositoryID, func(c rune) bool { return c < '0' || c > '9' }) >= 0 {
			return invalid("invalid pull request identity")
		}
		if !branchValid(s.Ref.HeadRef) || !branchValid(s.Ref.BaseRef) {
			return invalid("invalid Git branch ref")
		}
	default:
		return invalid("invalid scope ref")
	}
	return nil
}
func (s Scope) ID() (string, error) {
	if err := s.Validate(); err != nil {
		return "", err
	}
	return Digest(s)
}
func BatchID(scope Scope, runID string, attempt uint32) (string, error) {
	id, err := scope.ID()
	if err != nil {
		return "", err
	}
	return Digest([]any{id, runID, attempt})
}
func (e RunnerEnvironment) Validate() error {
	p := e.Profile
	if p.Version != 1 || p.AvailableCPUs == 0 || p.AvailableCPUs > MaxSafeInteger || p.MemoryMiB == 0 || p.MemoryMiB > MaxSafeInteger {
		return invalid("invalid runner environment capacity")
	}
	for _, n := range []*uint64{p.CPUQuotaMilli, p.MemoryLimitMiB} {
		if n != nil && (*n == 0 || *n > MaxSafeInteger) {
			return invalid("invalid runner environment capacity")
		}
	}
	if p.ContainerLimits != "cgroup-v2" && p.ContainerLimits != "unknown" {
		return invalid("invalid container limits")
	}
	for _, s := range []string{p.Os, p.Arch, p.CPUModel, p.ContainerLimits, p.NodeVersion, p.PackageManager, p.PackageManagerVersion} {
		if !textValid(s, 256) {
			return invalid("invalid runner environment field")
		}
	}
	id, err := Digest(p)
	if err != nil {
		return err
	}
	if len(p.Image) > 256 || hasControl(p.Image) || id != e.Fingerprint {
		return invalid("runner environment fingerprint does not match its profile")
	}
	return nil
}
func (c PreparationContext) Validate() error {
	if c.PackageManager != "pnpm" && c.PackageManager != "yarn" && c.PackageManager != "npm" {
		return invalid("invalid preparation context")
	}
	if !textValid(c.PackageManagerVersion, 128) || !textValid(c.InstallMode, 32) || !digestPattern.MatchString(c.LockfileDigest) {
		return invalid("invalid preparation context")
	}
	return nil
}
func validTime(at, now uint64) bool {
	return at <= MaxSafeInteger && at <= now+futureTolerance && sub(now, at) < batchMaxAge
}
func (o SuccessfulObservation) validate(s Scope, now uint64) error {
	if !textValid(o.ExecutionID, 1024) || o.Group != s.Group || o.TaskRunner != s.TaskRunner || o.TimingEnvironment != s.TimingEnvironment || o.Workspace == "" || o.Task == "" || hasControl(o.Workspace) || hasControl(o.Task) || o.DurationMs == 0 || o.DurationMs > maxDuration || !validTime(o.ObservedAtMs, now) {
		return invalid("invalid or expired successful observation")
	}
	if o.Shard == nil && o.TotalShards == nil {
		return nil
	}
	if o.Shard == nil || o.TotalShards == nil || *o.Shard == 0 || *o.Shard > *o.TotalShards || *o.TotalShards > MaxSafeInteger {
		return invalid("invalid shard identity")
	}
	return nil
}
func (o PreparationObservation) validate(s Scope, now uint64) error {
	if !textValid(o.ExecutionID, 1024) || !validTime(o.ObservedAtMs, now) || o.DurationMs > maxDuration || !digestPattern.MatchString(o.CheckoutDigest) || !digestPattern.MatchString(o.WorkspaceSetDigest) {
		return invalid("invalid or expired preparation observation")
	}
	if err := s.Validate(); err != nil {
		return err
	}
	return (PreparationContext{o.PackageManager, o.PackageManagerVersion, o.InstallMode, o.LockfileDigest}).Validate()
}
func (m MeasurementArtifact) Validate() error {
	if m.Version != Version || !decimalPattern.MatchString(m.RunID) || m.RunAttempt == 0 || len(m.Observations)+len(m.PreparationObservations) == 0 || len(m.Observations)+len(m.PreparationObservations) > maxKeys {
		return invalid("invalid or empty v3 MeasurementArtifact")
	}
	if err := m.Scope.Validate(); err != nil {
		return err
	}
	if m.RunnerEnvironment != nil {
		if err := m.RunnerEnvironment.Validate(); err != nil {
			return err
		}
	}
	var now uint64
	for _, o := range m.Observations {
		now = max(now, o.ObservedAtMs)
	}
	for _, o := range m.PreparationObservations {
		now = max(now, o.ObservedAtMs)
	}
	ids := map[string]bool{}
	for _, o := range m.Observations {
		if err := o.validate(m.Scope, now); err != nil {
			return err
		}
		if ids[o.ExecutionID] {
			return invalid("MeasurementArtifact contains duplicate executionIds")
		}
		ids[o.ExecutionID] = true
	}
	for _, o := range m.PreparationObservations {
		if err := o.validate(m.Scope, now); err != nil {
			return err
		}
		if ids[o.ExecutionID] {
			return invalid("MeasurementArtifact contains duplicate executionIds")
		}
		ids[o.ExecutionID] = true
	}
	return sizeValid(m, maxModelBytes)
}
func (a AggregateObservation) validate(now uint64) error {
	if !digestPattern.MatchString(a.KeyID) || a.UtcEpochDay > 3_652_059 || a.ObservationCount == 0 || a.ObservationCount > maxKeys || a.TotalDurationMs > MaxSafeInteger || a.TotalDurationMs > a.ObservationCount*maxDuration || a.LastObservedAtMs > MaxSafeInteger || a.LastObservedAtMs > now+futureTolerance || a.LastObservedAtMs/DayMs != a.UtcEpochDay {
		return invalid("invalid aggregate row")
	}
	return nil
}
func (b ObservationBatch) Validate(now uint64) error { return b.validate(now, false) }
func (b ObservationBatch) validate(now uint64, child bool) error {
	if b.Version != Version || !decimalPattern.MatchString(b.RunID) || b.RunAttempt == 0 {
		return invalid("invalid ObservationBatch identity or version")
	}
	if err := b.Scope.Validate(); err != nil {
		return err
	}
	if !validTime(b.ProducedAtMs, now) {
		return invalid("ObservationBatch is expired or from the future")
	}
	id, err := BatchID(b.Scope, b.RunID, b.RunAttempt)
	if err != nil {
		return err
	}
	if id != b.BatchID {
		return invalid("ObservationBatch batchId does not match scope/run/attempt")
	}
	if len(b.Aggregates)+len(b.EnvironmentBatches) == 0 || len(b.Aggregates) > maxKeys || len(b.EnvironmentBatches) > 32 || (child && len(b.EnvironmentBatches) > 0) {
		return invalid("ObservationBatch count is outside limits")
	}
	var prev AggregateObservation
	for i, a := range b.Aggregates {
		if err := a.validate(now); err != nil {
			return err
		}
		if i > 0 && (prev.KeyID > a.KeyID || (prev.KeyID == a.KeyID && prev.UtcEpochDay >= a.UtcEpochDay)) {
			return invalid("aggregates must have unique sorted key/day rows")
		}
		prev = a
	}
	count := len(b.Aggregates)
	last := ""
	for _, e := range b.EnvironmentBatches {
		if err := e.Environment.Validate(); err != nil {
			return err
		}
		if e.Environment.Fingerprint <= last || e.Batch == nil || e.Batch.Scope != b.Scope || e.Batch.RunID != b.RunID || e.Batch.RunAttempt != b.RunAttempt || e.Batch.ProducedAtMs != b.ProducedAtMs {
			return invalid("invalid or unsorted environment batches")
		}
		if err := e.Batch.validate(now, true); err != nil {
			return err
		}
		count += len(e.Batch.Aggregates)
		last = e.Environment.Fingerprint
	}
	if count > maxKeys {
		return invalid("environment batch rows exceed 50000")
	}
	return sizeValid(b, maxModelBytes)
}
func (s ModelState) Validate() error { return s.validate(false) }
func (s ModelState) validate(child bool) error {
	if s.Version != Version {
		return invalid("unsupported ModelState version")
	}
	if err := s.Scope.Validate(); err != nil {
		return err
	}
	if s.UpdatedAtMs > MaxSafeInteger || s.PruningDay > 3_652_059 || s.BatchAcceptanceAfterMs > MaxSafeInteger || len(s.Entries) > maxKeys || len(s.Receipts) > 4096 || len(s.EnvironmentStates) > 32 || (child && len(s.EnvironmentStates) > 0) {
		return invalid("ModelState exceeds a field or collection limit")
	}
	last := ""
	count := len(s.Entries)
	for _, e := range s.EnvironmentStates {
		if err := e.Environment.Validate(); err != nil {
			return err
		}
		if e.Environment.Fingerprint <= last || e.State == nil || e.State.Scope != s.Scope {
			return invalid("invalid or unsorted environment states")
		}
		if err := e.State.validate(true); err != nil {
			return err
		}
		count += len(e.State.Entries)
		last = e.Environment.Fingerprint
	}
	if count > maxKeys {
		return invalid("environment model keys exceed 50000")
	}
	last = ""
	for _, e := range s.Entries {
		if !digestPattern.MatchString(e.KeyID) || e.KeyID <= last || len(e.Buckets) == 0 || len(e.Buckets) > 7 || len(e.RecentBatches) > 3 {
			return invalid("invalid ModelState entry")
		}
		last = e.KeyID
		for i, b := range e.Buckets {
			if b[0] > 3_652_059 || b[1] == 0 || b[1] > MaxSafeInteger || b[2] > MaxSafeInteger || b[2]/b[1] > maxDuration || (b[2]/b[1] == maxDuration && b[2]%b[1] > 0) || b[3] > MaxSafeInteger || b[3]/DayMs != b[0] || b[0] > s.PruningDay || b[0] < sub(s.PruningDay, 29) || (i > 0 && e.Buckets[i-1][0] >= b[0]) {
				return invalid("invalid model day bucket")
			}
		}
		for i, r := range e.RecentBatches {
			found := false
			for _, b := range e.Buckets {
				if b[0] == r.ObservedAtMs/DayMs && r.ObservedAtMs <= b[3] {
					found = true
				}
			}
			if !decimalPattern.MatchString(r.RunID) || r.RunAttempt == 0 || r.ObservedAtMs > MaxSafeInteger || r.MeanMs > maxDuration || !found || (i > 0 && !recentLess(e.RecentBatches[i-1], r)) {
				return invalid("ModelState recent batches are invalid or unsorted")
			}
		}
		p, ok := derivePrediction(e.Buckets, e.RecentBatches)
		if !ok || p != e.Prediction {
			return invalid("ModelState cached prediction does not match its buckets")
		}
	}
	last = ""
	for _, r := range s.Receipts {
		if !digestPattern.MatchString(r.BatchID) || !digestPattern.MatchString(r.BodyDigest) || r.BatchID <= last || r.ProducedAtMs > MaxSafeInteger {
			return invalid("invalid ModelState receipt")
		}
		last = r.BatchID
	}
	return sizeValid(s, maxModelBytes)
}
func (t PredictionTable) Validate() error { return t.validate(false) }
func (t PredictionTable) validate(child bool) error {
	if t.Version != Version || t.ModelUpdatedAtMs > MaxSafeInteger || len(t.Rows) > maxKeys || len(t.EnvironmentPredictions) > 32 || (child && len(t.EnvironmentPredictions) > 0) {
		return invalid("invalid PredictionTable")
	}
	if err := t.Scope.Validate(); err != nil {
		return err
	}
	last := ""
	for _, r := range t.Rows {
		if !digestPattern.MatchString(r.KeyID) || r.KeyID <= last || r.EstimatedMs > MaxSafeInteger || r.ObservationCount == 0 || r.ObservationCount > MaxSafeInteger || r.LastObservedAtMs > MaxSafeInteger || r.ValidUntilMs > MaxSafeInteger || r.ValidUntilMs <= r.LastObservedAtMs {
			return invalid("invalid or unsorted PredictionTable row")
		}
		last = r.KeyID
	}
	last = ""
	count := len(t.Rows)
	for _, e := range t.EnvironmentPredictions {
		if err := e.Environment.Validate(); err != nil {
			return err
		}
		if e.Environment.Fingerprint <= last || e.Table == nil || e.Table.Scope != t.Scope {
			return invalid("invalid environment predictions")
		}
		if err := e.Table.validate(true); err != nil {
			return err
		}
		count += len(e.Table.Rows)
		last = e.Environment.Fingerprint
	}
	if count > maxKeys {
		return invalid("environment prediction rows exceed 50000")
	}
	return sizeValid(t, maxPredictionBytes)
}
func (t PredictionTable) Estimate(key string, now uint64) (uint64, uint64, bool) {
	for _, r := range t.Rows {
		if r.KeyID == key && now < r.ValidUntilMs {
			return r.EstimatedMs, r.ObservationCount, true
		}
	}
	return 0, 0, false
}
func equal(a, b any) bool { return reflect.DeepEqual(a, b) }
