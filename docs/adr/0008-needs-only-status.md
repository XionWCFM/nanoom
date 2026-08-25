# ADR-0008: Needs-only workflow status

- Status: Accepted
- Date: 2026-08-25

## Context

The public consumer workflow has one `affected` job, one conditional matrix `run` job, and one always-running `status` job. The status gate does not need to understand Nanoom's group or matrix semantics; GitHub already provides each dependency's result in `needs`.

## Decision

The status Action accepts only `needs`. It evaluates every dependency result as a set:

- `success` and `skipped` are accepted;
- `failure`, `cancelled`, missing, unknown, malformed, and empty input fail.

The Action does not accept or infer `affectedJob`, `matrixJob`, `group`, or `hasChange`. It exports one canonical result containing the sorted job results, final status, and reason. A consumer needs only:

```yaml
- uses: XionWCFM/nanoom/.github/actions/status@v0.2.8
  with:
    needs: ${{ toJSON(needs) }}
```

The real fixture remains responsible for proving that a positive affected change generated and executed every expected matrix entry. Status aggregation intentionally does not reimplement that product-specific assertion.

## Alternatives

- **Affected/matrix/group correlation:** rejected as unnecessary coupling to one workflow shape.
- **Treat only `success` as passing:** rejected because a no-change conditional matrix is intentionally skipped.
- **Accept every result except `failure`:** rejected because `cancelled`, missing, and unknown results must not produce a false-green gate.

## Acceptance criteria

- Focused Action tests cover all accepted and rejected result classes.
- The recommended consumer workflow contains only `needs` for status and no status checkout.
- Producer CI passes the Action contract and internal fixture aggregate.
- A released `nanoom-fixtures` workflow proves both a positive matrix run and an intentional no-change skipped run.
- The released Action tag selects its matching binary by default when a semver tag is used.
