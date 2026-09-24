use crate::json::parse_without_duplicate_keys;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const MAX_AUTH_JSON_BYTES: usize = 5 * 1024;
const MAX_AUTH_SCOPES: usize = 500;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthFile {
    repositories: Vec<Repository>,
    principals: Vec<Principal>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Repository {
    repository_key: String,
    api_origin: String,
    repository_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Principal {
    id: String,
    token_sha256: String,
    #[serde(default)]
    read: Vec<ScopePermission>,
    #[serde(default)]
    write: Vec<ScopePermission>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScopePermission {
    repository_key: String,
    scope_id: String,
}

#[derive(Debug)]
struct PrincipalRecord {
    id: String,
    token_digest: [u8; 32],
    read: HashSet<ScopePermission>,
    write: HashSet<ScopePermission>,
}

#[derive(Debug, Default)]
pub struct Authorizer {
    repositories: HashSet<String>,
    principals: Vec<PrincipalRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Read,
    Write,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Authorization<'a> {
    Allowed(&'a str),
    InvalidCredential,
    Forbidden,
}

impl Authorizer {
    pub fn from_json(json: &str) -> Result<Self, String> {
        if json.len() > MAX_AUTH_JSON_BYTES {
            return Err("auth configuration exceeds 5 KiB".into());
        }
        let value = parse_without_duplicate_keys(json.as_bytes())
            .map_err(|_| "auth configuration is invalid JSON")?;
        let file: AuthFile = serde_json::from_value(value)
            .map_err(|_| "auth configuration has an invalid schema")?;
        if file.repositories.is_empty() || file.principals.is_empty() {
            return Err("auth configuration must define repositories and principals".into());
        }

        let mut repositories = HashSet::new();
        for repository in file.repositories {
            let origin = repository.api_origin.strip_prefix("https://");
            if !valid_repository_key(&repository.repository_key)
                || !repositories.insert(repository.repository_key)
                || !valid_repository_id(&repository.repository_id)
                || origin.is_none_or(|host| {
                    host.is_empty()
                        || host.contains('/')
                        || host
                            .chars()
                            .any(|character| matches!(character, '?' | '#' | '@' | '\\' | ' '))
                })
            {
                return Err(
                    "repository registry entries must use unique keys and HTTPS origins".into(),
                );
            }
        }

        let mut ids = HashSet::new();
        let mut digests = HashSet::new();
        let mut scope_count = HashSet::new();
        let mut principals = Vec::with_capacity(file.principals.len());
        for principal in file.principals {
            if !valid_id(&principal.id) || !ids.insert(principal.id.clone()) {
                return Err("principal IDs must be valid and unique".into());
            }
            let token_digest = decode_sha256(&principal.token_sha256)
                .ok_or("principal tokenSha256 must be 64 lowercase hex characters")?;
            if !digests.insert(token_digest) {
                return Err("principal token digests must be unique".into());
            }
            let read = validate_permissions(principal.read, &repositories, &mut scope_count)?;
            let write = validate_permissions(principal.write, &repositories, &mut scope_count)?;
            principals.push(PrincipalRecord {
                id: principal.id,
                token_digest,
                read,
                write,
            });
        }
        if scope_count.len() > MAX_AUTH_SCOPES {
            return Err("auth configuration exceeds 500 unique scopes".into());
        }
        Ok(Self {
            repositories,
            principals,
        })
    }

    pub fn authorize(
        &self,
        bearer: &str,
        repository_key: &str,
        scope_id: &str,
        permission: Permission,
    ) -> Authorization<'_> {
        let digest: [u8; 32] = Sha256::digest(bearer.as_bytes()).into();
        let mut principal = None;
        for candidate in &self.principals {
            if constant_time_eq(&candidate.token_digest, &digest) {
                principal = Some(candidate);
            }
        }
        let Some(principal) = principal else {
            return Authorization::InvalidCredential;
        };
        if !self.repositories.contains(repository_key) {
            return Authorization::Forbidden;
        }
        let target = ScopePermission {
            repository_key: repository_key.to_owned(),
            scope_id: scope_id.to_owned(),
        };
        let allowed = match permission {
            Permission::Read => principal.read.contains(&target),
            Permission::Write => principal.write.contains(&target),
        };
        if allowed {
            Authorization::Allowed(principal.id.as_str())
        } else {
            Authorization::Forbidden
        }
    }
}

fn validate_permissions(
    values: Vec<ScopePermission>,
    repositories: &HashSet<String>,
    scope_count: &mut HashSet<ScopePermission>,
) -> Result<HashSet<ScopePermission>, String> {
    let mut result = HashSet::with_capacity(values.len());
    for permission in values {
        if !repositories.contains(&permission.repository_key)
            || !valid_scope_id(&permission.scope_id)
            || !result.insert(permission.clone())
        {
            return Err(
                "permissions must contain unique exact registered repository/scope pairs".into(),
            );
        }
        scope_count.insert(permission);
    }
    Ok(result)
}

pub fn valid_repository_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
        && value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

pub fn valid_scope_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub fn decode_sha256(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let high = pair[0]
            - if pair[0].is_ascii_digit() {
                b'0'
            } else {
                b'a' - 10
            };
        let low = pair[1]
            - if pair[1].is_ascii_digit() {
                b'0'
            } else {
                b'a' - 10
            };
        output[index] = (high << 4) | low;
    }
    Some(output)
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn valid_repository_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 20 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

#[cfg(test)]
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_uses_exact_registered_scope_and_permission() {
        let token = "local-test-token-that-is-not-for-production";
        let digest = hex(&Sha256::digest(token.as_bytes()));
        let scope_id = "a".repeat(64);
        let json = format!(
            r#"{{"repositories":[{{"repositoryKey":"github-123","apiOrigin":"https://github.com","repositoryId":"12345"}}],"principals":[{{"id":"writer","tokenSha256":"{digest}","read":[],"write":[{{"repositoryKey":"github-123","scopeId":"{scope_id}"}}]}}]}}"#
        );
        let auth = Authorizer::from_json(&json).unwrap();
        assert!(matches!(
            auth.authorize(token, "github-123", &scope_id, Permission::Write),
            Authorization::Allowed("writer")
        ));
        assert_eq!(
            auth.authorize(token, "github-123", &scope_id, Permission::Read),
            Authorization::Forbidden
        );
        assert_eq!(
            auth.authorize(token, "github-124", &scope_id, Permission::Write),
            Authorization::Forbidden
        );
        assert_eq!(
            auth.authorize("wrong", "github-123", &scope_id, Permission::Write),
            Authorization::InvalidCredential
        );
    }

    #[test]
    fn auth_rejects_duplicates_unknown_repositories_and_wildcards() {
        let scope_id = "a".repeat(64);
        let token_digest = "0".repeat(64);
        let base = format!(
            r#"{{"repositories":[{{"repositoryKey":"github-123","apiOrigin":"https://github.com","repositoryId":"12345"}}],"principals":[{{"id":"writer","tokenSha256":"{token_digest}","write":[{{"repositoryKey":"github-123","scopeId":"{scope_id}"}}]}}]}}"#
        );
        assert!(Authorizer::from_json(
            &base.replace("\"principals\":[", "\"principals\":[],\"principals\":[")
        )
        .is_err());
        assert!(
            Authorizer::from_json(&base.replace("github-123\",\"scopeId", "*\",\"scopeId"))
                .is_err()
        );
        assert!(Authorizer::from_json(&base.replace(
            "\"repositoryKey\":\"github-123\",\"scopeId\"",
            "\"repositoryKey\":\"unknown\",\"scopeId\""
        ))
        .is_err());
    }
}
