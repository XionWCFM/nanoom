package history

import (
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"fmt"
	"regexp"
	"strings"

	"github.com/XionWCFM/nanoom/internal/prediction"
)

type Permission struct {
	RepositoryKey string `json:"repositoryKey"`
	ScopeID       string `json:"scopeId"`
}
type repository struct {
	RepositoryKey string `json:"repositoryKey"`
	APIOrigin     string `json:"apiOrigin"`
	RepositoryID  string `json:"repositoryId"`
}
type principal struct {
	ID          string       `json:"id"`
	TokenSHA256 string       `json:"tokenSha256"`
	Read        []Permission `json:"read" optional:"true"`
	Write       []Permission `json:"write" optional:"true"`
	digest      []byte
}
type Authorizer struct {
	repositories map[string]bool
	principals   []principal
}

var repositoryPattern = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,63}$`)
var digestPattern = regexp.MustCompile(`^[0-9a-f]{64}$`)
var idPattern = regexp.MustCompile(`^[A-Za-z0-9._-]{1,64}$`)
var decimalPattern = regexp.MustCompile(`^[0-9]{1,20}$`)

func ParseAuth(data []byte) (*Authorizer, error) {
	if len(data) > 5*1024 {
		return nil, fmt.Errorf("auth configuration exceeds 5 KiB")
	}
	var file struct {
		Repositories []repository `json:"repositories"`
		Principals   []principal  `json:"principals"`
	}
	if err := prediction.Decode(data, &file); err != nil {
		return nil, fmt.Errorf("auth configuration has an invalid schema")
	}
	if len(file.Repositories) == 0 || len(file.Principals) == 0 {
		return nil, fmt.Errorf("auth configuration must define repositories and principals")
	}
	auth := &Authorizer{repositories: map[string]bool{}, principals: file.Principals}
	for _, r := range file.Repositories {
		host, ok := strings.CutPrefix(r.APIOrigin, "https://")
		if !repositoryPattern.MatchString(r.RepositoryKey) || auth.repositories[r.RepositoryKey] || !decimalPattern.MatchString(r.RepositoryID) || !ok || host == "" || strings.ContainsAny(host, "/?#@\\ ") {
			return nil, fmt.Errorf("repository registry entries must use unique keys and HTTPS origins")
		}
		auth.repositories[r.RepositoryKey] = true
	}
	ids := map[string]bool{}
	digests := map[string]bool{}
	scopes := map[Permission]bool{}
	for i := range auth.principals {
		p := &auth.principals[i]
		if !idPattern.MatchString(p.ID) || ids[p.ID] {
			return nil, fmt.Errorf("principal IDs must be valid and unique")
		}
		ids[p.ID] = true
		if !digestPattern.MatchString(p.TokenSHA256) || digests[p.TokenSHA256] {
			return nil, fmt.Errorf("principal token digests must be valid and unique")
		}
		digests[p.TokenSHA256] = true
		p.digest, _ = hex.DecodeString(p.TokenSHA256)
		for _, permissions := range [][]Permission{p.Read, p.Write} {
			seen := map[Permission]bool{}
			for _, permission := range permissions {
				if !auth.repositories[permission.RepositoryKey] || !digestPattern.MatchString(permission.ScopeID) || seen[permission] {
					return nil, fmt.Errorf("permissions must contain unique exact registered repository/scope pairs")
				}
				seen[permission] = true
				scopes[permission] = true
			}
		}
	}
	if len(scopes) > 500 {
		return nil, fmt.Errorf("auth configuration exceeds 500 unique scopes")
	}
	return auth, nil
}

// Authorize returns 200, 401 or 403; a write grant never implies a read grant.
func (a *Authorizer) Authorize(token string, permission Permission, write bool) int {
	sum := sha256.Sum256([]byte(token))
	match := -1
	for i, p := range a.principals {
		if subtle.ConstantTimeCompare(sum[:], p.digest) == 1 {
			match = i
		}
	}
	if match < 0 {
		return 401
	}
	if !a.repositories[permission.RepositoryKey] {
		return 403
	}
	p := a.principals[match]
	permissions := p.Read
	if write {
		permissions = p.Write
	}
	for _, allowed := range permissions {
		if allowed == permission {
			return 200
		}
	}
	return 403
}
func validBearer(s string) bool {
	if len(s) < 32 || len(s) > 256 {
		return false
	}
	value, padding, _ := strings.Cut(s, "=")
	for _, c := range value {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || strings.ContainsRune("-._~+/", c)) {
			return false
		}
	}
	for _, c := range padding {
		if c != '=' {
			return false
		}
	}
	return true
}
