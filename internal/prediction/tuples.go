package prediction

import (
	"encoding/json"
	"fmt"
)

func (v ScopeRef) MarshalJSON() ([]byte, error) {
	switch v.Kind {
	case "push":
		return json.Marshal(map[string]any{"kind": v.Kind, "ref": v.Ref})
	case "pull_request":
		return json.Marshal(map[string]any{"kind": v.Kind, "number": v.Number, "headRepositoryId": v.HeadRepositoryID, "headRef": v.HeadRef, "baseRef": v.BaseRef})
	default:
		return nil, fmt.Errorf("unsupported scope ref kind %q", v.Kind)
	}
}
func (v *ScopeRef) UnmarshalJSON(b []byte) error {
	var tag struct {
		Kind string `json:"kind"`
	}
	if err := json.Unmarshal(b, &tag); err != nil {
		return err
	}
	switch tag.Kind {
	case "push":
		var obj struct {
			Kind string `json:"kind"`
			Ref  string `json:"ref"`
		}
		if err := Decode(b, &obj); err != nil {
			return err
		}
		*v = ScopeRef{Kind: obj.Kind, Ref: obj.Ref}
	case "pull_request":
		var obj struct {
			Kind             string `json:"kind"`
			Number           uint64 `json:"number"`
			HeadRepositoryID string `json:"headRepositoryId"`
			HeadRef          string `json:"headRef"`
			BaseRef          string `json:"baseRef"`
		}
		if err := Decode(b, &obj); err != nil {
			return err
		}
		*v = ScopeRef{Kind: obj.Kind, Number: obj.Number, HeadRepositoryID: obj.HeadRepositoryID, HeadRef: obj.HeadRef, BaseRef: obj.BaseRef}
	default:
		return fmt.Errorf("unsupported scope ref kind %q", tag.Kind)
	}
	return nil
}
func tuple(b []byte, fields ...any) error {
	var raw []json.RawMessage
	if err := json.Unmarshal(b, &raw); err != nil {
		return err
	}
	if len(raw) != len(fields) {
		return fmt.Errorf("expected %d tuple elements", len(fields))
	}
	for i, dst := range fields {
		if err := Decode(raw[i], dst); err != nil {
			return err
		}
	}
	return nil
}
func (v PredictionRow) MarshalJSON() ([]byte, error) {
	return json.Marshal([]any{v.KeyID, v.EstimatedMs, v.ObservationCount, v.LastObservedAtMs, v.ValidUntilMs})
}
func (v *PredictionRow) UnmarshalJSON(b []byte) error {
	return tuple(b, &v.KeyID, &v.EstimatedMs, &v.ObservationCount, &v.LastObservedAtMs, &v.ValidUntilMs)
}
func (v RecentBatch) MarshalJSON() ([]byte, error) {
	return json.Marshal([]any{v.ObservedAtMs, v.RunID, v.RunAttempt, v.MeanMs})
}
func (v *RecentBatch) UnmarshalJSON(b []byte) error {
	return tuple(b, &v.ObservedAtMs, &v.RunID, &v.RunAttempt, &v.MeanMs)
}
func (v BatchReceipt) MarshalJSON() ([]byte, error) {
	return json.Marshal([]any{v.BatchID, v.BodyDigest, v.ProducedAtMs})
}
func (v *BatchReceipt) UnmarshalJSON(b []byte) error {
	return tuple(b, &v.BatchID, &v.BodyDigest, &v.ProducedAtMs)
}
