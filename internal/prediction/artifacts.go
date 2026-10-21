package prediction

import (
	"fmt"
	"io"
	"os"
)

func ReadArtifact(path string, limit int, v any) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	b, err := io.ReadAll(io.LimitReader(f, int64(limit)+1))
	if err != nil {
		return err
	}
	if len(b) > limit {
		return fmt.Errorf("artifact exceeds %d MiB", limit/(1024*1024))
	}
	return Decode(b, v)
}
func (b ModelStateBundle) Validate() error {
	if b.Version != Version || len(b.States) > maxKeys {
		return invalid("invalid ModelStateBundle version or state count")
	}
	seen := map[string]bool{}
	for _, s := range b.States {
		if err := s.Validate(); err != nil {
			return err
		}
		id, err := s.Scope.ID()
		if err != nil {
			return err
		}
		if seen[id] {
			return invalid("ModelStateBundle contains duplicate scopes")
		}
		seen[id] = true
	}
	return sizeValid(b, maxModelBytes)
}
func (b PredictionArtifactBundle) Validate() error {
	if b.Version != Version || len(b.Predictions) > maxKeys {
		return invalid("invalid PredictionArtifact version or table count")
	}
	seen := map[string]bool{}
	for _, a := range b.Predictions {
		if err := a.Table.Validate(); err != nil {
			return err
		}
		if !textValid(a.ModelArtifact.Name, 256) || !digestPattern.MatchString(a.ModelArtifact.Sha256) {
			return invalid("invalid model artifact reference")
		}
		id, err := a.Table.Scope.ID()
		if err != nil {
			return err
		}
		if seen[id] {
			return invalid("PredictionArtifact contains duplicate scopes")
		}
		seen[id] = true
	}
	return sizeValid(b, maxPredictionBytes)
}
