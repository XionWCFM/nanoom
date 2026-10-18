package cli

import (
	"encoding/json"
	"fmt"
	"io"
	"path/filepath"
	"sort"
	"strconv"
	"time"

	p "github.com/XionWCFM/nanoom/internal/prediction"
)

func history(a arguments, out, diagnostics io.Writer) error {
	if err := a.allow("input", "previous-model", "previous-prediction", "previous-model-name", "model-output", "prediction-output", "model-artifact-name", "run-id", "run-attempt", "batch-output-dir", "now-ms"); err != nil {
		return err
	}
	if err := a.require("model-output", "prediction-output", "model-artifact-name", "run-id", "run-attempt"); err != nil {
		return err
	}
	if len(a.positionals) > 0 {
		return fmt.Errorf("unexpected history positional argument")
	}
	if a.has("previous-model") != a.has("previous-prediction") || a.has("previous-model") != a.has("previous-model-name") {
		return fmt.Errorf("--previous-model, --previous-prediction, and --previous-model-name must be supplied together")
	}
	attempt, err := strconv.ParseUint(a.get("run-attempt"), 10, 32)
	if err != nil {
		return err
	}
	now := uint64(time.Now().UnixMilli())
	if a.has("now-ms") {
		now, err = strconv.ParseUint(a.get("now-ms"), 10, 64)
		if err != nil {
			return err
		}
	}
	states := map[string]p.ModelState{}
	previousDegraded := false
	if a.has("previous-model") {
		var model p.ModelStateBundle
		var prediction p.PredictionArtifactBundle
		err := p.ReadArtifact(a.get("previous-model"), 16*1024*1024, &model)
		if err == nil {
			err = model.Validate()
		}
		if err == nil {
			err = p.ReadArtifact(a.get("previous-prediction"), 8*1024*1024, &prediction)
		}
		if err == nil {
			err = prediction.Validate()
		}
		if err == nil {
			digest, digestErr := p.Digest(model)
			err = digestErr
			if len(prediction.Predictions) == 0 {
				err = fmt.Errorf("previous prediction is empty")
			}
			for _, artifact := range prediction.Predictions {
				if artifact.ModelArtifact.Name != a.get("previous-model-name") || artifact.ModelArtifact.Sha256 != digest {
					err = fmt.Errorf("previous model artifact pointer did not match")
				}
			}
		}
		if err != nil {
			previousDegraded = true
			fmt.Fprintln(diagnostics, "previous history unavailable; bootstrapping from current observations:", err)
		} else {
			for _, state := range model.States {
				id, _ := state.Scope.ID()
				states[id] = state
			}
		}
	}
	groups := map[string][]p.MeasurementArtifact{}
	rejected := 0
	for _, path := range a.options["input"] {
		var measurement p.MeasurementArtifact
		err := p.ReadArtifact(path, 16*1024*1024, &measurement)
		if err == nil {
			err = measurement.Validate()
		}
		if err == nil && (measurement.RunID != a.get("run-id") || measurement.RunAttempt != uint32(attempt)) {
			err = fmt.Errorf("measurement run identity mismatch")
		}
		if err != nil {
			rejected++
			fmt.Fprintf(diagnostics, "MeasurementArtifact v3 unavailable; skipping %s: %v\n", path, err)
			continue
		}
		id, err := measurement.Scope.ID()
		if err != nil {
			return err
		}
		if len(groups[id]) > 0 && groups[id][0].Scope != measurement.Scope {
			return fmt.Errorf("scope ID collision while grouping measurements")
		}
		groups[id] = append(groups[id], measurement)
	}
	applied, duplicate, degraded, accepted, emitted := 0, 0, 0, 0, 0
	scopeIDs := []string{}
	for id := range groups {
		scopeIDs = append(scopeIDs, id)
	}
	sort.Strings(scopeIDs)
	for _, id := range scopeIDs {
		measurements := groups[id]
		var produced uint64
		ids := map[string]bool{}
		for _, m := range measurements {
			for _, o := range m.Observations {
				produced = max(produced, o.ObservedAtMs)
				ids[o.ExecutionID] = true
			}
			for _, o := range m.PreparationObservations {
				produced = max(produced, o.ObservedAtMs)
				ids[o.ExecutionID] = true
			}
		}
		batch, err := p.CompileMeasurements(measurements[0].Scope, a.get("run-id"), uint32(attempt), produced, measurements)
		if err != nil {
			degraded++
			fmt.Fprintf(diagnostics, "could not compile scope %s; retaining prior state: %v\n", id, err)
			continue
		}
		if a.has("batch-output-dir") {
			b, err := p.CanonicalBytes(batch)
			if err != nil {
				return err
			}
			if err := writeFile(filepath.Join(a.get("batch-output-dir"), id+".json"), b); err != nil {
				return err
			}
			emitted++
		}
		var prior *p.ModelState
		if value, ok := states[id]; ok {
			prior = &value
		}
		state, outcome, err := p.ApplyBatch(prior, batch, now)
		if err != nil {
			degraded++
			fmt.Fprintf(diagnostics, "could not apply scope %s; retaining prior state: %v\n", id, err)
			continue
		}
		states[id] = state
		if outcome.Duplicate {
			duplicate++
		} else {
			applied++
		}
		accepted += len(ids)
	}
	stateBundle := p.ModelStateBundle{Version: p.Version, States: []p.ModelState{}}
	scopeIDs = nil
	for id := range states {
		scopeIDs = append(scopeIDs, id)
	}
	sort.Strings(scopeIDs)
	for _, id := range scopeIDs {
		stateBundle.States = append(stateBundle.States, states[id])
	}
	if err := stateBundle.Validate(); err != nil {
		return err
	}
	modelBytes, err := p.CanonicalBytes(stateBundle)
	if err != nil {
		return err
	}
	if err := writeFile(a.get("model-output"), modelBytes); err != nil {
		return err
	}
	digest := p.HexDigest(modelBytes)
	bundle := p.PredictionArtifactBundle{Version: p.Version, Predictions: []p.PredictionArtifact{}}
	for _, state := range stateBundle.States {
		table, err := p.ProjectPredictions(state, now)
		if err != nil {
			return err
		}
		bundle.Predictions = append(bundle.Predictions, p.PredictionArtifact{Table: table, ModelArtifact: p.ArtifactReference{Name: a.get("model-artifact-name"), Sha256: digest}})
	}
	if err := bundle.Validate(); err != nil {
		return err
	}
	predictionBytes, err := p.CanonicalBytes(bundle)
	if err != nil {
		return err
	}
	if err := writeFile(a.get("prediction-output"), predictionBytes); err != nil {
		return err
	}
	status := "success"
	if rejected > 0 || degraded > 0 || previousDegraded {
		status = "degraded"
	}
	return json.NewEncoder(out).Encode(map[string]any{"status": status, "runId": a.get("run-id"), "runAttempt": attempt, "measurementFiles": len(a.options["input"]), "rejectedMeasurementFiles": rejected, "previousModelDegraded": previousDegraded, "scopeCount": len(stateBundle.States), "appliedBatchCount": applied, "duplicateBatchCount": duplicate, "emittedBatchCount": emitted, "degradedScopeCount": degraded, "acceptedObservationCount": accepted, "modelArtifactName": a.get("model-artifact-name"), "modelArtifactSha256": digest, "modelBytes": len(modelBytes), "predictionBytes": len(predictionBytes)})
}
