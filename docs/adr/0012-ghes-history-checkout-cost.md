# ADR-0012: GHES 우선 history와 sparse checkout 비용

## Status

Accepted for v0.5.0.

## Context

task subprocess 시간만으로 LPT를 수행하면 runtime은 균등해도 같은 workspace와 dependency closure가 여러 assignment에 반복되어 GHES Git 부하가 커질 수 있다. 기존 artifact v4 backend는 GHES에서 지원되지 않고, repository 전체의 최신 이름만 조회하면 다른 workflow나 branch의 history가 섞일 수 있다. historical scheduling이 opt-in이면 released consumer가 equal-weight cold 배치에 머물기 쉽다.

## Decision

- artifact scheduler를 기본값으로 하고 `off`를 명시적 opt-out으로 둔다. 첫 run은 cold fallback을 허용하지만 표준 workflow는 run 뒤 history job을 실행한다.
- artifact upload의 기본값은 GitHub.com용 `artifactVersion: v4`와 `actions/upload-artifact@v4.6.2`다. GHES 사용자는 `run`과 `history`에 `artifactVersion: v3`를 명시해 Node 24 백포트 `v3.2.2`를 선택한다. 서버는 자동 감지하지 않으며 prefix fan-in은 공통 GitHub REST API로 구현한다.
- history는 같은 repository, workflow, branch의 마지막 성공·완료 run만 사용한다. 현재 run, 실패 run, 다른 provenance, expired/corrupt artifact는 제외한다.
- exact 최근 7개 median, group median, cold `1`과 deterministic LPT를 유지한다. 후보 비용은 `(predicted runtime makespan, total checkout path count, target bucket runtime, assignment ID)` 순서로 비교한다.
- checkout 비용은 assignment별 고유 closure path 수의 합이다. byte 추정을 위한 추가 Git object fetch나 사람이 관리하는 milliseconds weight는 사용하지 않는다.
- GitHub-hosted timing key는 OS/architecture, self-hosted는 OS/architecture/runner name을 기본으로 하며 명시적 environment override를 지원한다.

## Alternatives

- checkout path를 milliseconds로 환산하면 근거 없는 weight가 빠르게 낡으므로 제외했다.
- blob 크기 조회는 partial clone에서 추가 fetch를 일으켜 줄이려는 GHES 부하를 다시 만들 수 있어 제외했다.
- checkout/install/run을 소유하는 새 통합 Action은 기존 공개 workflow를 불필요하게 깨므로 제외했다.
- live work stealing은 artifact가 job 종료 뒤에만 생기므로 이 결정의 범위가 아니다.

## Acceptance criteria

- runtime makespan이 다른 후보에서는 runtime이 우선하고, 같은 후보에서는 전체 checkout path 합이 더 작은 배치를 고른다.
- canonical JSON이 history provenance, exact/group/cold sample coverage, 총/고유/중복 checkout path 수를 설명한다.
- v4 기본 경로, 명시적 v3 경로, 잘못된 version 거부, REST fan-in과 동일 workflow/branch 성공 run 선택을 Action contract로 검증한다.
- released `v0.5.0`과 `latest`를 사용하는 `nanoom-fixtures`가 small, medium, full의 cold/warm run, focused install, task, history, aggregate status를 모두 통과한다.
- 실제 GHES가 제공되지 않은 동안에는 공식 v3 contract와 GitHub.com hosted 실행만 증명하며 GHES hosted 검증을 주장하지 않는다.

## Consequences

runtime 균형을 희생하지 않는 범위에서 반복 sparse checkout closure를 줄인다. 첫 run은 history가 없어 cold이지만 이후 성공 run부터 재사용한다. GitHub.com은 v4, GHES는 명시적 v3로 플랫폼 제약을 드러낸다. released fixture가 개선을 보이지 않으면 v0.5.0 완료로 보지 않는다.
