package prediction

import (
	"fmt"
	"sort"
)

func taskKeys(o SuccessfulObservation) ([]string, error) {
	key := map[string]any{"kind": "taskExact", "group": o.Group, "workspace": o.Workspace, "task": o.Task, "shard": o.Shard, "totalShards": o.TotalShards, "taskRunner": o.TaskRunner, "timingEnvironment": o.TimingEnvironment}
	exact, err := Digest(key)
	if err != nil {
		return nil, err
	}
	delete(key, "workspace")
	key["kind"] = "taskFallback"
	fallback, err := Digest(key)
	return []string{exact, fallback}, err
}
func preparationKeys(s Scope, o PreparationObservation) ([]string, error) {
	key := map[string]any{"kind": "preparationExact", "group": s.Group, "taskRunner": s.TaskRunner, "timingEnvironment": s.TimingEnvironment, "packageManager": o.PackageManager, "packageManagerVersion": o.PackageManagerVersion, "installMode": o.InstallMode, "lockfileDigest": o.LockfileDigest, "checkoutDigest": o.CheckoutDigest, "workspaceSetDigest": o.WorkspaceSetDigest}
	exact, err := Digest(key)
	if err != nil {
		return nil, err
	}
	delete(key, "checkoutDigest")
	delete(key, "workspaceSetDigest")
	key["kind"] = "preparationFallback"
	fallback, err := Digest(key)
	return []string{exact, fallback}, err
}

func CompileBatch(scope Scope, runID string, attempt uint32, producedAt uint64, observations []SuccessfulObservation, preparations []PreparationObservation) (ObservationBatch, error) {
	var zero ObservationBatch
	if err := scope.Validate(); err != nil {
		return zero, err
	}
	if !decimalPattern.MatchString(runID) || attempt == 0 {
		return zero, invalid("invalid run identity")
	}
	if len(observations)+len(preparations) == 0 {
		return zero, invalid("cannot compile an empty observation batch")
	}
	tasks := map[string]SuccessfulObservation{}
	prep := map[string]PreparationObservation{}
	for _, o := range observations {
		if err := o.validate(scope, producedAt); err != nil {
			return zero, err
		}
		if prior, ok := tasks[o.ExecutionID]; ok && !equal(prior, o) {
			return zero, invalid("one executionId has conflicting observations")
		}
		tasks[o.ExecutionID] = o
	}
	for _, o := range preparations {
		if err := o.validate(scope, producedAt); err != nil {
			return zero, err
		}
		if _, ok := tasks[o.ExecutionID]; ok {
			return zero, invalid("task and preparation executionIds must use separate identities")
		}
		if prior, ok := prep[o.ExecutionID]; ok && prior != o {
			return zero, invalid("one preparation executionId has conflicting observations")
		}
		prep[o.ExecutionID] = o
	}
	type keyDay struct {
		key string
		day uint64
	}
	rows := map[keyDay]AggregateObservation{}
	add := func(keys []string, at, duration uint64) error {
		for _, key := range keys {
			k := keyDay{key, at / DayMs}
			a := rows[k]
			a.KeyID = key
			a.UtcEpochDay = k.day
			a.ObservationCount++
			if a.TotalDurationMs > MaxSafeInteger-duration {
				return invalid("duration sum overflow")
			}
			a.TotalDurationMs += duration
			a.LastObservedAtMs = max(a.LastObservedAtMs, at)
			if a.ObservationCount > maxKeys {
				return invalid("compiled aggregate exceeds count limit")
			}
			rows[k] = a
		}
		return nil
	}
	for _, o := range tasks {
		keys, err := taskKeys(o)
		if err != nil {
			return zero, err
		}
		if err := add(keys, o.ObservedAtMs, o.DurationMs); err != nil {
			return zero, err
		}
	}
	for _, o := range prep {
		keys, err := preparationKeys(scope, o)
		if err != nil {
			return zero, err
		}
		if err := add(keys, o.ObservedAtMs, o.DurationMs); err != nil {
			return zero, err
		}
	}
	id, err := BatchID(scope, runID, attempt)
	if err != nil {
		return zero, err
	}
	batch := ObservationBatch{Version: Version, Scope: scope, RunID: runID, RunAttempt: attempt, ProducedAtMs: producedAt, BatchID: id, Aggregates: make([]AggregateObservation, 0, len(rows))}
	for _, row := range rows {
		batch.Aggregates = append(batch.Aggregates, row)
	}
	sort.Slice(batch.Aggregates, func(i, j int) bool {
		a, b := batch.Aggregates[i], batch.Aggregates[j]
		if a.KeyID != b.KeyID {
			return a.KeyID < b.KeyID
		}
		return a.UtcEpochDay < b.UtcEpochDay
	})
	return batch, batch.Validate(producedAt)
}

func CompileMeasurements(scope Scope, runID string, attempt uint32, producedAt uint64, measurements []MeasurementArtifact) (ObservationBatch, error) {
	type group struct {
		environment *RunnerEnvironment
		tasks       []SuccessfulObservation
		prep        []PreparationObservation
	}
	groups := map[string]*group{}
	identities := map[string]string{}
	var tasks []SuccessfulObservation
	var prep []PreparationObservation
	for _, m := range measurements {
		if err := m.Validate(); err != nil {
			return ObservationBatch{}, err
		}
		if m.Scope != scope || m.RunID != runID || m.RunAttempt != attempt {
			return ObservationBatch{}, invalid("measurement scope or run identity mismatch")
		}
		fingerprint := ""
		if m.RunnerEnvironment != nil {
			fingerprint = m.RunnerEnvironment.Fingerprint
		}
		ids := []string{}
		for _, o := range m.Observations {
			ids = append(ids, o.ExecutionID)
		}
		for _, o := range m.PreparationObservations {
			ids = append(ids, o.ExecutionID)
		}
		for _, id := range ids {
			if prior, ok := identities[id]; ok && prior != fingerprint {
				return ObservationBatch{}, invalid("executionId belongs to conflicting runner environments")
			}
			identities[id] = fingerprint
		}
		g, ok := groups[fingerprint]
		if !ok {
			g = &group{environment: m.RunnerEnvironment}
			groups[fingerprint] = g
		}
		if !equal(g.environment, m.RunnerEnvironment) {
			return ObservationBatch{}, invalid("runner environment fingerprint collision")
		}
		g.tasks = append(g.tasks, m.Observations...)
		g.prep = append(g.prep, m.PreparationObservations...)
		tasks = append(tasks, m.Observations...)
		prep = append(prep, m.PreparationObservations...)
	}
	batch, err := CompileBatch(scope, runID, attempt, producedAt, tasks, prep)
	if err != nil {
		return batch, err
	}
	batch.Aggregates = []AggregateObservation{}
	fingerprints := []string{}
	for key := range groups {
		fingerprints = append(fingerprints, key)
	}
	sort.Strings(fingerprints)
	for _, key := range fingerprints {
		g := groups[key]
		child, err := CompileBatch(scope, runID, attempt, producedAt, g.tasks, g.prep)
		if err != nil {
			return batch, err
		}
		if g.environment == nil {
			batch.Aggregates = child.Aggregates
		} else {
			batch.EnvironmentBatches = append(batch.EnvironmentBatches, EnvironmentBatch{*g.environment, &child})
		}
	}
	if err := batch.Validate(producedAt); err != nil {
		return batch, fmt.Errorf("compiled measurements: %w", err)
	}
	return batch, nil
}
