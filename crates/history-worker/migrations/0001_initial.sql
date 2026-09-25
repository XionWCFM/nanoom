CREATE TABLE prediction_state (
  storage_key TEXT PRIMARY KEY,
  model_json TEXT NOT NULL CHECK (length(CAST(model_json AS BLOB)) <= 1900000),
  model_sha256 TEXT NOT NULL CHECK (length(model_sha256) = 64),
  updated_at_ms REAL NOT NULL
) WITHOUT ROWID;

CREATE INDEX prediction_state_updated_at_ms
  ON prediction_state (updated_at_ms);
