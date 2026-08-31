# Nanoom v0.3 specification

공개 계약의 기준은 [README](README.md), 생성된 [JSON schema](nanoom.schema.json), [ADR-0009](docs/adr/0009-runtime-aware-distribution.md)입니다.

## Work item과 assignment

- work item: `(group, workspace, task, shard)`
- static matrix entry: `{ assignmentId, items, predictedDurationMs, reason }`
- continuous matrix entry: `{ agentId, runId, mode: "continuous" }`
- `distribution`이 없는 group은 legacy `{ name, task, shard?, totalShards? }` entry를 유지한다.
- `concurrency`는 Nanoom assignment 상한이며 GitHub `max-parallel`이 아니다.

## Timing

- 성공 subprocess wall time만 monotonic clock으로 ms 단위 측정한다.
- key: `group/workspace/task/shard/resolvedRunner/timingEnvironment`
- 예측: exact key 최근 7개 median → group median → `1`
- 배치: stable ID 동률 규칙을 가진 deterministic LPT
- history 실패: artifact/off는 equal-weight 폴백, 시작된 HTTP run은 실패

## Backend

- `off`: 외부 I/O 없이 정적 assignment
- `artifact`: 과거 history로 정적 assignment, 성공 sample과 병합 history를 30일 보관
- `http`: HTTPS coordinator claim loop와 30초 heartbeat

Artifact/history/coordinator는 aggregate status의 입력이 아니다. `status` Action은 `needs`만 평가한다.

## 제거와 제외

`isolate`는 v0.3.0에서 제거됐다. Task DAG, remote cache, flaky retry, Nx assignment rules, Nanoom server/SaaS는 범위 밖이다.
