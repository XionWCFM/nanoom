# ADR-0009: 실행시간 기반 분산 실행

- 상태: Accepted
- 대상 릴리스: v0.3.0

## 맥락

work item 하나당 runner 하나를 만드는 기존 matrix는 affected 규모와 task 비용을 반영하지 않는다. Nx의 historical timing과 continuous assignment는 유용하지만 Nx 설정 문법, task dependency graph, launch template, remote cache까지 Nanoom에 복제할 이유는 없다. Nanoom은 이미 모든 runner를 subprocess로 실행하므로 그 공통 경계에서 실제 wall time을 측정할 수 있다.

## 결정

1. group의 선택적 `distribution.small|medium|full`이 affected 비율별 assignment 수 상한을 정한다. 경계는 inclusive이고 모든 tier, 오름차순 임계값, `full=100`, 양수 concurrency를 검증한다.
2. work item identity는 `(group, workspace, task, shard)`다. 정적 matrix entry는 `assignmentId`, `items`, `predictedDurationMs`, `reason`을 제공한다.
3. 성공한 subprocess만 `group/workspace/task/shard/resolvedRunner/timingEnvironment/durationMs` sample이 된다. monotonic clock으로 측정하고 최근 7개 median, group median, cold-start 가중치 `1` 순으로 예측한다.
4. 정적 배치는 deterministic LPT다. 긴 item부터 현재 합계가 가장 작은 bucket에 넣고 stable work-item ID와 assignment ID로 동률을 푼다.
5. `scheduler: artifact`는 과거 history 조회 실패를 equal-weight로 폴백한다. sample/history artifact는 30일 보관하며 correctness나 aggregate status를 전달하지 않는다.
6. `scheduler: http`는 HTTPS `/v1` client만 제공한다. 시작된 run의 coordinator 오류는 실패이며 정적 모드로 전환하지 않는다. 모든 요청은 bearer token과 idempotency key를 사용하고 worker는 30초 heartbeat를 보낸다.
7. `isolate`는 관찰 가능한 독립 실행 계약이 없으므로 config, CLI, matrix에서 제거한다. shard 또는 별도 group이 명시적 대안이다.

## 사용자 경로 계약

- `affected --json`: 전체/affected workspace 수, affected percent, tier, concurrency, history status/reason/cost, assignment와 predicted duration을 설명한다.
- `install`: 정적 assignment의 workspace union을 한 번에 focused install한다. continuous agent는 미래 claim을 모르므로 전체 workspace closure를 설치한다.
- `run`: 정적 items를 순서대로 실행하거나 HTTP claim loop를 수행한다. 첫 실패 뒤 새 item을 시작하지 않으며 canonical JSON에 completed/failed/pending을 남긴다.
- `history`: 성공 sample과 기존 history를 병합해 key별 최근 7개만 남긴다.
- `status`: 오직 `needs` JSON을 평가한다.

## HTTP `/v1` 책임 경계

Nanoom client는 `POST /runs`, `POST /runs/{runId}/claims`, `PATCH /runs/{runId}/claims/{itemId}`, `POST /runs/{runId}/complete`만 호출한다. coordinator는 atomic claim, lease, 첫 만료 1회 재할당, 두 번째 만료 또는 실제 task 실패의 run failure 확정을 책임진다. dependency graph가 없으므로 모든 item은 즉시 ready다.

## 제외

Task dependency graph, remote task cache, flaky retry, agent type routing, Nx assignment rules/lifecycle/AI, 공식 Nanoom coordinator/server/SaaS는 구현하지 않는다. artifact backend는 사전 배치만 제공하며 work stealing을 주장하지 않는다.

## 회귀 및 인수 기준

- 단위/CLI: 0/25/25 초과/60/60 초과/100 경계, invalid tier, 최근 7개 median과 outlier, deterministic LPT, shard identity, 손상 history 폴백, multi-workspace install, 네 runner의 timing field.
- Action contract: assignment 정규화, 정확한 cwd/command, 30일 artifact, HTTPS/token/idempotency/heartbeat endpoint, needs-only status.
- hosted producer: cold-start run 뒤 history-loaded run에서 서로 다른 predicted assignment를 관찰한다.
- released consumer: Yarn Berry+Turbo와 pnpm+Nx에서 positive/non-skipped matrix, install, run, timing sample, history, aggregate status를 각각 확인한다.
- 완료 보고는 local, producer hosted, v0.3.0 binary/Action, released fixture, aggregate status를 분리한다. HTTP 외부 coordinator hosted 증거가 없으면 production hosted 완료로 표시하지 않는다.
