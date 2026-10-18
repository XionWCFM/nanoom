package prediction

import "fmt"

type Index struct{ tables map[string]PredictionTable }

func NewIndex(bundles []PredictionArtifactBundle, tables []PredictionTable) (Index, error) {
	index := Index{tables: map[string]PredictionTable{}}
	for _, bundle := range bundles {
		if err := bundle.Validate(); err != nil {
			return index, err
		}
		for _, a := range bundle.Predictions {
			tables = append(tables, a.Table)
		}
	}
	for _, table := range tables {
		if err := table.Validate(); err != nil {
			return index, err
		}
		id, err := table.Scope.ID()
		if err != nil {
			return index, err
		}
		if _, ok := index.tables[id]; ok {
			return index, fmt.Errorf("duplicate PredictionTable scope")
		}
		index.tables[id] = table
	}
	return index, nil
}
func (c PredictionContext) Scope(ref ScopeRef, group, runner, environment string) Scope {
	return Scope{c.RepositoryKey, c.WorkflowPath, ref, group, runner, environment}
}
func (c PredictionContext) Validate() error {
	return c.Scope(c.Ref, "context", "npm", "context").Validate()
}
func (c PredictionContext) Candidates(group, runner, environment string) []Scope {
	scopes := []Scope{c.Scope(c.Ref, group, runner, environment)}
	if c.Ref.Kind == "pull_request" {
		scopes = append(scopes, c.Scope(ScopeRef{Kind: "push", Ref: c.Ref.BaseRef}, group, runner, environment))
	}
	return scopes
}
func (i Index) HasValidRows(now uint64) bool {
	for _, table := range i.tables {
		for _, row := range table.Rows {
			if now < row.ValidUntilMs {
				return true
			}
		}
	}
	return false
}
func (i Index) Estimate(c *PredictionContext, group, runner, environment, key string, now uint64) (uint64, uint64, bool) {
	if c == nil {
		return 0, 0, false
	}
	for _, scope := range c.Candidates(group, runner, environment) {
		id, err := scope.ID()
		if err != nil {
			continue
		}
		table, ok := i.tables[id]
		if !ok || table.Scope != scope {
			continue
		}
		if duration, count, ok := table.Estimate(key, now); ok {
			return duration, count, true
		}
	}
	return 0, 0, false
}
func (i Index) EnvironmentRange(c *PredictionContext, group, runner, environment, key string, now uint64) (uint64, uint64, uint64) {
	if c == nil {
		return 0, 0, 0
	}
	for _, scope := range c.Candidates(group, runner, environment) {
		id, err := scope.ID()
		if err != nil {
			continue
		}
		table, ok := i.tables[id]
		if !ok || table.Scope != scope {
			continue
		}
		if _, _, ok := table.Estimate(key, now); !ok {
			continue
		}
		var latest, count, low, high uint64
		for _, child := range table.EnvironmentPredictions {
			for _, r := range child.Table.Rows {
				if r.KeyID == key && now < r.ValidUntilMs {
					latest = max(latest, r.LastObservedAtMs)
				}
			}
		}
		cutoff := sub(latest, 7*DayMs)
		for _, child := range table.EnvironmentPredictions {
			for _, r := range child.Table.Rows {
				if r.KeyID == key && r.LastObservedAtMs >= cutoff && now < r.ValidUntilMs {
					if count == 0 {
						low = r.EstimatedMs
					}
					count++
					low = min(low, r.EstimatedMs)
					high = max(high, r.EstimatedMs)
				}
			}
		}
		return count, low, high
	}
	return 0, 0, 0
}
