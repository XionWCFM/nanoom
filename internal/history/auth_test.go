package history

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/XionWCFM/nanoom/internal/prediction"
)

func authJSON(token string, read, write []Permission) []byte {
	body, _ := json.Marshal(map[string]any{"repositories": []any{map[string]any{"repositoryKey": "github-12345", "apiOrigin": "https://github.com", "repositoryId": "12345"}}, "principals": []any{map[string]any{"id": "writer", "tokenSha256": prediction.HexDigest([]byte(token)), "read": read, "write": write}}})
	return body
}
func TestExactAuthAndCredentialIsolation(t *testing.T) {
	target := Permission{"github-12345", strings.Repeat("a", 64)}
	token := strings.Repeat("t", 40)
	for _, write := range []bool{false, true} {
		readPermissions, writePermissions := []Permission{}, []Permission{}
		if write {
			writePermissions = append(writePermissions, target)
		} else {
			readPermissions = append(readPermissions, target)
		}
		auth, err := ParseAuth(authJSON(token, readPermissions, writePermissions))
		if err != nil {
			t.Fatal(err)
		}
		if auth.Authorize(token, target, write) != 200 || auth.Authorize(token, target, !write) != 403 {
			t.Fatal("read/write permissions leaked")
		}
		if auth.Authorize(token, Permission{target.RepositoryKey, strings.Repeat("b", 64)}, write) != 403 || auth.Authorize(token, Permission{"other", target.ScopeID}, write) != 403 {
			t.Fatal("cross-scope grant leaked")
		}
		for i := 0; i < len(token); i++ {
			wrong := []byte(token)
			wrong[i] = 'x'
			if auth.Authorize(string(wrong), target, write) != 401 {
				t.Fatal("invalid credential accepted")
			}
		}
	}
}
func TestAuthRejectsMalformedAndDuplicateConfiguration(t *testing.T) {
	target := Permission{"github-12345", strings.Repeat("a", 64)}
	valid := string(authJSON(strings.Repeat("t", 40), []Permission{target}, []Permission{}))
	cases := []string{
		`{}`, strings.Replace(valid, `"principals":`, `"principals":[],"principals":`, 1), strings.Replace(valid, `"repositoryKey":"github-12345"`, `"repositoryKey":"*"`, 1), strings.Replace(valid, `"apiOrigin":"https://github.com"`, `"apiOrigin":"https://user@github.com"`, 1), strings.Replace(valid, `"id":"writer"`, `"id":"writer","unknown":1`, 1), strings.Replace(valid, `"read":[{`, `"read":[{"unknown":1,`, 1), strings.Repeat(" ", 5121), strings.Replace(valid, `"read":[`, `"read":null,"unused":[`, 1),
	}
	for _, input := range cases {
		if _, err := ParseAuth([]byte(input)); err == nil {
			t.Fatalf("invalid auth accepted: %s", input)
		}
	}
	padded := valid + strings.Repeat(" ", 5120-len(valid))
	if _, err := ParseAuth([]byte(padded)); err != nil {
		t.Fatal("5KiB boundary rejected:", err)
	}
	if _, err := ParseAuth([]byte(padded + " ")); err == nil {
		t.Fatal("oversized auth accepted")
	}
	// Missing optional read/write arrays must preserve the Rust secret format.
	omitted := strings.Replace(strings.Replace(valid, `"write":[],`, "", 1), `,"write":[]`, "", 1)
	if _, err := ParseAuth([]byte(omitted)); err != nil {
		t.Fatal(err)
	}
}
func TestBearerBoundary(t *testing.T) {
	for _, token := range []string{strings.Repeat("a", 31), strings.Repeat("a", 257), strings.Repeat("a", 32) + " a", strings.Repeat("a", 32) + "=x"} {
		if validBearer(token) {
			t.Fatal("invalid bearer accepted")
		}
	}
	for _, token := range []string{strings.Repeat("a", 32), strings.Repeat("a", 256), strings.Repeat("a", 32) + "=="} {
		if !validBearer(token) {
			t.Fatal("valid bearer rejected")
		}
	}
}
