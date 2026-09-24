# ADR-0008: Needs-only workflow status

- Status: Accepted
- Date: 2026-08-25

## Context

The public consumer workflow has one `affected` job, one conditional matrix `run` job, and one always-running `status` job. The status gate does not need to understand Nanoom's group or matrix semantics; GitHub already provides each dependency's result in `needs`.

## Decision

The status Action accepts either the original `needs` JSON or newline-delimited `results` entries, plus an optional JSON array of caller-selected `requiredJobs`. It evaluates every dependency result as a set:

- `success` and `skipped` are accepted;
- `failure`, `cancelled`, missing, unknown, malformed, and empty input fail.
- each `requiredJobs` entry must be present and have result `success`; its missing or skipped result fails.

The Action does not infer `affectedJob`, `matrixJob`, `group`, or `hasChange`. The caller maps positive work to required job IDs, preserving an intentional no-change skip. It exports one canonical result containing the sorted job results, required job IDs, final status, and reason. A consumer needs only:

```yaml
- uses: XionWCFM/nanoom/.github/actions/status@v0.2.8
  with:
    needs: ${{ toJSON(needs) }}
```

Large matrix workflows should avoid carrying every job output into the aggregate process environment:

```yaml
- uses: XionWCFM/nanoom/.github/actions/status@latest
  with:
    results: |
      affected=${{ needs.affected.result }}
      run=${{ needs.run.result }}
      history=${{ needs.history.result }}
    requiredJobs: ${{ needs.affected.outputs.has_change == 'true' && '["run"]' || '[]' }}
```

The real fixture remains responsible for proving that a positive affected change generated and executed every expected matrix entry. Status aggregation requires the run job when the caller says work exists, but intentionally does not infer the plan or reimplement per-item execution checks.

## Alternatives

- **Inferring affected/matrix/group semantics:** rejected as unnecessary coupling to one workflow shape; callers pass only the job IDs that must succeed.
- **Treat only `success` as passing:** rejected because a no-change conditional matrix is intentionally skipped.
- **Accept every result except `failure`:** rejected because `cancelled`, missing, and unknown results must not produce a false-green gate.

## Acceptance criteria

- Focused Action tests cover all accepted and rejected result classes, required success/skip/missing, and invalid requiredJobs.
- The recommended consumer workflow uses `needs` for small graphs or compact `results` for large graphs, marks positive run jobs required, and uses no status checkout.
- Producer CI passes the Action contract and internal fixture aggregate.
- A released `nanoom-fixtures` workflow proves both a positive matrix run and an intentional no-change skipped run.
- The released Action tag selects its matching binary by default when a semver tag is used.
