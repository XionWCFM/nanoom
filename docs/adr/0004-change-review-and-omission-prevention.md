# ADR-0004: Change review and omission prevention

- Status: Accepted
- Date: 2026-08-25

## Decision

Every non-trivial change gets two reviews before merge:

1. `bash scripts/review-change.sh <base-ref>` checks the mechanical evidence shape.
2. The independent Nanoom reviewer in `.codex/agents/nanoom-reviewer.md` checks the semantic path and reports `PASS` or `BLOCKED`.

The reviewer must verify implementation, regression/edge tests, docs, Action/CLI contract tests, focused install and transitive dependency behavior, release/version evidence, and the real fixture consumer path. The completion gates in ADR-0002 remain authoritative; this review does not replace them.

## Required evidence

The final change record names the source commit, changed public contract, tests added or updated, docs updated, fixture run, release/tag when applicable, and any intentionally skipped gate with an owner and expiry. A missing item blocks completion instead of becoming a follow-up task.

## Consequence

The shell check catches common omissions early, while the independent reviewer catches false positives and semantic gaps. Heuristics are deliberately not sufficient for approval.
