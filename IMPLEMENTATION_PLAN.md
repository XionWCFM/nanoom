# Nanoom 개선 실행 계획 — LUNA 작업 명세

상태: **A1~A3 구현·local gates·parent review 완료. A4 local Rust/Action 구현과 회귀 완료; 공식 OpenAPI validator 실행은 DNS로 막힘. PR/hosted validation/release 미실시. A5~A7과 S0~S6 미완료.** 기준 source `539b2c08cc7e2543f3a0cdd10fbdba451b2502d5`(v0.6.0). 조사일 2026-09-24. 이 문서의 나머지 proposed 동작을 released product 기능으로 설명하지 않는다.

## 다른 세션에서 시작하기

Luna Max 실행 세션은 [LUNA_HANDOFF.md](LUNA_HANDOFF.md)에서 시작한다. 인계 브랜치는 `codex/prediction-state-v3`이며, 이 저장소 루트의 `IMPLEMENTATION_PLAN.md`가 상세 실행 계획이다. [SPEC](SPEC.md)으로 현재/제안 계약을 구분하고, [예측 모델](docs/prediction-model-spec.md), [선택적 서버](docs/history-server-spec.md), [OpenAPI](docs/api/history.openapi.yaml), [CHECKLIST](CHECKLIST.md)를 함께 읽는다. 현재 runtime 완료 범위는 A1~A4 local뿐이다. A4 공식 OpenAPI validator, hosted GitHub/GHES, release는 미검증이며 A5부터 계속한다. 결과와 미실시 항목은 CHECKLIST에 갱신한다.

문서 검증은 저장소 루트에서 실행한다.

```sh
uv run --no-project --with openapi-spec-validator --with pyyaml --with rfc8785 python docs/validation/check_prediction_spec.py
```

## 1. 목적과 고정한 결정

[모노레포의 CI는 어때야 할까?](https://xionwcfm.tistory.com/492)의 전체 CI 완료 시간·sparse checkout·운영 부담 관점을 따른다. 사용자 경로는 변경 감지 → 계획 → matrix → checkout/install → task → history → 다음 배분 → aggregate status다.

- 서버 없는 GitHub artifacts가 기본값이다. 작은 PR도 실행 계획은 항상 artifact로 전달한다.
- 이력이 충분하면 assignment 개수를 기본으로 자동 선택한다. 예상 완료 시간이 첫 목적, 동률일 때 총 runner 시간이 두 번째 목적이다. 느려지는 것을 허용하는 임의 비율은 없다.
- 계획 오류와 task 실패는 fatal, timing history 실패는 경고와 결정적인 cold fallback이다.
- 자체 task DAG는 만들지 않는다. Nx/Turbo 의존성 실행 또는 GitHub needs를 쓴다.
- `concurrency`는 Nanoom assignment 상한이며 `strategy.max-parallel`이 아니다.
- 최종 구현 납품은 producer PR + 실제 consumer fixture PR/CI까지다. merge/release하지 않는다.
- 선택적 **Rust History Server 구현 계획을 포함**한다. 서버 spec은 [서버 명세](docs/history-server-spec.md), HTTP source of truth는 [OpenAPI](docs/api/history.openapi.yaml)다. 서버 runtime과 배포는 문서 작성 후 S0~S6 단계에서 수행한다.
- 기존 `scheduler:http` live coordinator는 새 History Server와 별개다. coordinator/queue/lease를 이 서버에 구현하지 않는다.

현재 Nanoom checkout의 `.opencode/`와 fixture checkout의 사용자 변경을 보존한다. Nanoom은 인계 브랜치 `codex/prediction-state-v3`에서 이어서 작업한다. 별도 worktree를 만드는 경우 해당 브랜치의 문서 커밋을 포함한다. fixture는 별도 `codex/` worktree에서 작업한다. fixture는 stale local branch가 아니라 작업 시작 시 확인한 remote main을 기반으로 한다. reset/stash로 사용자 작업을 치우지 않는다.

## 2. 확인한 문제와 회귀 기준

| 문제 | 관측 / 수정 후 기준 |
|---|---|
| 계획 output 크기 | 512 workspace × 3 task, 상한 24에서 전체 groups/result가 output 한도 초과. 상세 payload를 artifact로 옮긴다. |
| 빈 assignment | 100/1/1/1 시간과 상한 4에서 빈 bucket 가능. 빈 assignment를 생성하지 않고 빈 install 대상의 전체 설치 확대를 거부한다. |
| shard identity | 2-shard 이력이 4-shard에 사용됨. totalShards를 identity에 포함한다. |
| task fallback | group median이 build/test를 섞음. 동일 task·shard layout·runner·environment만 fallback한다. |
| 준비 비용 미측정 | checkout/setup/install을 예측에 포함할 실제 관측값을 수집한다. unknown을 0ms로 취급하지 않는다. |
| 예제 불일치 | basic JSON/CLI 옵션과 advanced matrix 형식을 실제 consumer 경로에 맞춘다. |
| 완료 gate | 고정 job 이름·최소 개수가 아니라 실제 계획의 assignment/item과 실행 결과로 판정한다. |

기존 Rust 테스트와 Action contract가 통과했다는 조사 결과는 위 신규 동작의 검증 증거가 아니다.

## 3. Plan v1과 실행 Action 계약

### 데이터와 출력

Plan v1은 repository/workflow/run/producerAttempt/base/head, group별 assignments/items, checkout paths, runnerLabels/timingEnvironment, 실행 도구, 예측 근거, 선택한 prediction/model artifact 참조를 포함한다. assignment identity는 `(group, assignmentId)`. 예정 item이 정확히 하나의 assignment에 속하고 모든 assignment는 비어 있지 않아야 한다. 변경 없음은 유효한 0-assignment 계획이다.

계획 bytes SHA-256과 artifact 이름을 작은 `plan` 참조로 전달한다. 이름에 run/producerAttempt/planningJob 식별자를 포함하고 30일 보관을 요청한다. affected outputs:

- `has_change`: 작업 유무.
- `plan`: artifact/digest/run/producerAttempt/head 참조.
- `groups`: 기존 group별 matrix wrapper. 행마다 group/assignmentId/runnerLabels/timingEnvironment만 포함.
- `result`: bounded 개수/상태/이유 요약. 상세 JSON은 파일에 보관.

각 matrix 최대 256행과 실제 UTF-16 output 크기를 검사한다. 넘으면 해당 group과 원인을 표시하고 실패한다. 작업 버리기, 전체 실행 전환은 금지한다. 전체 JSON·checkout paths를 argv/env/output으로 되돌리지 않는다. CLI `affected --json` full report는 유지한다.

추가 CLI:

- `affected --plan-output FILE --plan-context FILE`: 기존 계산 결과를 계획 파일로 저장.
- `plan select --input FILE --reference FILE --group GROUP --assignment ID --output-dir DIR`: config/checkout 없이 validate/select; assignment context와 paths 파일 생성.
- `install --filter-file FILE`: JSON string array. 기존 --filter와 동시 사용 거부.

### prepare → install → run

prepare Action은 plan/group/assignmentId를 받아 공식 download action → metadata/hash/path 검증 → 공식 checkout을 수행한다. 격리 위치는 `$GITHUB_WORKSPACE/.nanoom/` 아래 run/attempt/job/matrix-index별이다. 정확한 plan head를 fetch-depth 1, non-cone 고정 root-only 패턴 `/*`와 `!/*/`로 받은 뒤, 실제 assignment paths 파일을 `git sparse-checkout set --cone --stdin`에 전달한다. root-only → 선택 workspace 추가는 로컬 Git PoC로 feasibility를 확인했다.

checkout은 dependency closure + checkout.always의 정렬된 디렉터리 합집합이다. 절대 경로/.. /control character 거부, cone mode root files 포함을 명시한다. prepare output은 assignment-file/cwd와 작은 identity뿐이다.

install/run Action은 기존 matrix 입력 대신 assignment-file을 읽는다. 매번 digest와 실제 Git HEAD를 검증한다. focused install은 root dev tools + workspace union + dependency closure를 포함하며 unrelated workspace를 제외한다. 빈 대상은 오류다. run은 계획된 item의 실제 실행을 확인하고 executions=0 성공을 금지한다. 최초 실패 이후 pending을 남기고 종료한다. Turbo 누락 시 다른 runner로 조용히 바꾸지 않는다. result는 작은 output + 로컬 상세 result-file, 대형 shell JSON 누적 금지.

이것은 다음 minor의 공개 계약 변경이다. legacy inline matrix 호환 경로는 만들지 않는다. 문서/예제/fixture를 함께 바꾼다.

### 재실행과 transport

같은 repository/workflow/run에서 `producerAttempt <= currentAttempt`이며 reference/artifact의 producerAttempt·digest·head가 일치하면 이전 attempt 계획을 재사용한다. 다른 run, 미래 attempt, hash/head 불일치, 없는 assignment/schema 오류는 task 전에 실패한다. 재계산/전체 checkout fallback 금지.

GitHub.com upload v4.6.2/download v4.3.0, GHES upload v3.2.2/download v3.1.0을 사용한다. affected-ghes와 prepare-ghes 추가, run/history GHES와 공유 검증 로직 사용. GHES 같은 run artifact는 공식 download action 사용; 완료된 과거 run만 REST로 조회한다. GHES history의 v3 전체 다운로드 후 sample payload 선별 비용을 명시하고 자체 artifact SDK는 만들지 않는다.

## 4. PredictionState v3·예측·자동 개수

공용 데이터 source of truth는 [예측 모델 명세](docs/prediction-model-spec.md)다. 기존 History v2 원본 sample 보관 제안을 대체한다. 공유 Rust 모듈의 compile-batch/apply-batch/project-predictions만 artifact와 서버가 재사용한다. scheduler는 작은 PredictionTable만 읽는다. GitHub 전송은 Action 계층, S3 전송은 선택적 server crate다. 단일 구현용 provider factory/trait나 서버 scaffold를 먼저 만들지 않는다.

- task identity: group/workspace/task/shard/totalShards/taskRunner/timingEnvironment. exact → workspace만 제외한 동일 task/layout/runner/environment fallback → cold 1. build/test나 shard 1/2와 1/4 혼합 금지.
- 원본 실행은 현재 attempt 집계에서만 실행 ID로 dedup한다. 이후 key별 날짜 count/sum 최대 7개와 짧은 batch receipt를 저장한다. 전체 raw history나 실행별 provenance를 누적하지 않는다.
- 최대 30 UTC일 안의 최근 관측 날짜 7개, 반감기 7일 가중 평균으로 예측한다. 기존 최근 7회 median과 다른 알고리즘이므로 outlier/급변 trace의 오차와 실제 makespan 비교가 필요하다.
- affected는 계산된 예측값·관측 수·최종 관측·유효 기한만 읽는다. model은 history updater만 읽는다. model 먼저, prediction publish marker를 마지막에 업로드한다. 손상 모델은 현재 관측으로 bootstrap하며 이전 평균을 가짜 sample로 재사용하지 않는다.
- versionless/v0.6/V2 파일은 schema-incompatible warning 후 cold/bootstrap한다. 같은 batch 재시도는 원래 bytes를 유지한다. receipt 8일, batch 입력 age 7일 미만, stats+receipt 원자 반영 규칙을 공용 명세대로 적용한다.
- PR은 같은 workflow/head repository/PR/head branch, 없으면 신뢰하는 base branch의 성공 push를 조회한다. push는 같은 workflow/branch 성공 push만 조회한다. 각 출처 최근 30일 성공 run metadata 최대 20개를 검토하되 공용 읽기 예산이 우선한다.
- affected가 선택한 prediction의 model pointer로 history를 갱신한다. artifact updater는 latest를 다시 찾지 않는다. 서로 병렬인 run의 독립 artifact 상태가 모두 합쳐지는 것은 보장하지 않는다. 이 경로는 best-effort 학습이며 원자적 다중 writer 병합은 선택적 서버가 제공한다.
- work item 0/1개, assignment cap 1, distribution 미설정으로 배분 선택지가 없는 group은 history 조회를 생략한다. 모두 해당하면 metadata를 포함한 history read 0회다. `history_not_needed`와 오류 fallback을 구분한다.
- planning 전체 이력 조회·다운로드·파싱은 **공유 3초 예산**이다. scope별 3초를 차례로 소비하지 않는다. JSON 8 MiB/개, archive 4 MiB/개, 모든 scope의 metadata+archive 수신 합계 8 MiB, scope별 후보 body 최대 2개. metadata 사전 검사 + 수신/압축 해제 크기 제한을 적용하고 초과 시 즉시 warning/cold다.
- 최적화 목적은 **이력 조회와 필요한 갱신 후처리까지 포함한 전체 CI 완료 시간 감소**다. 조회가 절약한 시간보다 오래 걸리면 성공이 아니다. timing lookup 실패와 fatal인 affected base SHA 선택 실패는 구분한다.

준비 구간은 prepare 시작부터 첫 task subprocess 시작 직전까지다. plan download/checkout/tool setup/install을 포함한다. task는 기존 monotonic 측정; wrapper와 upload 비용은 진단값; queue는 unknown이다. 여러 Action 사이의 timestamp 역전은 학습에서 제외한다.

준비 key는 group/runner environment/package manager+version/install mode/lockfile identity/정렬된 checkout+workspace 집합. head/plan digest를 넣지 않는다. exact 준비 예측 → 같은 group/환경/PM/설치모드/lockfile의 집계 예측 → unknown. paths에 임의 ms 계수를 곱하지 않는다.

N=item 수, C=min(선택 tier concurrency,N). 후보는 1, C 이하 2의 거듭제곱, C 이하 설정 tier concurrency, C. 기존 deterministic LPT로 각각 배분하고 empty/동일 결과를 제거한다. 기존 v0.6 layout도 후보에 포함한다. 비교는 `max(preparation + task sum)` → 총 runner ms → 전체 checkout paths → 실제 assignment 개수 → stable 순서다. 무설정 distribution은 기존 item별 assignment, N=0은 assignment 0개다.

task에 cold가 남거나 비교에 필요한 준비 예측이 없으면 상한 기반 LPT에서 empty만 제거하고 `cold-cap` 기록. 이력이 충분하면 기본 자동 선택. 유한 후보 최적이지 전역 최적이라고 주장하지 않는다. 이력 index는 한 번 만들고 bucket checkout 집합을 누적 관리한다. 단일 긴 item 병목은 shard 권고만 한다.

History API/artifact/업로드 문제는 구체 상태·이유·source·fallback을 남긴다. 성공한 batch만 병합하고 누락 telemetry는 degraded; correctness는 GitHub job 결과와 실제 item 실행으로 판단한다. blanket `|| true` 금지.

### 이력 증가량과 만료 인수

회당 planning payload와 updater state, 총 artifact 저장량을 각각 측정한다. 같은 key를 100,000번 관측해도 날짜 bucket 수는 7개 이하다. 새 key가 늘면 30 UTC일 pruning 이후 key 50,000개/model 16 MiB/receipt 4096개 중 먼저 닿는 cap을 적용한다. 저장 초과는 기존 상태 보존 + warning/degraded이며 80%부터 경고한다. 정상 PR이 반복적으로 cold가 되는 것은 완료로 인정하지 않는다.

서버 probe는 `/health`, `/ready`다. 유효 예측이 없으면 matching ETag보다 먼저 404다. S3 versioning 기본 off, opt-in noncurrent 1일 및 delete marker 정리, 비활성 model은 마지막 실제 변경 후 45일 lifecycle이다. 예측의 논리 만료와 비동기 물리 삭제는 별개다. model/prediction artifact는 30일, 현재 attempt 측정 artifact는 1일, task rerun용 Plan은 30일 보관한다. run별 사본의 빈도×크기×보관기간 총량도 별도로 보고한다.

## 5. LUNA 실행 카드

각 단계는 입력 확인 → 재현 회귀 → 최소 구현 → 실제 결과 검증 → 주 에이전트 리뷰 → CHECKLIST evidence 갱신으로 진행한다. 동시에 같은 파일을 수정하지 않는다. 주 에이전트가 공개 계약과 완료 여부를 책임진다.

| 단계 | 선행/입력 | 구현 산출물 | 합격 조건 |
|---|---|---|---|
| A0 | 기준 SHA와 dirty-tree 조사 | worktree, 이 계획/spec/checklist, 공개 계약 ADR | 사용자 변경 보존, 아직 미구현인 항목 명시 |
| A1 | 위 재현 입력 | identity/fallback/empty/실행0 회귀 및 수정 | 100/1/1/1, shard2→4, build/test, no-match 시나리오 통과 |
| A2 | A1, Plan v1 계약 | 파일 CLI, digest/provenance validator, compact matrix | 큰 plan·잘못된 ref·rerun·0작업 검증 |
| A3 | A2 | affected/prepare와 GHES wrappers, install/run 파일 입력 | 실제 sparse 범위·root tools·dependency closure·작업 실행 일치 |
| A4 | A1/A3 | v3 compile/apply/project·artifact 분리·bounded lookup | cold/warm·중복/만료·PR 격리·model download 0회 |
| A5 | A4 telemetry | 준비 예측/자동 k/index/진단 | 속도 우선·동률 비용·cold fallback·결정성·성능 검증 |
| A6 | A2~A5 | basic/advanced/Nx/Turbo/needs 예제, completion gates | 잘못된 skip/누락은 최종 실패, 0작업은 정상 skip |
| A7 | A6 | producer PR + fixture PR + candidate hosted runs | 동일 SHA Action/binary, cold→warm, positive jobs, aggregate 증거 |
| S0~S6 | A4 공유 PredictionState v3, A7 artifact 경로 보장 | 선택적 Rust History Server와 client opt-in | [서버 작업 카드](docs/history-server-spec.md)의 OpenAPI/S3/2-replica/인증/운영 인수 |

서버 phase는 공유 데이터 계약에 의존한다. 서버를 먼저 억지로 완성해 artifact 기본 경로를 우회하지 않는다. 계약 변경 시 관련 문서/CLI/Action/fixture/서버 spec을 함께 리뷰한다. 작업자가 새로운 공개 API, fallback, 데이터 손실 가능 구현을 임의로 선택하지 않도록 의문은 주 에이전트가 이 명세에 반영한다.

status Action은 optional `requiredJobs`를 추가한다. 그 이름의 job은 존재하고 success여야 한다. 나머지는 기존 success/skipped 정책이다. group에 작업이 있을 때 해당 run job을 required로 지정한다. completion script는 계획에서 expected assignment/item을 읽고 고정 최소 job 수 검사를 제거한다. local coverage 기준을 producer의 96%와 맞춘다.

## 6. 사용자 경로 acceptance와 검증

| 입력 | 결과 / 검증 |
|---|---|
| 512×3 tasks, C24 | artifact 계획, bounded output, item 정확히 1회 배정·실행 |
| 10,000 workspace generated fixture/history | argv/env 과대 JSON 없이 완주, 측정 시간의 중앙값/RSS/model·prediction·전체 artifact 크기 구분 |
| 100/1/1/1, C4 | empty bucket 0, full install fallback 0 |
| shard2→4 / build vs test / 다른 runner 환경 | 잘못된 exact/fallback 0 |
| 동일 batch 반복·expired/schema-old | 중복 가중치 0, 명시한 cold/fallback |
| 준비비용 큰 작은 PR / 같은 makespan | 실제 모델에 따른 k 선택 / 총 runner ms tie-break |
| corrupt plan/hash/head/없는 assignment | install/task 시작 전 실패 |
| 실패 job 재실행 | 검증된 이전 producerAttempt 계획 재사용 |
| history 권한/네트워크/upload 오류 | task 결과 유지, warning/degraded |
| 느린 이력/거짓 size/여러 scope | 전체 3초·합산 bytes 상한, model/sample planning 다운로드 0회 |
| 작은 PR/짧은 task | history off 대비 이력 I/O와 후처리를 포함한 전체 CI 시간 비교 |
| 계획 positive인데 실행0 또는 skipped | assignment/aggregate 실패 |
| no-change | assignment0, expected skip, aggregate 성공 |
| Yarn+Turbo / pnpm+Nx | root dev tools·내부 dependency closure 포함, unrelated 제외, dependency task 실행 |
| self-hosted | run/attempt/assignment별 checkout 격리 |

이력 조회를 포함한 전체 CI 측정은 wall clock 시작/종료와 critical path를 사용한다. 병렬 구간 시간을 단순 합산하지 않는다. `historyFetchMs`, plan 생성, checkout/install/task, history 갱신 시간을 별도로 기록하고 실제 사용자 대기 시간과 비교한다.

대규모 계획 생성은 같은 환경에서 기준/수정 버전을 반복 측정하고 중앙값으로 비교한다. 정상 입력에서 20% 이상 느려지는 회귀는 해결한다. 기존 output 초과 실패 입력은 완주와 자원 사용을 별도로 기록한다. 관측 전 성능 향상을 주장하지 않는다.

로컬 검증은 `cargo test --locked --all-targets`, 기존 fmt/lint/schema/Action 계약, 신규 실제 Git sparse 및 artifact/history/실패 경로 회귀를 실행한다. coverage만으로 완료를 선언하지 않는다. `scripts/review-change.sh`가 보는 committed diff와 실제 working diff를 구분한다. 문서 전용 단계에 의미 없는 runtime 테스트 변경을 하지 않는다.

producer의 생성된 local Git fixture는 remote head checkout 증거가 아니다. 실제 fixture remote main 기반 worktree에서 candidate commit SHA로 Actions를 고정한다. 같은 SHA의 binary를 준비 job에서 한 번 빌드·artifact 전달하고 `version:local`로 사용한다. @latest 이전 release로 후보 검증을 대신하지 않는다.

cold 실행 후 같은 workflow/branch의 warm 실행에서 loaded/source run/자동 선택 근거/non-skipped jobs/item 일치/aggregate를 확인한다. small/medium/full, no-change, 실패 전파를 구분한다. 서버는 별도로 2개 process + 실제 S3 조건부 쓰기 인수를 수행한다. mock/local compatible/AWS/hosted/released 증거를 섞지 않는다.

실제 GHES, 실제 AWS S3 또는 released consumer 증거가 없으면 해당 항목은 미실시다. 권한이 없을 때 새 계정/리소스를 자동 생성한 것으로 대신하지 않는다. merge·release·배포는 이번 문서 납품에 포함하지 않는다.
