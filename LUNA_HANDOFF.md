# Luna Max 실행 지시서 — Nanoom PredictionState v3

작성일: 2026-09-24. 대상: **Luna, reasoning effort `max`**. 이 문서는 다음 실행 세션의 작업 지시서다. 문서가 존재한다는 사실은 에이전트 실행 또는 runtime 구현 완료를 뜻하지 않는다.

## 1. 시작 상태와 목표

- 저장소: Nanoom. 이 문서를 포함한 Git checkout의 루트를 작업 디렉터리로 사용한다.
- 인계 브랜치: `codex/prediction-state-v3`.
- 구현 조사 기준: `539b2c08cc7e2543f3a0cdd10fbdba451b2502d5`(v0.6.0). 문서 커밋 이후 현재 HEAD와 차이는 시작할 때 다시 확인한다.
- 준비된 것: 전체 실행 계획, 데이터/서버 명세, OpenAPI, 체크리스트, 문서 검증 스크립트.
- 미구현: A1~A7과 S0~S6의 runtime 변경. OpenAPI·합성 payload 검증을 구현 검증으로 승계하지 않는다.
- 인계 시 기존 사용자 파일 `.opencode/`가 untracked다. 읽을 필요 없이 보존하며 작업 커밋에 포함하지 않는다. 이후 발견하는 사용자 변경도 같은 원칙으로 보존한다.

목표는 **이력 조회와 갱신 비용까지 포함한 전체 CI 완료 시간 감소**다. 서버 없는 GitHub artifact 경로를 먼저 완성하고, 같은 집계·예측 로직을 재사용하는 선택적 Rust/S3 서버를 이어서 구현한다. 실제 검증으로 확인한 범위만 완료로 표시한다.

새 worktree에서 시작할 때는 이 브랜치의 문서 커밋을 포함해야 한다. 로컬 `main`의 v0.6.0만으로 시작하지 않는다. 현재 checkout에 이미 이 브랜치가 선택되어 있으면 그대로 사용한다. 다른 작업이 점유한 checkout이나 사용자 변경을 옮기기 위한 reset/stash/clean은 하지 않는다.

## 2. 읽는 순서와 계약의 기준

1. [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md): A1~A7, S0~S6의 순서와 사용자 경로.
2. [SPEC.md](SPEC.md): 현재 구현과 제안 계약의 구분.
3. [예측 모델 명세](docs/prediction-model-spec.md): Key, compile/apply/project, 알고리즘, artifact, 크기/시간/만료 계약.
4. [CHECKLIST.md](CHECKLIST.md): 현재 증거와 미실시 항목.
5. 서버 단계 전에 [서버 명세](docs/history-server-spec.md)와 [OpenAPI](docs/api/history.openapi.yaml)를 읽는다.
6. 변경할 경계의 README/기존 ADR와 `.codex/skills/nanoom-change-review/SKILL.md`를 읽는다. 이 실행 환경에서 제공되는 `user-first-engineering`과 `ponytail`도 적용한다.

이 문서는 실행 순서와 인계 규칙을 설명한다. 세부 데이터/알고리즘은 예측 모델 명세, HTTP wire는 OpenAPI, 운영/분산 동작은 서버 명세가 기준이다. 충돌이 있으면 편한 쪽을 임의로 선택하지 말고, 근거와 영향을 기록해 주 에이전트 또는 사용자에게 확인한다. 영향을 받지 않는 작업은 계속한다.

## 3. 시작 시 실제로 할 일

```sh
git rev-parse --show-toplevel
git branch --show-current
git status --short
git log -3 --oneline
```

현재 branch/HEAD와 기존 변경 목록을 CHECKLIST의 실행 기록에 남긴다. `.opencode/`를 이유로 초기화하지 않는다. 계획을 다시 작성하는 데서 끝내지 말고 **A1의 실패 재현과 수정부터 시작**한다.

문서 검증은 다음 명령으로 재현할 수 있다. 최초 한 번 실행하고 이후에는 관련 계약을 바꿨을 때 다시 실행한다. 설치가 막히면 네트워크/도구 오류를 기록하며 독립적인 Rust 작업은 계속한다.

```sh
uv run --no-project --with openapi-spec-validator --with pyyaml --with rfc8785 python docs/validation/check_prediction_spec.py
```

한 번에 한 단계의 diff를 만든다. 각 단계에서 재현 → 최소 수정 → 관련 검증 → 자기 검토 → CHECKLIST 기록을 끝낸 뒤 다음 단계로 진행한다. 컨텍스트가 부족하면 현재 위치를 기록하고 이어갈 수 있도록 남긴다. 기능과 무관한 전면 리팩터링이나 새로운 framework를 함께 넣지 않는다.

## 4. 첫 구현 작업: A1

현재 코드의 출발점은 아래와 같다. 파일 이름만 보고 수정하지 말고 `rg`로 호출자와 serializer/consumer를 확인한다.

| 경계 | 우선 확인할 파일 |
|---|---|
| 시간 key·fallback·assignment | `src/scheduler.rs`, `src/affected.rs`, `src/commands/affected.rs` |
| 실제 task 실행·실행 없음 처리 | `src/commands/run.rs`, `.github/actions/run/run.sh` |
| assignment focused install | `src/commands/install.rs`, `.github/actions/install/run.sh` |
| sample 생성·병합 | `.github/actions/run/run.sh`, `.github/actions/history/run.sh`, `src/commands/history.rs` |
| 회귀 | 기존 모듈 tests, `tests/cli_integration_tests.rs`, `tests/affected_tests.rs`, `scripts/assignment-action-test.sh`, `scripts/history-artifact-test.sh`, `scripts/action-contract.sh` |

A1에서 아래 네 가지를 실제로 재현하고 고친다.

| 입력/상황 | 합격 조건 |
|---|---|
| 예상 시간이 100/1/1/1인 4개 item, concurrency 4 | 빈 assignment 0개, 각 item 정확히 1회 배정, 결정적인 결과. 이 입력에서 반드시 4개 runner를 써야 한다는 뜻은 아니다. |
| `shard=1,totalShards=2` 관측 뒤 `shard=1,totalShards=4` 요청 | 서로 exact/fallback으로 섞이지 않는다. producer→wire→merge→predictor 전 경계에 totalShards를 반영한다. |
| 같은 group의 build/test, 다른 runner/environment | workspace만 제외한 동일 task/layout/runner/environment 사이에서만 fallback한다. 적합한 관측이 없으면 cold다. |
| 계획된 item이 실제로는 no-match / executions 0, 또는 빈 assignment install | 성공으로 통과하거나 전체 설치로 확대하지 않는다. 명시적으로 실패하며 후속 pending item을 실행하지 않는다. |

주의: standalone `nanoom install`의 필터 없음은 기존 전체 설치 용도다. **빈 assignment 경계의 오류를 고치려고 일반 CLI 전체 설치 기능을 제거하지 않는다.** standalone no-change와 실제 계획된 item 미실행도 구분한다. 기존 continuous scheduler의 계약은 호출자를 검토해 유지한다.

A1은 identity·fallback·실행 정확성 수정이다. A4의 PredictionState v3와 서버를 한 번에 구현하지 않는다. 기존 raw-sample 구조에서 필요한 최소 수정과 회귀를 먼저 만들고, raw history용 새 장기 호환 계층은 추가하지 않는다. v3 전환 때도 사용자 시나리오 회귀는 유지한다.

## 5. 후속 단계와 넘겨야 할 증거

| 순서 | 구현 초점 | 다음 단계에 넘길 결과 |
|---|---|---|
| A2 | Plan v1 파일 CLI, compact matrix, digest/provenance | 큰 계획·변조·head 불일치·같은 run 재실행·0작업 회귀 |
| A3 | prepare→checkout→install→run, GHES wrappers | 정확한 head, 실제 sparse 범위, root 도구와 dependency closure, unrelated 제외, 실행 item 일치 |
| A4 | 공용 compile/apply/project와 분리된 model/prediction artifact | bounded read, duplicate/expiry/PR 격리, model/sample planning 다운로드 0회 |
| A5 | 준비 시간 관측과 자동 assignment 개수 | cold/unknown 분리, 결정성, 실제 시간·예측 오차·크기 측정 |
| A6 | 예제/마이그레이션, requiredJobs, completion gate | positive인데 skip/실행0이면 실패, no-change는 성공, coverage 96% 기준 정합 |
| A7 | producer와 실제 consumer fixture의 candidate 검증 | 같은 SHA의 Action+binary, cold→warm, non-skipped jobs, 실패 전파, aggregate, PR/run URL |
| S0~S6 | 별도 Rust 서버, S3 CAS, 인증/운영, opt-in client | 서버 명세의 단계별 증거, 2-process 동시성, 실제 S3/hosted 경로 |

A1~A6의 로컬 작업은 각 단계 검증이 통과하면 이어서 진행한다. A7 artifact 사용자 경로가 확인되기 전에 서버 단계의 구현을 앞당기지 않는다. 서버는 기존 공용 모델 코드를 재사용하며 CLI 기본 사용에 서버 dependency나 credential을 요구하지 않는다.

fixture 작업은 그 저장소의 현재 상태와 remote main을 확인한 별도 worktree에서 수행한다. 기존 fixture checkout의 미커밋 변경을 가져가거나 지우지 않는다. `@latest`의 이전 release로 candidate SHA를 검증하지 않는다.

## 6. 반드시 지킬 결정

- 기본 history backend는 artifact. 기존 `scheduler:http` live coordinator와 선택적 history server를 섞지 않는다.
- planner는 계산된 PredictionTable만 읽는다. ModelState와 원본 sample을 내려받지 않는다. 배분 선택지가 없으면 history metadata 요청도 하지 않는다.
- 모든 scope의 metadata/retry/download/parse에 **공유 3초** budget을 적용한다. 크기 제한과 취소를 실제 요청·압축 해제·해석에 연결한다. 3초는 정상 목표 latency가 아니다.
- task/fallback/preparation key 합계 50,000, model 16 MiB, 날짜 bucket 7개, receipt 4096개 등 한도는 원본 명세를 따른다. raw 실행 배열의 장기 누적이나 유효 receipt의 조용한 삭제를 금지한다.
- 과거 median과 새 가중 평균의 정확도 차이를 실측한다. 합성 파일이 작다는 사실로 속도·정확도 향상을 주장하지 않는다.
- history 실패는 warning/degraded/cold. Plan/hash/head/실제 task/affected base SHA 실패는 fatal이다.
- `concurrency`는 Nanoom assignment 상한이다. GitHub `strategy.max-parallel`과 다르다.
- 자체 DAG/queue/Redis/DB/partition framework를 추가하지 않는다. 서버는 Rust 별도 binary, scope당 S3 객체와 CAS, `/health`·`/ready`를 따른다.
- 현재 계획에 없는 권한 변경·유료 리소스 생성·운영 배포·merge·release는 실행하지 않는다. 기존에 제공된 테스트 환경에서 할 수 있는 검증과 필요한 추가 환경을 구분한다.

## 7. 검증 명령과 증거의 범위

변경한 영역의 기존 회귀와 새 회귀를 먼저 실행한다. 단계의 로컬 완료 시 아래 공통 gate를 실행한다. 관련 변경이 없는 검사를 매 작은 편집마다 반복하지 않는다.

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
bash scripts/action-contract.sh
git diff --check
```

Action/이력/상태/완료 gate를 변경하면 대응하는 `scripts/*-test.sh`를 실행한다. schema/config를 바꾸면 기존 생성·일치 검사를 확인하고 schema 산출물도 갱신한다. coverage gate는 다음과 같다.

```sh
cargo llvm-cov --locked --workspace --all-features --fail-under-lines 96 --summary-only
```

현재 `scripts/verify-completion.sh`는 coverage 90%를 사용한다. 이 명령 하나의 통과를 최종 기준으로 삼지 말고 A6에서 producer 96%와 맞춘다. 도구가 없거나 외부 환경이 없어서 실행하지 못한 검사는 미실시로 기록한다.

`bash scripts/review-change.sh <검증한-base-ref>`는 committed diff만 본다. 인계 문서 커밋을 구현 증거로 사용하지 않는다. 미커밋 diff와 untracked 파일을 별도로 검토하고, 실제 구현 commit이 생긴 뒤 올바른 base로 다시 확인한다.

local unit/Action contract, 생성 fixture, 실제 GitHub consumer, GHES, S3-compatible, AWS S3, released consumer는 서로 다른 증거다. 필요한 외부 proof가 빠졌으면 로컬 단계만 완료하고 전체 완료는 미검증으로 남긴다. 공급된 환경이 없을 때 mock 결과를 실제 서비스 결과로 대체하지 않는다.

## 8. 진행 기록과 인계 형식

각 단계가 끝날 때 CHECKLIST에 다음을 남긴다. 통과하지 않은 항목의 체크박스를 채우지 않는다.

```text
단계: A1
시작 HEAD / 결과 commit 또는 working diff:
문제와 변경 경로:
사용자 입력 → 실제 결과:
검증 명령 / 환경 / exit status:
새 회귀가 수정 전 실패하고 수정 후 통과한 근거:
미실시 검증 / 남은 위험:
다음 단계 / 첫 작업:
```

로컬 commit은 검증된 단계 단위로 작성하고 관련 파일만 명시적으로 stage한다. `.opencode/`, credentials, build output과 사용자 변경은 포함하지 않는다. PR 단계에서는 branch/commit/run을 연결하고 주 에이전트의 검토를 받는다. 단독 세션으로 실행하는 경우에는 같은 기준으로 자기 검토한 뒤 검토 가능한 diff와 근거를 사용자에게 인계한다. 주 에이전트가 없는 세션에서 가상의 승인이나 검증 결과를 만들지 않는다.

다음 상황에서는 해당 부분을 멈추고 구체적 사유를 보고한다: 계약 간 충돌, 사용자 변경과의 충돌, 필요한 외부 환경/권한 부재, 실제 성능 회귀. 구현 난이도가 높다는 이유만으로 목표를 축소하거나 체크리스트를 바꿔 통과시키지 않는다. 독립적으로 진행 가능한 작업은 계속한다.

최종 보고에는 완료 단계·변경 파일·검증 결과·PR/실행 링크·미검증 범위·다음 작업을 포함한다. 파일을 만들었다는 이유만으로 runtime 또는 전체 CI 개선을 완료 처리하지 않는다.

## 9. 실행 세션에 전달할 시작 지시문

```text
Luna 모델, reasoning max로 Nanoom 작업을 시작해줘.
브랜치는 codex/prediction-state-v3이고 이 브랜치의 문서 커밋을 포함한 checkout을 사용해.
루트 LUNA_HANDOFF.md를 먼저 읽고 연결된 명세와 CHECKLIST를 확인해.
계획만 다시 제안하지 말고 A1의 실제 실패 재현과 수정부터 수행해.
각 단계 검증과 실행 기록을 남기며 A1~A6을 순서대로 진행하고,
A7의 실제 artifact 사용자 경로를 확인한 후 S0~S6 서버 단계로 진행해.
기존 사용자 변경은 보존하고, 외부 환경이 없으면 해당 검증을 미실시로 정확히 기록해.
merge/release/운영 배포는 하지 말고 검토 가능한 변경과 증거를 인계해.
```
