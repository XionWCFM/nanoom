# 개선 계획 완료 체크리스트

기준: source `539b2c08cc7e2543f3a0cdd10fbdba451b2502d5`. 체크는 **이번 변경에 대한 증거**에만 붙인다. 이전 v0.3/v0.6 완료 표시를 새 구현의 완료로 승계하지 않는다.

## 문서 단계

- [x] A0~A7 artifact/자동배분 실행 명세 작성.
- [x] S0~S6 선택적 Rust History Server 실행 명세 작성.
- [x] OpenAPI 요청/응답/schema/오류/권한/조건부 조회 계약 작성.
- [x] OpenAPI 표준 validator, examples/schema, digest vectors 검증.
- [x] 문서 간 기본값/한도/링크/현재 vs proposed 일치 검토.

## artifact/CLI/Action 구현

- [x] A1 identity/fallback/empty/no-execution 회귀와 수정 — 아래 A1 실행 기록 참조.
- [x] A2 Plan v1 CLI·compact matrix·provenance/rerun·planned install filter-file 검증 — 아래 A2 실행 및 follow-up 기록 참조.
- [x] A3 prepare·sparse checkout·install/run·GHES local contracts — 아래 A3 실행 기록 참조. Hosted GitHub/GHES transport는 별도 미실시.
- [x] A4 PredictionState v3·compile/apply/project·분리된 artifact·bounded lookup의 local Rust/Action 구현과 회귀.
- [x] A5 preparation telemetry·automatic k·cold fallback·결정성·384-item synthetic warm scheduler benchmark — 아래 A5 기록.
- [ ] A5 real CI traces의 전체 wall time·prediction error vs median·실제 artifact 크기 측정 — hosted consumer 경로 필요.
- [x] A6 examples·requiredJobs·계획 기반 completion gate·96% coverage 기준 — 아래 A6 실행 기록.
- [x] Local fmt/lint/tests/Action/실제 Git 및 focused-install 회귀 — A4/A6 공통 로컬 gates 포함.
- [ ] 현재 A4 OpenAPI 변경의 공식 schema/examples/digest 검증 — uv 설치는 PyPI DNS로 미실시.
- [ ] Producer hosted PR CI: exact candidate SHA 기록.
- [ ] Consumer Yarn+Turbo / pnpm+Nx: cold→warm, positive/non-skipped, selected closure.
- [ ] Consumer no-change / task failure / unexpected skip / rerun aggregate 검증.
- [ ] A7 producer PR + fixture PR + run URL / SHA / 결과 표 첨부.

## A6 실행 기록 — 2026-09-24

```text
단계: A6 local implementation
시작 HEAD: 3256597 (A5 validation evidence)
브랜치: codex/prediction-state-v3
시작 상태: A5 tracked tree clean; 사용자 .opencode/ 미추적 상태 보존.
```

basic/advanced 예제에 positive, no-change, required status job wiring을 보이고 Nx/Turbo 입력도 명시했다. status Action의 `requiredJobs`는 지정 job의 존재와 success를 요구하며, 그 밖의 dependency는 기존처럼 success/skipped를 허용한다. fixture completion gate는 고정 job 개수 대신 실제 Plan artifact의 assignment/item 수와 run 결과를 대조하고 no-change의 skip을 허용한다. hosted completion 검증도 대상 run attempt의 Plan artifact를 받아 같은 계약을 검사한다.

회귀는 required job 누락/skip/입력 오류, positive Plan의 실행 누락·unexpected skip, no-change, Plan count 변조, 외부 Plan/reference/matrix 검증, affected 사유 및 context 제한을 다룬다. 로컬 공통 완료 스크립트와 coverage 기준을 96%로 올려 통과시켰다. 실제 GitHub/GHES hosted artifact 전송, 외부 candidate SHA PR/run, released binary 증거는 이 로컬 결과로 승계하지 않는다.

검증:

- `bash scripts/verify-completion.sh --local` — exit 0, local completion gate passed; full Rust tests, fmt/clippy, coverage, Action/completion contracts, smoke/install/platform gates 포함.
- 공통 `cargo llvm-cov --locked --workspace --all-features --fail-under-lines 96 --summary-only` — exit 0, line coverage **96.03%** (7,865 lines, 312 missed).
- `git diff --check`, `cargo fmt --all --check` — exit 0.
- `gh auth status` — exit 1; `github.com` 기본 계정 `XionWCFM` token invalid. Producer/fixture hosted PR 및 run을 만들거나 hosted evidence를 수집하지 않았다.
- `.opencode/`는 계속 미추적이며 stage하지 않았다.

미실시 및 다음 단계: A7 producer/fixture PR 및 hosted cold→warm/positive/no-change/failure/rerun 증거, GitHub/GHES artifact transport, released binary, A5 real trace/정확도/실제 artifact bytes, 공식 OpenAPI validator, S0~S6 server runtime. invalid GitHub token을 다시 인증한 뒤 A7 candidate SHA hosted evidence부터 진행한다.

## 선택적 서버 구현

- [ ] S0 공유 PredictionState v3 / RFC8785 vectors / batch 원자 병합 회귀.
- [ ] S1 별도 server crate / OpenAPI boundary / exact auth ACL.
- [ ] S2 2-process S3 conditional write / lost response / corruption / capacity.
- [ ] S3 /health·/ready/probe/drain/OCI 실행 검증.
- [ ] S4 artifact 기본값 유지 / server opt-in / 오류 fallback / token 비노출.
- [ ] S5 실제 AWS S3 + 2 replicas + hosted consumer warm reuse.
- [ ] S6 서버 PR / 운영 runbook / 자원·성능 측정 / 문서 업데이트.

## 별도 외부 증거와 범위

- [ ] 실제 GHES: 환경 미확보 시 미실시. 로컬 wrapper 검증과 분리.
- [ ] 실제 S3-compatible 제품/버전: 계약 통과 전 지원 주장 금지.
- [ ] Released Action/CLI/server consumer: release하지 않은 후보 증거와 분리.

runtime 구현은 A4 실행 기록을 따른다. 서버 배포·AWS 리소스 생성·merge·release는 없다. 각 phase의 증거는 명령/환경/시나리오/result/commit/run URL 및 미검증 범위를 함께 기록한다.


## 문서 검증 evidence — 2026-09-24

[문서 검증 스크립트](docs/validation/check_prediction_spec.py)를 저장소 dependency 변경 없는 임시 uv 환경에서 실행했다.

```sh
uv run --no-project --with openapi-spec-validator --with pyyaml --with rfc8785 python docs/validation/check_prediction_spec.py
```

- OpenAPI 3.1.1: schema 22개, schema/요청/응답 examples 19개 PASS.
- RFC8785: scope/key/batch ID, request digest, prediction ETag, model digest 6개 PASS.
- unknown field·빈 aggregate·잘못된 version/ID/attempt·음수 count/duration·불완전 tuple·raw samples 유입 등 잘못된 입력 9개 거부.
- 문서 local file links와 `git diff --check` PASS. `/health`, `/ready` 명명 확인.
- 1k/10k/30k task key + 103 fallback/preparation keys, 4096 receipts의 synthetic payload/schema 검사 PASS. [크기와 한계](docs/prediction-model-spec.md#7-문서-단계-합성-크기-측정--2026-09-24)를 기록했다.
- 7→700,000회 합계 prototype에서 bucket 개수 7 유지, integer 자릿수만 증가. runtime 집계 구현의 증거는 아니다.

앞선 raw-sample V2 계약의 결과를 승계하지 않고 최종 v3 문서를 다시 검증했다. 검증 환경의 jsonschema `RefResolver` deprecation warning은 있으며 실행은 exit 0이다.

LUNA의 읽기 전용 분산 설계 검토를 반영했다. 추가 LUNA 문서 작성 호출은 사용 한도로 중단되어 주 에이전트가 문서를 작성·검증했다. LUNA가 최종 문서를 재승인한 것으로 표시하지 않는다.

## 용량·지연·보존 추가 인수

- [ ] 같은 key 100,000회 → 최대 7개 날짜 count/sum, raw 배열 증가 없음; duplicate batch 가중치 증가 없음.
- [ ] 순서 변경·2-process CAS·응답 유실 → 동일 count/sum, stats+receipt 원자성.
- [ ] 새 key 증가 → 날짜 prune, 50,000 keys/16 MiB/4096 receipts cap, 80% 경고·기존 모델 보존.
- [ ] input age 7일/receipt 8일 경계, clock 역전, 정확한 expiry/validUntil 및 전부 만료 404.
- [ ] 1k/10k/30k task keys + fallback/prep: model/projection raw·archive bytes와 총 저장량 구분.
- [ ] 0/1 item·cap 1·고정 legacy 배분만 있는 입력은 history read/metadata 요청 0회.
- [ ] planning model/sample download 0회, 여러 scope를 포함한 공유 3초 예산·합산 8 MiB.
- [ ] 느린 stream/허위 Content-Length/과대 artifact/압축 폭탄 → bounded cold, task 결과 유지.
- [ ] 작은 PR/짧은 task/느린 history: history off 대비 전체 CI 시간과 historyFetchMs 비교.
- [ ] daily weighted mean vs 기존 median: 이상치·급변·드문 실행의 오차와 실제 배분 성능 비교.
- [ ] 실제 artifact 측정 1일/model+prediction 30일/Plan 30일 보관, 자료 없는 rerun degraded.
- [ ] S3 current 45일, opt-in noncurrent 1일, delete marker 정리 설정과 실제 지연 구분.
- [ ] versioning 기본 off, 선택적 on, artifact run 빈도에 따른 총량 비교.

위 항목은 후속 runtime 인수이며 문서나 합성 bytes 측정으로 완료 처리하지 않는다. 서버 runtime, 실제 S3 write, 2-replica 정합성, hosted CI, 실측 성능은 미실시다.

## 저장소 내 인계 확인 — 2026-09-24

Nanoom 작업 폴더에 계획/명세/OpenAPI/검증 스크립트 7개를 반영하고 IMPLEMENTATION_PLAN에 다른 세션의 시작 절차를 추가했다. 저장소 루트에서 OpenAPI 22 schemas·19 examples·6 digest vectors·9 invalid cases·25 local links를 재검증했다. `git diff --check` 통과. `review-change.sh HEAD`는 committed diff가 없어 no changes를 반환했으므로 현재 미커밋 문서 변경의 리뷰 증거로 계산하지 않았다. 문서 이동으로 runtime 완료 상태는 바뀌지 않는다.

## Luna Max 인계 준비 — 2026-09-24

- [x] 현재 checkout에서 `codex/prediction-state-v3` 브랜치 생성, 사용자 `.opencode/` 보존.
- [x] [실행 지시서](LUNA_HANDOFF.md)에 A1 재현·소스 위치·단계 순서·검증·재개 형식 작성.
- [x] 다른 checkout에서도 읽을 수 있도록 계획·명세·검증 스크립트·지시서만 인계 commit에 포함.
- [x] A1 runtime을 이 세션에서 실행·검증함. A3~A7과 서버 단계는 위 체크처럼 미완료.

## A1 실행 기록 — 2026-09-24

```text
단계: A1
시작 HEAD: 0aa7e968db4cfb2418c38e11fdebff7440932384
브랜치: codex/prediction-state-v3
시작 상태: 추적 파일 변경 없음, 기존 사용자 파일 .opencode/ 미추적 1개. 보존함.
결과: A1 구현은 local gate 통과 후 commit `3fe56ee`로 기록함. `scripts/action-contract.sh` 보강은 후속 commit에 포함. PR/release/외부 hosted 실행 없음.
```

| 입력과 경계 | 시작 시 결과 | 수정 후 결과 / 확인 |
|---|---|---|
| 시간 100/1/1/1, concurrency 4 | 기존 LPT 구현에서 빈 assignment가 나오지 않았고 4개 모두 1회 배정됨. | scheduler 알고리즘은 바꾸지 않음. `skewed_four_item_plan_has_no_empty_assignments`에서 비어 있지 않은 배정, 각 항목 1회, 반복 실행 결정성을 유지 확인. |
| shard 1/2 관측 → shard 1/4 요청 | `TimingSample`에 `totalShards`가 없어 병합 입력이 거부되고 배분 key에서도 구분할 수 없었음. | Action producer와 serde wire에 선택적 `totalShards` 추가. exact/fallback/merge identity 및 안정 정렬 key에 반영. 1/2 자료만 있는 1/4 요청은 cold, merge 후 1/2와 1/4 행 모두 보존됨. 기존 필드가 없는 raw sample도 계속 읽음. |
| 같은 group의 build/test 및 다른 runner/environment/layout | group fallback이 group+runner+environment만 비교해 다른 task/layout을 섞음. 재현값은 기대 25ms, 실제 162ms. | workspace를 제외하고 group/task/shard/totalShards/runner/environment가 모두 같은 관측만 fallback으로 사용. build, runner, environment, layout이 다른 표본은 제외됨. |
| 계획된 item no-match 또는 executions 0 | `nanoom run --all --filter missing` 성공 종료했고 Action도 빈 `executions`를 성공으로 셈. | 명시한 `--all --filter`에 대한 no-match는 CLI 오류. Action은 성공 JSON이어도 해당 workspace execution이 없으면 실패 처리하고 후속 pending item을 시작하지 않음. filter 없는 affected no-change 경로는 변경하지 않음. |
| static assignment의 빈 install | Bash 3.2에서 빈 배열 반복이 `names[@]: unbound variable`로 비정상 종료했고, 코드 경로상 필터 없는 전체 설치 명령까지 도달할 수 있었음. | 빈 static assignment는 명시적 오류로 거부하고 CLI를 호출하지 않음. `mode=continuous`의 빈 item 목록은 기존대로 필터 없는 전체 설치를 수행. 일반 CLI `install`의 필터 없음도 root 설치로 유지. |

수정 전 재현:

- `cargo test --locked --lib scheduler::tests::skewed_four_item_plan_has_no_empty_assignments` — exit 0; 이 사례는 기존 구현의 결함으로 재현되지 않아 scheduler 변경을 하지 않음.
- `cargo test --locked --lib scheduler::tests::group_fallback_only_uses_the_same_task_runner_and_environment` — exit 101; 기대 25, 실제 162.
- `cargo test --locked --test cli_integration_tests history_keeps_samples_with_different_shard_layouts_separate` — exit 101; `totalShards`가 unknown field라며 history 병합 거부.
- `cargo test --locked --test cli_integration_tests explicit_planned_run_filter_fails_when_no_workspace_matches` — exit 101; 명시한 계획 항목 no-match가 성공 no-op.
- `bash scripts/assignment-action-test.sh` — exit 1; CLI 성공/`executions: []`가 assignment 성공으로 처리됨.
- Bash 3.2에서 빈 static install Action 직접 실행 — exit 1, `names[@]: unbound variable`; CLI 미호출. 연속 install도 동일한 빈 배열 반복 경로라 별도 호환 회귀로 고침.

수정 후 관련 검증:

- `cargo fmt --all --check` — exit 0.
- `cargo test --locked --lib scheduler::tests` — exit 0, 13 passed.
- `cargo test --locked --test cli_integration_tests history_` — exit 0, 3 passed.
- `cargo test --locked --test cli_integration_tests explicit_planned_run_filter_fails_when_no_workspace_matches` — exit 0, 1 passed.
- `cargo test --locked --lib commands::install::tests::execute_runs_only_the_root_install_without_network` — exit 0, 1 passed.
- `bash scripts/assignment-action-test.sh` — exit 0. 빈 execution은 failed item 처리, 후속 item 미실행; producer sample의 shard/totalShards 보존; static empty install은 CLI 미호출; continuous empty install은 no-filter install 유지.
- `cargo fmt --all --check` — exit 0.
- `cargo clippy --locked --all-targets --all-features -- -D warnings` — exit 0.
- `cargo test --locked --all-targets --all-features` — exit 0, 186 tests passed. 첫 전체 실행에서 이전 group fallback을 가정하던 affected expectation이 실패해, unsharded `test` 표본이 sharded `build`에 섞이지 않는 기대값(79ms, exact 1/cold 2)으로 고친 뒤 전체 통과.
- `bash scripts/action-contract.sh` — exit 0; status/coordinator/assignment/history/revision/checkout-cleanup/fixture-completion contracts 모두 통과.
- `bash scripts/review-change.sh 0aa7e968db4cfb2418c38e11fdebff7440932384` — exit 0, committed diff 12 files, PASS.
- `git diff --check` — exit 0.

미실시 및 경계: 지시서의 OpenAPI/spec validator는 기본 uv cache 권한 오류(exit 2) 뒤 `/private/tmp` cache로 재시도했으나 PyPI DNS 차단(exit 2)으로 실행되지 않음. A1의 external producer/consumer hosted fixture 증거는 A7까지 미실시다. A3~A7, server, GHES, release/실사용 consumer 증거는 미완료이며 기존 체크를 변경하지 않음.

## A2 실행 기록 — 2026-09-24

```text
단계: A2
시작 HEAD: d3f147a18372d94d42e9e15f0cefde51cd44d5ab
브랜치: codex/prediction-state-v3
시작 상태: 추적 파일 clean, 사용자 .opencode/ 미추적 1개. 보존함.
변경: Plan v1 writer/reference, compact output, config-free assignment selector, `install --filter-file`, integration/unit regressions, README/SPEC/ADR contract.
결과: Plan CLI commit `eb43d00`, filter-file commit `9d3a63f`, ADR contract commit `36250d2`. PR, artifact transport, checkout/install/run Action consumer, hosted run은 없음.
```

Plan producer는 `--plan-output`과 `--plan-context`를 함께 요구하고 raw Plan bytes의 SHA-256과 current/source provenance를 작은 reference로 출력한다. group별 최대 256 matrix rows와 compact JSON의 UTF-16 크기를 검사한다. Plan selector는 config/checkout 없이 Plan/reference/schema/digest/current run/head와 assignment를 검증하고 `assignment.json`, `paths.txt`를 만든다. no-change는 유효한 빈 Plan이며 선택할 assignment는 없다. Affected가 만든 reference는 producer attempt를 current로 초기화한다. rerun consumer는 실제 current run/attempt를 reference에 채워야 하며 Action transport는 A3다.

첫 CLI 재현에서 legacy matrix의 workspace `path`가 절대경로이고 checkout path가 repo-relative여서 Plan 검증이 실패했다. 기존 matrix 호환성은 유지하고 Plan v1 item 경로만 repo-relative로 정규화한 뒤 통과했다.

검증한 사용자 경로와 결과:

- 10,000 item synthetic Plan에서 상세 파일이 1 MiB보다 큰 동안 compact output은 bounded JSON이며 24-row matrix와 item count를 유지했다.
- 257-row group은 compact output 전에 명시적으로 거부됐다.
- digest reference 변조, raw Plan tampering, repository/run/head 불일치, 존재하지 않는 assignment는 assignment 파일 생성 전에 실패했다.
- 같은 run의 이전 producer attempt는 reference current attempt가 이후 attempt로 제공되면 선택 가능했다.
- no-change Plan은 0 assignment로 유효했고 선택 시 존재하지 않는 assignment 오류로 실패했다.
- `--json` full report와 Plan bounded output을 함께 요청하면 Plan 파일을 쓰지 않고 거부했다.
- Plan context의 taskRunner가 affected의 resolved runner와 다르면 Plan 파일을 쓰지 않고 거부했다.

검증 명령 / 결과:

- `cargo fmt --all --check` — exit 0.
- `cargo clippy --locked --all-targets --all-features -- -D warnings` — exit 0.
- `cargo test --locked --all-targets --all-features` — exit 0, 198 passed.
- `bash scripts/action-contract.sh` — exit 0.
- `git diff --check` — exit 0.
- `bash scripts/review-change.sh d3f147a18372d94d42e9e15f0cefde51cd44d5ab` — exit 0, 13 committed files, heuristic PASS; parent semantic review PASS.
- `cargo test --locked --lib plan::tests` — exit 0, 6 passed.
- `cargo test --locked --test plan_cli_tests -- --nocapture` — exit 0, 6 passed.
- `cargo test --locked --test cli_integration_tests test_affected_pull_request_event_with_changes` — exit 0, 1 passed.

미실시와 다음 단계: artifact upload/download, Action current-attempt construction, sparse checkout, install/run Action consumer, same-SHA hosted fixture, GHES, release는 미실시이며 A3/A7 경로다. `affected --json`은 기존 상세 보고서로 유지되고 bounded Plan mode는 별도 CLI 응답이다. 다음 runtime 단계는 A3 prepare/checkout/install/run 파일 입력과 GHES wrapper다.

### A2 planned install filter-file follow-up — 2026-09-24

기준 HEAD: 1f4796977af1de7326949c7f7f48df2a5f066f51. branch: codex/prediction-state-v3.
시작 상태: tracked tree clean. 기존 사용자 파일 .opencode/만 untracked이며 보존.
재현: parent contract audit에서 README/IMPLEMENTATION_PLAN이 가리키는 planned install input인 --filter-file이 현재 InstallArgs/CLI에 노출되지 않은 점을 확인함.
수정: 기존 focused install 필터 경로를 재사용하는 --filter-file JSON string-array CLI 추가. 빈 배열 및 잘못된 item은 package manager 탐지/실행 전에 거부하고 standalone 무필터 install은 root install로 유지.
상태: A2 follow-up와 공통 gates, parent review를 마쳐 A2를 완료함.

회귀 사용자 경로:

- ["pkg-a","pkg-b","pkg-a"] 입력은 기존 pnpm focused args로 전달하고 중복을 제거한다.
- malformed JSON, object/non-array, number/non-string, 빈 배열, 빈/공백 문자열, newline/NUL 제어문자는 package manager를 실행하기 전에 거부한다.
- --filter와 --filter-file 동시 사용을 거부한다.
- 두 옵션 모두 없는 standalone install은 계속 root npm install을 실행한다.

검증:

- cargo test --locked --test install_filter_file_tests -- --nocapture — exit 0, 4 passed.
- cargo test --locked --lib commands::install::tests — exit 0, 14 passed.
- cargo fmt --all --check — exit 0.
- cargo clippy --locked --all-targets --all-features -- -D warnings — exit 0.
- cargo test --locked --all-targets --all-features — exit 0, 202 passed.
- bash scripts/action-contract.sh — exit 0.
- git diff --check — exit 0.
- bash scripts/review-change.sh 1f4796977af1de7326949c7f7f48df2a5f066f51 — exit 0, 6 changed files, parent semantic review PASS.

## A3 실행 기록 — 2026-09-24

```text
단계: A3
시작 HEAD: 6d3f221
결과 commit: c372891 feat: wire artifact-backed assignment actions
브랜치: codex/prediction-state-v3
시작 상태: A2 tracked tree clean; 기존 사용자 .opencode/ 미추적 1개 보존.
결과: Plan artifact producer→prepare→exact-head sparse checkout→focused install→planned run의 Action 경계를 구현했다. GitHub.com v4와 GHES v3 artifact wrappers 및 shared assignment validator를 추가했다.
```

로컬 Git fixture에서 실제 `affected --plan-output`으로 Plan/reference를 만들고 rerun attempt에서 선택했다. digest 변조를 거부하고 exact head를 shallow clone해 root-only non-cone에서 계획된 cone으로 전환했다. 내부 dependency와 `checkout.always` 경로가 존재하고 unrelated workspace/tool은 없는 것을 확인했다. static install은 실제 Nanoom CLI와 fake pnpm shim으로 filter-file argument를 확인했고, run은 fake pnpm을 거쳐 fixture의 실제 npm test script를 실행했다. 이는 local shell/Git 검증이며 GitHub Actions hosted transport 증거가 아니다.

run Action 회귀는 no execution 및 task failure가 assignment를 실패시키고 후속 item을 실행하지 않는 것, pending item을 로컬 상세 JSONL에 보존하는 것, static output에서 전체 item 배열을 내보내지 않는 것, 계획된 taskRunner override를 거부하는 것, artifact sample v3/v4와 continuous full-install 동작을 확인했다.

검증:

- `cargo fmt --all --check` — exit 0.
- `cargo clippy --locked --all-targets --all-features -- -D warnings` — exit 0.
- `cargo test --locked --all-targets --all-features` — exit 0, 202 passed.
- `bash scripts/action-contract.sh` — exit 0. status/coordinator/assignment/history/Plan/revision/cleanup/fixture-completion contracts 포함.
- `bash scripts/plan-action-test.sh` — exit 0; producer, rerun, digest, exact-head shallow sparse checkout, focused install, real fixture task script.
- `bash scripts/assignment-action-test.sh` — exit 0; execution failure/empty execution/runner switch/compact outputs/pending detail/static install/continuous install.
- `bash scripts/revision-action-test.sh` — exit 0.
- 변경한 Action/test shell `bash -n` — exit 0; Action YAML Ruby parse/input descriptions는 `action-contract.sh`에서 통과.
- `git diff --check` — exit 0.
- `bash scripts/review-change.sh 6d3f221` — exit 0, 25 committed files; heuristic PASS. Parent semantic review PASS.

미실시 및 경계: 실제 GitHub.com `upload-artifact`/`download-artifact`, GHES v3 transport, released binary/Action, hosted producer/consumer fixture, PR/run URL은 없다. `.opencode/`는 commit에 포함하지 않았다. A7에서 candidate SHA를 기록한 실제 hosted producer/fixture 경로를 확인해야 한다.

다음 단계: A4의 PredictionState v3 compile/apply/project 및 prediction/model artifact separation. Plan v1/Action contract를 유지하고 shared pure aggregation부터 구현한다.

## A4 실행 기록 — 2026-09-24

```text
단계: A4
시작 HEAD: 9faa14024285e5ad5cc562d1e29e14b0bedd1389
결과 commits: `622e1fa feat: implement PredictionState v3 artifact history`, `c7a9530 docs: record PredictionState v3 artifact decision`, `6ffb8ba fix: bound history metadata parsing by shared deadline`.
시작 상태: tracked tree clean; 사용자 .opencode/ 미추적 1개 보존하고 stage하지 않음.
```

`MeasurementArtifact → ObservationBatch → ModelState → PredictionTable` 경로와 sorted deterministic aggregate/apply/project, duplicate receipt, expiry, PR scope, key/model/prediction/measurement byte caps를 구현했다. `affected`는 history가 assignment를 바꿀 수 있을 때만 같은 workflow/event scope의 v3 prediction artifact를 조회한다. planner는 model이나 measurements를 받지 않고, history API/archive/unzip/JSON/CLI 재계산에 하나의 3초 deadline을 전달한다. 메타데이터와 압축 파일 수신 bytes를 합산해 8 MiB에서 cold 처리하고 ZIP JSON은 각자 한도를 넘기지 못한다. upload/update 오류는 task 결과에 영향을 주지 않는 degraded 경로다.

실제 CLI regression에서 PR `ScopeRef`가 OpenAPI가 요구하는 camelCase (`headRepositoryId`, `headRef`, `baseRef`)를 Rust runtime은 snake_case로만 받아 `corrupt`가 되는 불일치를 발견했다. enum variant 필드 serde를 camelCase로 맞춘 뒤 같은 v3 prediction file이 `loaded`가 되고 `pkg-a` exact row의 250 ms 추정치를 사용했다. 두 번째 item은 cold로 남았으며 source count는 exact 1/cold 1이다. 수정 전 실패와 수정 후 통과를 `affected_loads_v3_prediction_artifact_for_an_exact_task_estimate`에서 확인했다.

검증:

- `cargo test --locked --test plan_cli_tests affected_loads_v3_prediction_artifact_for_an_exact_task_estimate -- --nocapture` — exit 0, 1 passed. 수정 전에는 OpenAPI PR ref의 `baseRef` unknown field로 `historyStatus=corrupt`가 재현됐다.
- `cargo test --locked --all-targets --all-features` — A4 구현과 새 scope regression 포함, exit 0, 216 passed.
- `cargo clippy --locked --all-targets --all-features -- -D warnings` — exit 0.
- `cargo fmt --all --check` — exit 0.
- `bash scripts/action-contract.sh` — exit 0; assignment/history lookup/Plan/checkout/completion contracts 포함.
- `bash scripts/review-change.sh 9faa14024285e5ad5cc562d1e29e14b0bedd1389` — exit 0, 30 changed committed files, heuristics PASS; primary agent manual producer→history→affected review PASS.
- `bash scripts/history-artifact-test.sh` — exit 0; push/PR run 선택, prediction-only planning download, updater-only model download, metadata+archive 합산 byte limit, deadline cancellation, no-change metadata I/O 0회.
- `bash scripts/assignment-action-test.sh` — exit 0.
- `bash -n` changed Action/test scripts, Ruby/Node YAML parse, `git diff --check` — exit 0.
- OpenAPI 공식 validator 실행은 uv 임시 cache에서도 PyPI DNS 차단으로 exit 2. 현재 환경의 Node YAML parser는 파싱했지만 공식 schema/examples/digest validation 통과로 기록하지 않는다.

미실시: 실제 GitHub artifact transport, GHES, producer/consumer hosted PR, released binary, full-workflow cold→warm 성능·정확도, AWS/S3, OpenAPI 공식 validator. A7까지 hosted evidence와 runtime 성능을 완료로 표시하지 않는다. 다음 단계는 A5 preparation telemetry와 automatic assignment count다.

## A5 실행 기록 — 2026-09-24

```text
단계: A5 local implementation
시작 HEAD: 01b6a75 (A4 validation evidence)
결과 commits: `718900b8baff5a4bb9e58329c8a14e27da312cbf` (implementation), `c01cfdbacec175d2eb3d20a5c7604e1b50c7ab40` (ADR decision)
브랜치: codex/prediction-state-v3
시작 상태: tracked A4 tree clean; 사용자 .opencode/ 미추적 상태 보존.
```

`affected --preparation-context`에서 preparation key를 만들고 candidate concurrency의 LPT layout을 비교한다. task 예측이 cold거나 후보 중 preparation 예측이 unknown이면 configured tier cap을 유지한다. known 상태에서는 prep+task makespan, 총 runner time, checkout 경로 수, assignment 수, 안정적인 layout으로 후보를 선택한다. prepare Action 첫 단계가 시각을 기록하고, install Action JSON과 run CLI가 child process 시작 시각을 연결한다. 유효한 경우에만 전체 준비 구간을 v3 `preparationObservations`로 기록한다. 기존 v3 artifact에서 필드가 빠진 경우도 계속 읽는다.

로컬 경로에서 positive telemetry, reversed clock 처리, exact/fallback aggregate, old v3 호환, cold-cap, warm 자동 선택 결정성을 회귀했다. preparation exact/fallback RFC8785 hash vectors를 OpenAPI와 Rust 회귀에 고정했다. `affected`는 실행 중 package-manager 명령에 의존하지 않고 root `packageManager` 선언의 정확 버전과 lockfile을 사용하며, 선언/input mismatch는 cold-cap이 된다. 384개 item·cap 24의 synthetic warm Rust scheduler는 debug test에서 24개 assignment를 선택하고 361,619 µs를 기록했다. 이는 한 번의 로컬 scheduler 계산이며 설치·checkout·실제 task wall time이나 예측 오차를 재지 않았다. 이전 문서 합성 크기 측정은 runtime artifact 크기 검증을 대신하지 않는다. 실제 CI wall time, weighted mean 대 기존 median의 real trace 오차, 실제 v3 artifact 크기는 A7 hosted consumer 증거가 필요해 미실시다.

A5 수정 중 `bash scripts/action-contract.sh`가 empty Bash array를 `set -u`에서 확장하는 오류를 잡아 affected Action에 빈 인자 목록 guard를 추가했다. Continuous install Action 테스트도 run Action이 아닌 실제 install Action 경로를 호출하도록 고쳤다.

검증:

- `cargo fmt --all --check` — exit 0.
- `cargo clippy --locked --all-targets --all-features -- -D warnings` — exit 0.
- `cargo test --locked --all-targets --all-features` — exit 0, 223 passed.
- `cargo test --locked --lib scheduler::tests::large_warm_auto_schedule_reports_local_cost -- --nocapture` — exit 0, 1 passed; 384 items, selected=24, 361,619 µs.
- `bash scripts/plan-action-test.sh` — exit 0; affected Action은 packageManager version을 manifest에서 구성하고 `--version` subprocess 없이 Plan에 전달했다.
- `bash scripts/assignment-action-test.sh` — exit 0; preparation telemetry positive/reversed case와 continuous focused/full install 경로 포함.
- `bash scripts/action-contract.sh` — exit 0; affected, prepare, install, run, history, plan, coordinator, completion Action contracts 포함.
- `cargo fmt --all --check`, `git diff --check`, Python PyYAML structural check, Ruby OpenAPI YAML parse — exit 0.
- preparation exact/fallback JCS IDs — stdlib canonical-ASCII digest 계산과 Rust regression vectors 일치.
- 공식 `uv run --no-project --with openapi-spec-validator --with pyyaml --with rfc8785 ...` — exit 2, PyPI DNS lookup 실패. schema metadata parse/shape 검사는 통과했지만 official OpenAPI validator는 미실시다.
- `bash scripts/review-change.sh 01b6a75` — exit 0, 31 committed files, heuristics PASS. 첫 실행은 `docs/content`/ADR 경로 변경이 없어 block됐고, ADR-0014에 A5 사용자 경로·key·cold-cap·warm objective 결정을 추가해 다시 실행했다.

미실시: actual GHES/GitHub artifact transport, real preparation/task traces, real prediction error vs median, actual artifact bytes, hosted consumer PR/run, released binary. `.opencode/`는 stage하지 않았다. 다음 단계는 A6 예제·requiredJobs·96% completion gate다.
