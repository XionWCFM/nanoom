package prediction

import (
	"math"
	"math/big"
	"sort"
)

type ApplyOutcome struct {
	Duplicate     bool
	PrunedBuckets uint64
}

func recentLess(a, b RecentBatch) bool {
	if a.ObservedAtMs != b.ObservedAtMs {
		return a.ObservedAtMs < b.ObservedAtMs
	}
	if a.RunID != b.RunID {
		return a.RunID < b.RunID
	}
	return a.RunAttempt < b.RunAttempt
}
func derivePrediction(buckets []DayBucket, recent []RecentBatch) (ModelPrediction, bool) {
	var p ModelPrediction
	if len(buckets) == 0 {
		return p, false
	}
	latest := buckets[len(buckets)-1][0]
	var numerator, denominator float64
	for _, b := range buckets {
		if p[1] > MaxSafeInteger-b[1] {
			return p, false
		}
		p[1] += b[1]
		p[2] = max(p[2], b[3])
		weight := math.Pow(2, -float64(latest-b[0])/7)
		numerator += float64(b[2]) * weight
		denominator += float64(b[1]) * weight
	}
	if denominator == 0 {
		return p, false
	}
	estimate := math.Floor(numerator/denominator + 0.5)
	if math.IsNaN(estimate) || math.IsInf(estimate, 0) || estimate < 0 || estimate > float64(MaxSafeInteger) {
		return p, false
	}
	p[0] = uint64(estimate)
	p[3] = (buckets[0][0] + 30) * DayMs
	if len(recent) > 0 {
		last := recent[len(recent)-1].ObservedAtMs
		durations := []uint64{}
		for _, r := range recent {
			if sub(last, r.ObservedAtMs) <= 7*DayMs {
				durations = append(durations, r.MeanMs)
			}
		}
		sort.Slice(durations, func(i, j int) bool { return durations[i] < durations[j] })
		middle := len(durations) / 2
		p[0] = durations[middle]
		if len(durations)%2 == 0 {
			a, b := durations[middle-1], durations[middle]
			p[0] = a + (b-a+1)/2
		}
	}
	return p, true
}

// ApplyBatch returns a new state; the caller's prior state is untouched on both
// success and failure, so a failed merge cannot leak partial writes.
func ApplyBatch(previous *ModelState, batch ObservationBatch, now uint64) (ModelState, ApplyOutcome, error) {
	var out ApplyOutcome
	zero := ModelState{}
	if err := batch.Validate(now); err != nil {
		return zero, out, err
	}
	digest, err := Digest(batch)
	if err != nil {
		return zero, out, err
	}
	today := now / DayMs
	acceptance := sub(now, batchMaxAge) + 1
	state := ModelState{Version: Version, Scope: batch.Scope, UpdatedAtMs: now, PruningDay: today, BatchAcceptanceAfterMs: acceptance, Entries: []ModelEntry{}, Receipts: []BatchReceipt{}}
	if previous != nil {
		if err := previous.Validate(); err != nil {
			return zero, out, err
		}
		state = *previous
	}
	if state.Scope != batch.Scope {
		return zero, out, invalid("ObservationBatch scope does not match ModelState")
	}
	for _, r := range state.Receipts {
		if r.BatchID == batch.BatchID {
			if r.BodyDigest != digest {
				return zero, out, invalid("batchId already exists with a different body digest")
			}
			out.Duplicate = true
			return state, out, nil
		}
	}
	if batch.ProducedAtMs < state.BatchAcceptanceAfterMs {
		return zero, out, invalid("ObservationBatch is older than the model acceptance watermark")
	}
	effective := max(today, state.PruningDay)
	for _, a := range batch.Aggregates {
		effective = max(effective, a.UtcEpochDay)
	}
	cutoff := sub(effective, 29)
	byKey := map[string]map[uint64]DayBucket{}
	recents := map[string][]RecentBatch{}
	for _, e := range state.Entries {
		recents[e.KeyID] = append([]RecentBatch{}, e.RecentBatches...)
		for _, b := range e.Buckets {
			if b[0] >= cutoff && b[0] <= effective {
				if byKey[e.KeyID] == nil {
					byKey[e.KeyID] = map[uint64]DayBucket{}
				}
				byKey[e.KeyID][b[0]] = b
			} else {
				out.PrunedBuckets++
			}
		}
	}
	for _, a := range batch.Aggregates {
		if a.UtcEpochDay < cutoff || a.UtcEpochDay > effective {
			continue
		}
		if byKey[a.KeyID] == nil {
			byKey[a.KeyID] = map[uint64]DayBucket{}
		}
		b := byKey[a.KeyID][a.UtcEpochDay]
		b[0] = a.UtcEpochDay
		if b[1] > MaxSafeInteger-a.ObservationCount || b[2] > MaxSafeInteger-a.TotalDurationMs {
			return zero, out, invalid("model aggregate overflow")
		}
		b[1] += a.ObservationCount
		b[2] += a.TotalDurationMs
		b[3] = max(b[3], a.LastObservedAtMs)
		byKey[a.KeyID][a.UtcEpochDay] = b
		mean := a.TotalDurationMs / a.ObservationCount
		if a.TotalDurationMs%a.ObservationCount >= (a.ObservationCount+1)/2 {
			mean++
		}
		recents[a.KeyID] = append(recents[a.KeyID], RecentBatch{a.LastObservedAtMs, batch.RunID, batch.RunAttempt, mean})
	}
	entries := []ModelEntry{}
	for key, days := range byKey {
		buckets := make([]DayBucket, 0, len(days))
		for _, b := range days {
			buckets = append(buckets, b)
		}
		sort.Slice(buckets, func(i, j int) bool { return buckets[i][0] < buckets[j][0] })
		if len(buckets) > 7 {
			out.PrunedBuckets += uint64(len(buckets) - 7)
			buckets = buckets[len(buckets)-7:]
		}
		recent := filterRecent(recents[key], buckets)
		sort.Slice(recent, func(i, j int) bool { return recentLess(recent[i], recent[j]) })
		if len(recent) > 3 {
			recent = recent[len(recent)-3:]
		}
		if p, ok := derivePrediction(buckets, recent); ok {
			entries = append(entries, ModelEntry{key, buckets, recent, p})
		}
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].KeyID < entries[j].KeyID })
	if len(entries) > maxKeys {
		return zero, out, invalid("ModelState exceeds 50000 keys")
	}
	receipts := []BatchReceipt{}
	for _, r := range state.Receipts {
		if sub(now, r.ProducedAtMs) < 8*DayMs {
			receipts = append(receipts, r)
		}
	}
	if len(receipts) >= 4096 {
		return zero, out, invalid("ModelState receipt capacity exceeded")
	}
	receipts = append(receipts, BatchReceipt{batch.BatchID, digest, batch.ProducedAtMs})
	sort.Slice(receipts, func(i, j int) bool { return receipts[i].BatchID < receipts[j].BatchID })
	state.Version = Version
	state.UpdatedAtMs = max(state.UpdatedAtMs, now)
	state.PruningDay = effective
	state.BatchAcceptanceAfterMs = max(state.BatchAcceptanceAfterMs, acceptance)
	state.Entries = entries
	state.Receipts = receipts
	environments := map[string]EnvironmentState{}
	for _, e := range state.EnvironmentStates {
		environments[e.Environment.Fingerprint] = e
	}
	for _, incoming := range batch.EnvironmentBatches {
		var prior *ModelState
		if old, ok := environments[incoming.Environment.Fingerprint]; ok {
			if !equal(old.Environment, incoming.Environment) {
				return zero, out, invalid("runner environment fingerprint collision")
			}
			prior = old.State
		}
		child, result, err := ApplyBatch(prior, *incoming.Batch, now)
		if err != nil {
			return zero, out, err
		}
		out.PrunedBuckets += result.PrunedBuckets
		environments[incoming.Environment.Fingerprint] = EnvironmentState{incoming.Environment, &child}
	}
	state.EnvironmentStates = nil
	for _, child := range environments {
		cleaned := *child.State
		childToday := max(cleaned.PruningDay, today)
		childCutoff := sub(childToday, 29)
		cleaned.Entries = []ModelEntry{}
		for _, entry := range child.State.Entries {
			buckets := []DayBucket{}
			for _, b := range entry.Buckets {
				if b[0] >= childCutoff && b[0] <= childToday {
					buckets = append(buckets, b)
				} else {
					out.PrunedBuckets++
				}
			}
			recent := filterRecent(entry.RecentBatches, buckets)
			if p, ok := derivePrediction(buckets, recent); ok {
				cleaned.Entries = append(cleaned.Entries, ModelEntry{entry.KeyID, buckets, recent, p})
			}
		}
		cleaned.Receipts = []BatchReceipt{}
		for _, r := range child.State.Receipts {
			if sub(now, r.ProducedAtMs) < 8*DayMs {
				cleaned.Receipts = append(cleaned.Receipts, r)
			}
		}
		cleaned.PruningDay = childToday
		cleaned.UpdatedAtMs = max(cleaned.UpdatedAtMs, now)
		cleaned.BatchAcceptanceAfterMs = max(cleaned.BatchAcceptanceAfterMs, acceptance)
		if len(cleaned.Entries) > 0 {
			state.EnvironmentStates = append(state.EnvironmentStates, EnvironmentState{child.Environment, &cleaned})
		}
	}
	sort.Slice(state.EnvironmentStates, func(i, j int) bool {
		return state.EnvironmentStates[i].Environment.Fingerprint < state.EnvironmentStates[j].Environment.Fingerprint
	})
	return state, out, state.Validate()
}
func filterRecent(recent []RecentBatch, buckets []DayBucket) []RecentBatch {
	out := []RecentBatch{}
	for _, r := range recent {
		for _, b := range buckets {
			if b[0] == r.ObservedAtMs/DayMs {
				out = append(out, r)
				break
			}
		}
	}
	return out
}

func ProjectPredictions(state ModelState, now uint64) (PredictionTable, error) {
	var zero PredictionTable
	if err := state.Validate(); err != nil {
		return zero, err
	}
	today := now / DayMs
	cutoff := sub(today, 29)
	rows := map[string]PredictionRow{}
	latest := map[string]uint64{}
	for _, e := range state.Entries {
		buckets := []DayBucket{}
		for _, b := range e.Buckets {
			if b[0] >= cutoff && b[0] <= today {
				buckets = append(buckets, b)
			}
		}
		if p, ok := derivePrediction(buckets, filterRecent(e.RecentBatches, buckets)); ok && now < p[3] {
			rows[e.KeyID] = PredictionRow{e.KeyID, p[0], p[1], p[2], p[3]}
			latest[e.KeyID] = p[2]
		}
	}
	children := []EnvironmentPrediction{}
	for _, child := range state.EnvironmentStates {
		table, err := ProjectPredictions(*child.State, now)
		if err != nil {
			return zero, err
		}
		if len(table.Rows) == 0 {
			continue
		}
		for _, r := range table.Rows {
			latest[r.KeyID] = max(latest[r.KeyID], r.LastObservedAtMs)
		}
		children = append(children, EnvironmentPrediction{child.Environment, &table})
	}
	type pool struct {
		total               *big.Int
		count, last, expiry uint64
	}
	pooled := map[string]*pool{}
	for _, child := range children {
		var model *ModelState
		for _, s := range state.EnvironmentStates {
			if s.Environment.Fingerprint == child.Environment.Fingerprint {
				model = s.State
				break
			}
		}
		for _, r := range child.Table.Rows {
			cutoff := sub(latest[r.KeyID], 7*DayMs)
			if r.LastObservedAtMs < cutoff {
				continue
			}
			var count uint64
			for _, e := range model.Entries {
				if e.KeyID == r.KeyID {
					for _, b := range e.Buckets {
						if b[3] >= cutoff {
							count += b[1]
						}
					}
					break
				}
			}
			p := pooled[r.KeyID]
			if p == nil {
				p = &pool{total: new(big.Int), expiry: math.MaxUint64}
				pooled[r.KeyID] = p
			}
			if count > MaxSafeInteger || p.count > MaxSafeInteger-count {
				return zero, invalid("pooled observation count overflow")
			}
			p.total.Add(p.total, new(big.Int).Mul(new(big.Int).SetUint64(r.EstimatedMs), new(big.Int).SetUint64(count)))
			p.count += count
			p.last = max(p.last, r.LastObservedAtMs)
			p.expiry = min(p.expiry, r.ValidUntilMs)
		}
	}
	for key, p := range pooled {
		if p.count == 0 {
			return zero, invalid("empty prediction pool")
		}
		estimate := new(big.Int).Add(p.total, new(big.Int).SetUint64(p.count/2))
		estimate.Div(estimate, new(big.Int).SetUint64(p.count))
		rows[key] = PredictionRow{key, estimate.Uint64(), p.count, p.last, p.expiry}
	}
	table := PredictionTable{Version: Version, Scope: state.Scope, ModelUpdatedAtMs: state.UpdatedAtMs, Rows: make([]PredictionRow, 0, len(rows)), EnvironmentPredictions: children}
	for _, r := range rows {
		table.Rows = append(table.Rows, r)
	}
	sort.Slice(table.Rows, func(i, j int) bool { return table.Rows[i].KeyID < table.Rows[j].KeyID })
	return table, table.Validate()
}
