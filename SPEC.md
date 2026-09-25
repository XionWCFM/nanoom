# Nanoom 공개 계약과 개선 명세

## 기준 구현: v0.6.0

아래 기존 계약은 source `539b2c08cc7e2543f3a0cdd10fbdba451b2502d5` 기준이다. 이 branch의 A1~A6 구현은 local Rust/Action gates를 통과했고 공식 OpenAPI schema/examples/digest 검증도 통과했다. A5 real trace 성능·오차·실제 artifact 크기와 A7 hosted producer/consumer 검증은 미완료다. Cloudflare Workers Free + D1 Worker는 배포되어 health/readiness가 통과했지만 인증 secret과 Actions client는 설정하지 않아 보호 데이터 경로는 아직 사용할 수 없다. release도 하지 않았다. 이 branch의 변경은 PR/hosted validation/release 전 후보 구현이다. 공개 계약의 기준은 [README](README.md), 생성된 [JSON schema](nanoom.schema.json), [ADR-0011](docs/adr/0011-sparse-checkout-plan.md), [ADR-0012](docs/adr/0012-ghes-history-checkout-cost.md), [ADR-0014](docs/adr/0014-prediction-state-v3-artifact-history.md)입니다.

## Work item과 assignment

- work item: `(group, workspace, task, shard)`
- static matrix entry: `{ assignmentId, items, predictedDurationMs, reason, checkout, predictedPreparationMs?, preparationPredictionSource?, preparationSampleCount?, schedulingMode? }`
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

`isolate`는 v0.3.0에서 제거됐다. Task DAG, remote cache, flaky retry, Nx assignment rules, Nanoom server/SaaS는 현재 배포 구현에 없다. 아래 선택적 History Server 제안과 구분한다.

## 이 branch의 A1 변경 — 미출시

- work item과 history identity는 `(group, workspace, task, shard, totalShards)`다. 샘플에서 `totalShards`가 빠진 기존 자료는 읽을 수 있지만, 분할 실행의 새 key와 섞지 않는다.
- exact sample이 없으면 workspace를 제외한 동일 group/task/shard layout/runner/environment의 median을 사용하고, 없으면 cold weight `1`을 쓴다.
- 명시한 `nanoom run --all --filter`가 workspace를 찾지 못하면 오류다. run Action은 성공 JSON이어도 계획된 workspace 실행이 없으면 assignment를 실패시키고 뒤의 item을 시작하지 않는다.
- static assignment의 빈 install은 오류다. continuous assignment는 미래 item을 알 수 없어 기존 전체 install 경로를 유지하며, standalone `nanoom install`도 필터 없이 root install을 유지한다.
- 100/1/1/1 시간 입력은 기존 scheduler에서 이미 빈 assignment 없이 결정적으로 처리되므로 배분 알고리즘을 변경하지 않았다.

## 이 branch의 A3 Action 계약 — 미출시

- 정적 `affected`는 Plan v1 file을 만들고 30일 artifact로 올린다. Action output은 `plan` reference와 group/assignmentId/runner matrix만 전달한다. 상세 Plan이나 checkout path 전체를 Actions output으로 되돌리지 않는다.
- `prepare`는 official artifact download와 checkout Action을 쓰며, original reference/digest/schema/provenance/assignment/current attempt/head를 확인한다. checkout은 `$GITHUB_WORKSPACE/.nanoom/<run>/<attempt>/<job>/<matrix-index>`에 root-only non-cone shallow checkout 후 assignment paths 파일을 cone mode로 적용한다.
- `install`과 `run`의 정적 경로는 `assignmentFile`과 original `plan` reference를 함께 요구하고 Plan digest 및 checkout `HEAD`를 다시 검증한다. static inline-matrix 호환은 없다. 기존 `scheduler:http` continuous-agent matrix와 전체 install 경로는 분리해 유지한다.
- GitHub.com artifact transport는 upload v4.6.2/download v4.3.0, GHES wrappers는 upload v3.2.2/download v3.1.0을 고정한다.


## 제안 계약: artifact plan / PredictionState v3 / 선택적 서버

전체 결정과 인수 기준은 [IMPLEMENTATION_PLAN](IMPLEMENTATION_PLAN.md)을 따른다. 아래 표는 branch-local 구현과 남은 제안 계약을 함께 설명한다. 어느 것도 release됐다고 가정하지 않는다.

| 경계 | 변경 |
|---|---|
| 실행 계획 | Plan v1 artifact, 작은 assignment matrix, digest/provenance 검증 |
| checkout | prepare가 exact head를 격리 checkout하고 paths 파일을 sparse --stdin으로 적용 |
| install/run | assignment-file 입력, empty/no-execution 성공 금지, 상세 결과 파일 |
| 이력 | PredictionTable/ModelState v3, planner는 예측값만 조회, updater는 날짜 count/sum·bounded batch dedup |
| 배분 | 관측 preparation+task 비용, tier cap 안에서 기본 자동 k, cold-cap |
| 상태 | requiredJobs로 예상하지 않은 skipped run 거부 |
| 서버 | Rust 별도 binary, opt-in historyBackend:server, 기본 artifact 유지, /health와 /ready |

현재 branch의 A2/A3 구현은 `affected --plan-output FILE --plan-context FILE`, `plan select --input FILE --reference FILE --group GROUP --assignment ID --output-dir DIR`, planned install의 `--filter-file`, 그리고 artifact-backed `affected→prepare→install→run` Action 경로다. 상세 plan은 파일에 저장하고 compact output은 group/assignmentId/runnerLabels/timingEnvironment만 matrix row로 전달한다. validator는 raw-file SHA-256, schema, repository/workflow/run/head provenance, 그리고 `producerAttempt <= current.attempt`를 확인한다. rerun consumer는 actual current run identity를 채운 reference를 제공한다. `install`과 `run`은 original reference를 받아 digest와 checkout HEAD를 매번 검증한다. GHES용 `affected-ghes`/`prepare-ghes`는 v3 artifact Actions를 쓴다. group별 256행 또는 UTF-16 출력 1 MiB 초과는 실패하고 zero-work Plan은 assignment 없이 유효하다. `--filter-file`은 non-empty JSON string array를 요구하고 malformed/non-array/non-string/empty/control-character entry 및 기존 `--filter`과의 동시 사용을 install 전에 거부한다. 필터 옵션 없는 standalone install은 root 설치를 유지한다.

A4 local 구현은 v3 MeasurementArtifact/ModelStateBundle/PredictionArtifact, 공용 deterministic compile/apply/project, PR-aware exact/fallback prediction lookup, bounded artifact read와 updater publish marker를 추가했다. `affected`는 선택지가 있을 때만 prediction artifact를 읽고 model/measurement는 planner에 전달하지 않는다. PR ScopeRef의 Rust wire field는 OpenAPI 계약에 맞춰 camelCase를 사용한다. A5 local 구현은 first-task start와 preparation start 사이의 telemetry를 선택적으로 기록하고, task/preparation history가 warm일 때 tier concurrency 상한 안에서 automatic assignment count를 고른다. unknown/cold이면 상한을 유지하며 선택 근거를 compact matrix와 scheduling result에 표시한다. Local 384-item synthetic scheduler 시간은 측정했지만 hosted preparation/task wall time과 real trace prediction error는 미실시다. CLI/Action regressions와 자세한 local/external evidence는 [CHECKLIST](CHECKLIST.md)에 기록했다. 공식 OpenAPI validator는 통과했고 actual hosted consumer는 아직 증명되지 않았다.

서버 HTTP source of truth는 [OpenAPI 3.1.1](docs/api/history.openapi.yaml), Cloudflare Workers/D1·인증·운영 규칙은 [서버 명세](docs/history-server-spec.md)다. API /v1, Plan v1, PredictionTable/ModelState v3 버전은 각각 독립적이다. 기본 배포 목표는 Workers Free + D1 + `workers.dev`이며, 무료 요청/row/storage/CPU 한도를 넘으면 요청이 실패한다. 기존 scheduler:http live coordinator와 새 History Server API를 혼합하지 않는다.

새 공개 계약은 다음 minor에서 workflow 예제/fixture/마이그레이션 문서와 함께 적용한다. 다음 중 하나라도 없으면 구현 완료가 아니다: 계약 회귀, docs, 실제 candidate consumer, positive 실행, 실패 전파. released 검증은 release 후 별도 증거다.

데이터와 산식은 [예측 모델 명세](docs/prediction-model-spec.md)를 따른다. raw 실행 history 보존이 목표가 아니다. key당 최대 30 UTC일 내 최근 7개 날짜의 count/sum을 유지하며 가중 평균을 계산한다. 기존 median과 정확도 차이는 후속 구현의 비교 대상이다. 현재 attempt 측정 artifact는 1일, model/prediction 및 Plan artifact는 30일이다.

planning 전체 이력 I/O·파싱의 공유 budget은 3초이며 크기/시간 초과는 cold fallback한다. planner는 model을 다운로드하지 않는다. artifact 학습 state는 16 MiB/50,000 keys/4096 receipts 상한, 80% 경고다. D1 Worker는 scope row 1.9 MB로 제한하며 비활성 state는 45일 뒤 daily cron이 삭제한다. D1 Free storage/query 한도나 Workers CPU를 넘으면 호출이 실패할 수 있다. artifact run별 사본 총량도 별도 측정한다. 이력 조회와 갱신을 포함한 전체 CI가 느려지면 성능 개선 완료로 인정하지 않는다.
