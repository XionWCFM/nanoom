package history

import (
	"context"
	"database/sql"
	"errors"
)

// SQLStore uses the existing D1 schema and atomic conditional SQL statements.
// No transaction or process-local mutex is used to emulate cross-isolate CAS.
type SQLStore struct{ DB *sql.DB }

func (s SQLStore) Ready(ctx context.Context) error {
	var ready int
	return s.DB.QueryRowContext(ctx, "SELECT 1 AS ready").Scan(&ready)
}
func (s SQLStore) Load(ctx context.Context, key string) (*StoredModel, error) {
	var body, digest string
	err := s.DB.QueryRowContext(ctx, "SELECT model_json, model_sha256 FROM prediction_state WHERE storage_key = ?1", key).Scan(&body, &digest)
	if errors.Is(err, sql.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	return &StoredModel{[]byte(body), digest}, nil
}
func (s SQLStore) CompareAndSwap(ctx context.Context, key, expected string, value StoredModel, updatedAt uint64) (bool, error) {
	var result sql.Result
	var err error
	if expected == "" {
		result, err = s.DB.ExecContext(ctx, "INSERT OR IGNORE INTO prediction_state (storage_key, model_json, model_sha256, updated_at_ms) VALUES (?1, ?2, ?3, ?4)", key, string(value.JSON), value.SHA256, int64(updatedAt))
	} else {
		result, err = s.DB.ExecContext(ctx, "UPDATE prediction_state SET model_json = ?1, model_sha256 = ?2, updated_at_ms = ?3 WHERE storage_key = ?4 AND model_sha256 = ?5", string(value.JSON), value.SHA256, int64(updatedAt), key, expected)
	}
	if err != nil {
		return false, err
	}
	changed, err := result.RowsAffected()
	return changed == 1, err
}
func (s SQLStore) Cleanup(ctx context.Context, cutoff uint64) (int64, error) {
	result, err := s.DB.ExecContext(ctx, "DELETE FROM prediction_state WHERE storage_key IN (SELECT storage_key FROM prediction_state WHERE updated_at_ms < ?1 ORDER BY updated_at_ms LIMIT 10000)", int64(cutoff))
	if err != nil {
		return 0, err
	}
	return result.RowsAffected()
}
