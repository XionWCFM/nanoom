---
name: nanoom-change-review
description: Review Nanoom changes for missed regression tests, documentation, fixture, release, and completion-gate updates before merge.
---

# Nanoom change review

Use this skill for every non-trivial Nanoom change, especially CLI, composite Action, dependency/install, workflow, public contract, or release work.

The goal is to catch omissions before implementation is called complete. Review the real diff and its callers; do not approve based on coverage alone.

Read the repository root `AGENTS.md` first. Its released-CI and template-first principles are authoritative; see `docs/adr/0015-released-ci-and-template-first.md`. Inspect the actual consumer template before implementation details. Treat consumer jq/shell glue or required Nx/Turbo/package-manager overrides for ordinary repositories as product gaps. Operating CI must consume the latest released Action and its matching binary; candidate builds are test subjects, not substitutes for released-consumer evidence.

Before applying any `.github/workflows/**` addition, edit, deletion, or Action-version bump, present the complete proposed diff and trigger/permission/required-check impact and obtain explicit user approval. Prepare proposals outside the workflow directory until approved. Reconfirm changes outside the approved diff; do not manufacture approval evidence. Changes or exceptions to the root principles also require user approval.

## Required review sequence

1. Identify the changed public surface and the owning path: CLI, Action, workflow, package, docs, release, or fixture.
2. Trace producer → Action/CLI boundary → released binary (when applicable) → `nanoom-fixtures` consumer → aggregate status.
3. Require a regression test for every changed behavior and an edge/error test for every new branch or validation rule.
4. Check documentation in the same change. Public CLI/Action/output/configuration changes require the relevant reference page and ADR update when the contract or completion rule changes.
5. Check dependency and install behavior. Do not make a production-only install pass by moving test tooling into runtime dependencies; prove selected workspace, transitive closure, root tools, and unrelated-workspace exclusion in a clean fixture.
6. Check release impact: version synchronization, lockfiles, package wrappers, release smoke tests, and the released consumer path.
7. Run `bash scripts/review-change.sh <base-ref>` and record its output. Then run the applicable local gates and the released-consumer completion gates in ADR-0015 and the relevant feature ADRs.

## Stop conditions

Reject the change until fixed when any of these is true:

- a public behavior changed without a specification or regression test;
- a public contract changed without matching docs;
- a fixture-only or source-tree test substitutes for the released consumer path;
- an install/dependency change lacks a clean focused-install assertion;
- a workflow can skip work while aggregate status still passes;
- release/version evidence does not identify the exact source commit;
- the final JSON, reason, command, or selected commit cannot be explained from logs.
- operating CI bypasses the latest released product, or the basic template requires consumer glue or redundant tool configuration;
- workflow changes were applied without approval, or either repository's root principles were silently changed or waived.

## Review output

Report findings first, each with severity, file/line, concrete failure mode, and required fix. End with:

`PASS` only when no blocking finding remains and every applicable gate has evidence; otherwise `BLOCKED`.
