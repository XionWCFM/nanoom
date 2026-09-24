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
- [ ] A2 Plan v1 CLI·compact matrix·provenance/rerun 검증.
- [ ] A3 prepare·sparse checkout·install/run·GHES contracts.
- [ ] A4 PredictionState v3·compile/apply/project·작은 prediction artifact·bounded lookup.
- [ ] A5 preparation telemetry·automatic k·determinism·대규모 benchmark.
- [ ] A6 examples·requiredJobs·계획 기반 completion gate·96% coverage 기준.
- [ ] Local fmt/lint/tests/schema/Action/실제 Git 및 focused-install 회귀.
- [ ] Producer hosted PR CI: exact candidate SHA 기록.
- [ ] Consumer Yarn+Turbo / pnpm+Nx: cold→warm, positive/non-skipped, selected closure.
- [ ] Consumer no-change / task failure / unexpected skip / rerun aggregate 검증.
- [ ] A7 producer PR + fixture PR + run URL / SHA / 결과 표 첨부.

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

이번 문서 단계에는 runtime 구현·서버 배포·AWS 리소스 생성·merge·release가 없다. 실제 구현 phase의 acceptance가 충족될 때만 해당 항목을 갱신한다. 증거는 명령/환경/시나리오/result/commit/run URL 및 미검증 범위를 함께 기록한다.


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
- [x] A1 runtime을 이 세션에서 실행·검증함. A2~A7과 서버 단계는 위 체크처럼 미완료.

## A1 실행 기록 — 2026-09-24

```text
단계: A1
시작 HEAD: 0aa7e968db4cfb2418c38e11fdebff7440932384
브랜치: codex/prediction-state-v3
시작 상태: 추적 파일 변경 없음, 기존 사용자 파일 .opencode/ 미추적 1개. 보존함.
결과: A1은 local gate 통과 후 이 단계 commit으로 기록함. PR/release/외부 hosted 실행 없음.
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
- `git diff --check` — exit 0.

미실시 및 경계: 지시서의 OpenAPI/spec validator는 기본 uv cache 권한 오류(exit 2) 뒤 `/private/tmp` cache로 재시도했으나 PyPI DNS 차단(exit 2)으로 실행되지 않음. A1의 external producer/consumer hosted fixture 증거는 A7까지 미실시다. A2~A7, server, GHES, release/실사용 consumer 증거는 미완료이며 기존 체크를 변경하지 않음.
