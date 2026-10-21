// Package history serves the existing History v1 HTTP contract using Go.
package history

import (
	"context"
	"crypto/rand"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/XionWCFM/nanoom/internal/prediction"
)

const maxRequestBytes = 16 * 1024 * 1024
const maxModelBytes = 1_900_000

type StoredModel struct {
	JSON   []byte
	SHA256 string
}

// Store must compare and replace atomically across all service instances. An
// empty expected digest means insert only, never overwrite an existing row.
type Store interface {
	Ready(context.Context) error
	Load(context.Context, string) (*StoredModel, error)
	CompareAndSwap(context.Context, string, string, StoredModel, uint64) (bool, error)
	Cleanup(context.Context, uint64) (int64, error)
}
type Server struct {
	Store Store
	Auth  *Authorizer
	Now   func() time.Time
}
type problem struct {
	status       int
	code, detail string
	allow        string
}

func (p *problem) Error() string { return p.code }
func fail(status int, code, detail string) *problem {
	return &problem{status: status, code: code, detail: detail}
}
func (s *Server) now() uint64 {
	if s.Now != nil {
		return uint64(s.Now().UnixMilli())
	}
	return uint64(time.Now().UnixMilli())
}
func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	requestID := r.Header.Get("Cf-Ray")
	if len(requestID) == 0 || len(requestID) > 128 || strings.ContainsAny(requestID, "\r\n") {
		requestID = fmt.Sprintf("local-%d", s.now())
	}
	w.Header().Set("X-Request-Id", requestID)
	w.Header().Set("Cache-Control", "no-store")
	if err := s.handle(w, r); err != nil {
		var p *problem
		if !errors.As(err, &p) {
			p = fail(503, "storage_unavailable", "History storage is unavailable.")
		}
		w.Header().Set("Content-Type", "application/problem+json")
		if p.status == 429 || p.status == 503 {
			w.Header().Set("Retry-After", "1")
		}
		if p.status == 401 {
			w.Header().Set("WWW-Authenticate", "Bearer")
		}
		if p.allow != "" {
			w.Header().Set("Allow", p.allow)
		}
		w.WriteHeader(p.status)
		_ = json.NewEncoder(w).Encode(map[string]any{"type": "about:blank", "title": http.StatusText(p.status), "status": p.status, "code": p.code, "detail": p.detail, "requestId": requestID})
	}
}
func (s *Server) handle(w http.ResponseWriter, r *http.Request) error {
	if r.Method == http.MethodGet && r.URL.Path == "/health" {
		return writeJSON(w, 200, map[string]any{"status": "ok"})
	}
	if r.Method == http.MethodGet && r.URL.Path == "/ready" {
		if s.Store == nil || s.Store.Ready(r.Context()) != nil {
			return writeJSON(w, 503, map[string]any{"status": "not_ready", "reason": "storage_unavailable", "requestId": w.Header().Get("X-Request-Id")})
		}
		return writeJSON(w, 200, map[string]any{"status": "ready"})
	}
	parts := strings.Split(r.URL.Path, "/")
	if len(parts) != 7 || parts[0] != "" || parts[1] != "v1" || parts[2] != "repositories" || parts[4] != "scopes" || !repositoryPattern.MatchString(parts[3]) || !digestPattern.MatchString(parts[5]) || (parts[6] != "snapshot" && parts[6] != "observations:merge") {
		return fail(404, "not_found", "No endpoint matches this path.")
	}
	write := parts[6] == "observations:merge"
	method := http.MethodGet
	if write {
		method = http.MethodPost
	}
	if r.Method != method {
		p := fail(405, "method_not_allowed", "Method is not supported for this endpoint.")
		p.allow = method
		return p
	}
	if s.Auth == nil {
		return fail(503, "configuration_error", "History authorization is not configured.")
	}
	scheme, token, ok := strings.Cut(r.Header.Get("Authorization"), " ")
	if !ok || !strings.EqualFold(scheme, "Bearer") || !validBearer(token) {
		return fail(401, "unauthenticated", "A valid bearer credential is required.")
	}
	permission := Permission{parts[3], parts[5]}
	if status := s.Auth.Authorize(token, permission, write); status != 200 {
		if status == 401 {
			return fail(401, "unauthenticated", "A valid bearer credential is required.")
		}
		return fail(403, "forbidden", "Principal lacks exact scope permission.")
	}
	if s.Store == nil {
		return fail(503, "storage_unavailable", "History storage is unavailable.")
	}
	key := "nanoom/prediction-state/v3/repositories/" + permission.RepositoryKey + "/scopes/" + permission.ScopeID + "/model.json"
	if write {
		return s.merge(w, r, key, permission)
	}
	return s.snapshot(w, r, key, permission)
}
func writeJSON(w http.ResponseWriter, status int, v any) error {
	body, err := prediction.CanonicalBytes(v)
	if err != nil {
		return err
	}
	w.Header().Set("Content-Type", "application/json")
	if status == 503 {
		w.Header().Set("Retry-After", "1")
	}
	w.WriteHeader(status)
	_, err = w.Write(body)
	return err
}
func (s *Server) load(ctx context.Context, key string, p Permission) (*prediction.ModelState, string, error) {
	stored, err := s.Store.Load(ctx, key)
	if err != nil {
		return nil, "", err
	}
	if stored == nil {
		return nil, "", nil
	}
	corrupt := func() (*prediction.ModelState, string, error) {
		return nil, "", fail(503, "storage_corrupt", "Stored history failed integrity validation.")
	}
	if len(stored.JSON) > maxModelBytes || prediction.HexDigest(stored.JSON) != stored.SHA256 {
		return corrupt()
	}
	var model prediction.ModelState
	if prediction.Decode(stored.JSON, &model) != nil || model.Validate() != nil {
		return corrupt()
	}
	id, err := model.Scope.ID()
	if err != nil || model.Scope.RepositoryKey != p.RepositoryKey || id != p.ScopeID {
		return corrupt()
	}
	return &model, stored.SHA256, nil
}
func (s *Server) snapshot(w http.ResponseWriter, r *http.Request, key string, p Permission) error {
	state, _, err := s.load(r.Context(), key, p)
	if err != nil {
		return err
	}
	if state == nil {
		return fail(404, "history_not_found", "No currently usable history exists for this scope.")
	}
	table, err := prediction.ProjectPredictions(*state, s.now())
	if err != nil {
		return fail(503, "storage_corrupt", "Stored history failed integrity validation.")
	}
	if len(table.Rows) == 0 {
		return fail(404, "history_not_found", "No currently usable history exists for this scope.")
	}
	body, err := prediction.CanonicalBytes(table)
	if err != nil {
		return err
	}
	etag := `"sha256:` + prediction.HexDigest(body) + `"`
	w.Header().Set("ETag", etag)
	w.Header().Set("Cache-Control", "private, no-cache")
	w.Header().Set("Content-Type", "application/json")
	for _, candidate := range strings.Split(r.Header.Get("If-None-Match"), ",") {
		candidate = strings.TrimSpace(candidate)
		candidate = strings.TrimPrefix(candidate, "W/")
		if candidate == etag || candidate == "*" {
			w.WriteHeader(304)
			return nil
		}
	}
	w.WriteHeader(200)
	_, err = w.Write(body)
	return err
}
func requestMedia(r *http.Request) error {
	parts := strings.Split(r.Header.Get("Content-Type"), ";")
	valid := strings.EqualFold(strings.TrimSpace(parts[0]), "application/json") && len(parts) <= 2
	if len(parts) == 2 {
		name, value, ok := strings.Cut(strings.TrimSpace(parts[1]), "=")
		valid = valid && ok && strings.EqualFold(strings.TrimSpace(name), "charset") && strings.EqualFold(strings.Trim(strings.TrimSpace(value), `"`), "utf-8")
	}
	if encoding, ok := r.Header["Content-Encoding"]; ok && (len(encoding) != 1 || !strings.EqualFold(encoding[0], "identity")) {
		valid = false
	}
	if !valid {
		return fail(415, "unsupported_media_type", "Only uncompressed application/json UTF-8 is supported.")
	}
	if length := r.Header.Get("Content-Length"); length != "" {
		n, err := strconv.ParseUint(length, 10, 64)
		if err != nil {
			return fail(400, "invalid_request", "Content-Length is invalid.")
		}
		if n > maxRequestBytes {
			return fail(413, "payload_too_large", "Request exceeds 16 MiB.")
		}
	}
	if r.ContentLength > maxRequestBytes {
		return fail(413, "payload_too_large", "Request exceeds 16 MiB.")
	}
	return nil
}
func (s *Server) merge(w http.ResponseWriter, r *http.Request, key string, p Permission) error {
	if err := requestMedia(r); err != nil {
		return err
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, maxRequestBytes+1))
	if err != nil {
		return fail(400, "invalid_request", "Request body could not be read.")
	}
	if len(body) > maxRequestBytes {
		return fail(413, "payload_too_large", "Request exceeds 16 MiB.")
	}
	var raw any
	if err := prediction.Decode(body, &raw); err != nil {
		return fail(400, "invalid_request", "Request body must be valid UTF-8 JSON with unique keys.")
	}
	var batch prediction.ObservationBatch
	if err := prediction.Decode(body, &batch); err != nil {
		return fail(422, "invalid_history", "Observation batch schema is invalid.")
	}
	id, err := batch.Scope.ID()
	if err != nil {
		return fail(422, "invalid_history", "Observation scope is invalid.")
	}
	if batch.Scope.RepositoryKey != p.RepositoryKey || id != p.ScopeID {
		return fail(403, "scope_mismatch", "Observation scope does not match the authorized URL.")
	}
	now := s.now()
	if err := batch.Validate(now); err != nil {
		code := "invalid_history"
		if strings.Contains(err.Error(), "expired") || strings.Contains(err.Error(), "future") {
			code = "batch_expired"
		}
		return fail(422, code, "Observation batch validation failed.")
	}
	digest, err := prediction.Digest(batch)
	if err != nil {
		return fail(422, "invalid_history", "Observation batch digest is invalid.")
	}
	idem := r.Header.Get("Idempotency-Key")
	if idem == "" {
		return fail(400, "idempotency_key_mismatch", "Idempotency-Key is required.")
	}
	if idem != digest {
		return fail(409, "idempotency_key_mismatch", "Idempotency-Key must equal the canonical batch digest.")
	}
	for attempt := 0; attempt < 8; attempt++ {
		prior, expected, err := s.load(r.Context(), key, p)
		if err != nil {
			return err
		}
		state, outcome, err := prediction.ApplyBatch(prior, batch, now)
		if err != nil {
			code := "invalid_history"
			status := 422
			if strings.Contains(err.Error(), "different body digest") {
				status = 409
				code = "batch_conflict"
			} else if strings.Contains(err.Error(), "receipt capacity") {
				status = 409
				code = "receipt_capacity_exceeded"
			} else if strings.Contains(err.Error(), "capacity") || strings.Contains(err.Error(), "exceeds 50000") || strings.Contains(err.Error(), "byte limit") {
				status = 409
				code = "model_capacity_exceeded"
			}
			return fail(status, code, "Observation batch could not be applied.")
		}
		if !outcome.Duplicate {
			bytes, err := prediction.CanonicalBytes(state)
			if err != nil {
				return err
			}
			if len(bytes) > maxModelBytes {
				return fail(409, "model_capacity_exceeded", "History model exceeds the 1.9 MB D1 row limit.")
			}
			stored, err := s.Store.CompareAndSwap(r.Context(), key, expected, StoredModel{bytes, prediction.HexDigest(bytes)}, state.UpdatedAtMs)
			if err != nil {
				return err
			}
			if !stored {
				if attempt == 7 {
					break
				}
				ceiling := min(10<<min(attempt, 4), 200)
				var random [1]byte
				_, _ = rand.Read(random[:])
				timer := time.NewTimer(time.Duration(int(random[0])%(ceiling+1)) * time.Millisecond)
				select {
				case <-r.Context().Done():
					timer.Stop()
					return r.Context().Err()
				case <-timer.C:
				}
				continue
			}
		}
		received := len(batch.Aggregates)
		for _, child := range batch.EnvironmentBatches {
			received += len(child.Batch.Aggregates)
		}
		retained := len(state.Entries)
		for _, child := range state.EnvironmentStates {
			retained += len(child.State.Entries)
		}
		result := "created"
		if prior != nil {
			result = "updated"
		}
		if outcome.Duplicate {
			result = "unchanged"
		}
		return writeJSON(w, 200, map[string]any{"scopeId": id, "modelVersion": state.Version, "outcome": result, "applied": !outcome.Duplicate, "receivedAggregateCount": received, "retainedKeyCount": retained, "prunedBucketCount": outcome.PrunedBuckets, "receiptCount": len(state.Receipts), "modelUpdatedAtMs": state.UpdatedAtMs})
	}
	return fail(503, "cas_retries_exhausted", "Concurrent history updates exceeded the retry limit.")
}

func (s *Server) Cleanup(ctx context.Context) (int64, error) {
	if s.Store == nil {
		return 0, fmt.Errorf("history storage is unavailable")
	}
	now := s.now()
	var cutoff uint64
	if now > 45*prediction.DayMs {
		cutoff = now - 45*prediction.DayMs
	}
	return s.Store.Cleanup(ctx, cutoff)
}
