# Nanoom v0.3 specification

공개 계약의 기준은 [README](README.md), 생성된 [JSON schema](nanoom.schema.json), [ADR-0009](docs/adr/0009-runtime-aware-distribution.md)입니다.

## Work item과 assignment

- work item: `(group, workspace, task, shard)`
- static matrix entry: `{ assignmentId, items, predictedDurationMs, reason, checkout }`
- continuous matrix entry: `{ agentId, runId, mode: "continuous", checkout }`
- `distribution`이 없는 group은 legacy `{ name, task, shard?, totalShards?, checkout }` entry를 유지한다.
- `concurrency`는 Nanoom assignment 상한이며 GitHub `max-parallel`이 아니다.

## Timing

- 성공 subprocess wall time만 monotonic clock으로 ms 단위 측정한다.
- key: `group/workspace/task/shard/resolvedRunner/timingEnvironment`
- 예측: exact key 최근 7개 median → group median → `1`
- 배치: stable ID 동률 규칙을 가진 deterministic LPT
- history 실패: artifact/off는 equal-weight 폴백, 시작된 HTTP run은 실패

## GitHub revision resolution

- 명시적 Action `base`/`head`가 항상 우선한다.
- pull request와 merge queue는 기존 이벤트 base를 사용한다.
- push는 현재 workflow 파일·branch의 최신 성공 `push` run의 `head_sha`를 base로
  사용하고, 현재 run은 후보에서 제외한다.
- 성공 run 조회 실패는 CLI 전에 실패한다. SHA fetch는 CLI의 bounded blobless history
  경계에서 수행하며 fetch 또는 ancestor 검증 실패 시 Action output을 기록하지 않는다.
- Action result의 `revisionResolution`은 source, 실제 full SHA 범위, 성공 run ID를
  설명한다. CLI는 GitHub API를 호출하지 않는다.

## Sparse checkout

- `workspace.include`/`exclude`가 workspace manifest discovery의 유일한 범위다.
- affected checkout은 non-cone으로 root manifest, config, 해당 범위의 모든
  `package.json`을 포함해야 하며 누락 시 실패한다.
- shallow history fetch는 commit DAG만 받는 `--filter=tree:0`, `--no-tags`를 사용하고
  `affected.maxFetchDepth`에서 중단한다.
- matrix `checkout`은 `coneMode: true`와 affected workspace의 내부 dependency
  closure 및 `checkout.always` 디렉터리의 정렬된 합집합을 제공한다.
- configured workspace manifest의 삭제나 rename은 현재 manifest graph에 남아 있는
  모든 workspace를 affected 처리하며 `workspaceManifestStructure` reason을 기록한다.
- run별 self-hosted checkout 경로는 `.nanoom/` 아래에 격리한다. run Action의
  `cleanupCheckout: true`는 `if: always()` 단계에서 해당 경로를 검증한 후 삭제한다.

## Backend

- `off`: 외부 I/O 없이 정적 assignment
- `artifact`: 과거 history로 정적 assignment, 성공 sample과 병합 history를 30일 보관
- `http`: HTTPS coordinator claim loop와 30초 heartbeat

Artifact/history/coordinator는 aggregate status의 입력이 아니다. `status` Action은 `needs`만 평가한다.

## 제거와 제외

`isolate`는 v0.3.0에서 제거됐다. Task DAG, remote cache, flaky retry, Nx assignment rules, Nanoom server/SaaS는 범위 밖이다.
