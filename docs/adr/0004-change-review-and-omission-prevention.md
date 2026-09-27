# ADR-0004: Change review and omission prevention

- Status: Accepted
- Date: 2026-08-25

## Decision

Every non-trivial change gets two reviews before merge:

1. `bash scripts/review-change.sh <base-ref>` checks the mechanical evidence shape.
2. The independent Nanoom reviewer in `.codex/agents/nanoom-reviewer.md` checks the semantic path and reports `PASS` or `BLOCKED`.

The reviewer must verify implementation, regression/edge tests, docs, Action/CLI contract tests, focused install and transitive dependency behavior, release/version evidence, and the real fixture consumer path. Root `AGENTS.md` and ADR-0015 define the current product principles and released-consumer completion gates; relevant feature ADRs add their specific checks. This review does not replace those gates.

## Required evidence

The final change record names the source commit, changed public contract, tests added or updated, docs updated, fixture run, release/tag when applicable, and any intentionally skipped gate with an owner and expiry. A missing item blocks completion instead of becoming a follow-up task.

## Consequence

The shell check catches common omissions early, while the independent reviewer catches false positives and semantic gaps. Heuristics are deliberately not sufficient for approval.

Pure dependency-version updates do not require meaningless touched test or documentation files. They reuse the existing regression, Action-contract, and release-contract gates; any behavioral change alongside the version bump remains subject to the normal omission checks.

배포 wrapper smoke는 Linux의 전체 게이트뿐 아니라 기존 macOS·Windows `test` 작업에서도 실제 해당 플랫폼 binary로 실행한다. Windows의 `.exe` 파일명과 optional platform package 경로를 검증하고 Unix의 executable bit 복구 검증을 유지한다. 테스트 추가는 기존 `scripts/ci-task.sh`에 연결하여 운영 템플릿을 늘리지 않는다.
