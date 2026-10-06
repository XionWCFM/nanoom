# nanoom Code Principles & Conventions

The repository root AGENTS.md and ADR-0015 are authoritative. These implementation conventions complement the released-product and template-first principles. The current public contract is in SPEC.md; historical plans do not override it.

---

## 1. Core Philosophy

- **Unix philosophy**: Do one thing well, composable, text streams
- **Vite-inspired API**: Declarative, zero-config defaults, explicit customization
- **Performance first**: O(n) algorithms, minimal allocations, streaming where possible
- **Type safety**: Leverage Rust type system, no `unwrap()` in production code
- **Testability**: Pure functions, dependency injection, no global state
- **Minimal configuration**: Reuse existing tools and native defaults; declare only needed differences. Share repeated settings and keep generated files out of source control.

---

## 2. Rust Edition & Toolchain

- **Edition**: 2021
- **Development Rust version**: The exact version in `rust-toolchain.toml`, synchronized with `mise.toml`
- **Toolchain**: Managed via `rust-toolchain.toml`

---

## 3. Code Style & Formatting

- **Formatter**: `cargo fmt` (default style)
- **Linter**: `cargo clippy --locked --all-targets --all-features -- -D warnings`
- **Max line width**: 100 characters
- **Import grouping**: std → external → local
- **No trailing whitespace**

---

## 4. Naming Conventions

| Category | Convention | Example |
|----------|------------|---------|
| Types | PascalCase | `Config`, `AffectedOutput`, `MatrixEntry` |
| Functions/Variables | snake_case | `calculate_affected`, `repo_root` |
| Constants | SCREAMING_SNAKE_CASE | `DEFAULT_TIMEOUT`, `MAX_RETRIES` |
| Crates | kebab-case | `nanoom` |
| Modules | snake_case | `config_parser`, `git_ops` |
| Features | kebab-case | `cli`, `json-output` |

---

## 5. Error Handling

- **NEVER** use `unwrap()`, `expect()`, `panic!()` in production paths
- **Errors**: Use the shared `crate::error::Result` and `thiserror::Error` variants.
- **Error messages must be actionable**: What failed, why, how to fix
- **CI errors**: Use `::error` GitHub Actions annotation format
- **Propagation**: Use `?` operator
- **Context enrichment**: Convert boundary errors into actionable existing Error variants.

```rust
// Good
fn parse_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)?;
    let config = serde_json::from_str(&content)?;
    Ok(config)
}

// Bad - never do this
fn parse_config(path: &Path) -> Config {
    let content = fs::read_to_string(path).unwrap();
    serde_json::from_str(&content).unwrap()
}
```

---

## 6. Testing Principles (User-path Contracts)

- **Prefer real implementations over mocks**: Test actual git, actual filesystem, actual CLI
- **Unit tests**: Test pure functions in isolation (parser, mapper, calculator)
- **Integration tests**: Test full CLI commands with real filesystem repositories and Git subprocesses.
- **Edge cases**: Reuse the existing test harness for glob, path and error boundaries.
- **Test organization**: `#[cfg(test)]` modules in same file, integration tests in `tests/`
- **Acceptance**: Verify observable outputs, exit codes, side effects and real consumer paths. No coverage percentage gate; coverage may be used manually to investigate missing cases.

### 6.1 Test Patterns

```rust
// Unit test pattern
#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    
    #[test]
    fn parse_valid_config() {
        let json = r#"{"group":{...}}"#;
        let config: Configuration = serde_json::from_str(json).unwrap();
        assert_eq!(config.group.len(), 2);
    }
}
```

---

## 7. Git Operations (gitoxide)

- Use **sync API** for local operations (`rev-parse`, `diff`, `merge-base`)
- Use **async API** only for network fetch
- Always check `is_shallow_repository()` before `merge-base`
- Deepen shallow history using the existing bounded 32/128/512/2048 progression and configured cap.
- Convert gitoxide errors to our error types with context

---

## 8. Configuration Design

- Repository config uses `nanoom.config.json`; explicit CLI overrides and native GitHub Action context remain supported.
- **JSON Schema** generated from Rust types via `schemars`
- Validation at parse time with clear error messages
- Unknown fields = error (strict parsing)
- Config structs derive: `Serialize`, `Deserialize`, `JsonSchema`, `Debug`, `Clone`, `PartialEq`

---

## 9. CLI Design (clap derive)

- Single binary, subcommands for each operation
- **Global flags**: `--config`, `--cwd`, `--verbose`; machine-output subcommands provide `--json`.
- Subcommand-specific args
- **Help text**: Concise, examples in `long_help`
- **Exit codes**:
  - `0` = success
  - `1` = general error
  - `2` = usage error

---

## 10. JSON Output

- Always valid UTF-8 JSON to stdout
- Use `serde_json` for serialization
- Structured output for machine consumption
- Human-readable output to stderr (logs, progress)
- Machine output to stdout only (for piping)

---

## 11. Performance Guidelines

- Avoid unnecessary allocations: use `&str` over `String` where possible
- Stream large outputs (diff, logs) rather than collecting
- Reuse buffers for repeated operations
- Profile before optimizing: `cargo flamegraph`
- **Target**: Affected calculation < 2s for 10k file monorepo

---

## 12. Security

- No shell injection: use `Command` API, not shell
- Validate all paths: canonicalize, check within repo root
- No secrets in logs or output
- Fork PR defense: limited permissions, no write access

---

## 13. Dependency Management

- **Minimal dependencies**: Audit with `cargo audit`
- Prefer stdlib over external crates
- **Pin versions** in `Cargo.lock` (committed)
- **Update policy**: Monthly `cargo update`, test thoroughly

---

## 14. Documentation

- Every public API: doc comment with example
- Module-level docs explaining purpose
- README for each major component
- Architecture Decision Records (ADR) for major choices

---

## 15. Git Workflow

- **Trunk-based development**
- Feature branches, PR required
- CI must pass (all quality gates)
- **Conventional commits**: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`
- Changesets for version bumps

---

## 16. CI Pipeline Requirements

- **Operating path**: Released public Nanoom Actions `@latest`, with the binary matching the Action source.
- **Jobs**: `affected` → `run` → `status`; see `AGENTS.md` and `docs/ci-philosophy.md`.
- **Platforms**: Linux runs the complete local gate; macOS and Windows run native tests, npm wrapper smoke and release builds.
- **Formatting**: `yarn format:check` is also part of the existing native CI tasks.
- **Evidence**: Source tests, released fixture execution and aggregate status are reported separately.
- **Workflow changes**: Present the completed diff and execution impact, then obtain the approval required by `AGENTS.md`.

---

## 17. Release Process

- **Changesets** for version management
- **Tag format**: `v{major}.{minor}.{patch}`
- **GitHub Release** with binaries + checksums
- **npm publish** automated
- Changelog auto-generated

---

## Enforcement

These principles are enforced through:

1. **CI gates**: fmt, clippy, behavior tests, audit must pass
2. **Code review**: All PRs reviewed for adherence
3. **Automation**: Pre-commit hooks, CI checks
4. **Architecture reviews**: For significant changes

Violations block merge. Exceptions require explicit approval and ADR documentation.
