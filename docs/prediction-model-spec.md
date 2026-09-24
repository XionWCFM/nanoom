# PredictionState v3 — 실행 이력 대신 예측 상태 저장

상태: A4 local Rust/Action 구현 및 회귀가 통과한 branch 후보 계약. Official OpenAPI validator는 패키지 DNS 문제로 미실시다. Hosted GitHub, GHES, released consumer, 선택적 서버 증거는 별도다. 이전 원본 sample 7개/128 MiB snapshot 설계는 사용하지 않는다. [전체 계획](../IMPLEMENTATION_PLAN.md), [서버](history-server-spec.md), [OpenAPI](api/history.openapi.yaml)와 함께 적용한다.

## 1. 목적과 데이터 분리

Nanoom이 보존할 것은 다음 CI의 배분에 필요한 예측값과 이를 갱신할 작은 계산 상태다. 원본 실행 로그를 장기간 조회하는 분석 시스템은 만들지 않는다. 29.3 GiB 예시는 많은 run의 snapshot 사본을 합한 저장량이며 한 번에 내려받는 파일 크기가 아니다. **총 보관량·회당 다운로드량·병합 상태 크기를 따로 제한**한다.

| 데이터 | 소비자 | 포함 | 기본 보관 |
|---|---|---|---|
| PredictionTable v3 | affected/planner | key ID, 예상 ms, 관측 수, 마지막 관측, 유효 기한 | artifact 30일, 자체 만료 검증 |
| ModelState v3 | history updater / 선택적 Rust server | key별 날짜 집계, 계산된 예측값, 작은 batch receipt | artifact 30일 / S3 현재 scope 객체 |
| 현재 attempt 측정 | history job | 실행 ID, 실제 duration, 시각, provenance | 임시 sample artifact 1일, 학습 후 장기 이력에 복사하지 않음 |
| Plan v1 | prepare/install/run | 실제 실행할 작업과 checkout 계획 | 기존 30일, 변경 없음 |

Artifact file envelopes are `MeasurementArtifact {version, scope, runId, runAttempt, observations}`, `ModelStateBundle {version, states}`, and `PredictionArtifact {version, predictions}`. A prediction entry links one scope's compact table to the canonical ModelStateBundle name and digest. The publish marker is uploaded last; planning downloads that marker only.

동일 key는 하루에 1번이든 10만 번이든 **날짜별 count와 totalDurationMs만 증가**한다. raw duration 배열, command/log, 각 실행의 SHA·workflow metadata를 model에 반복 보관하지 않는다. scope에 공통인 정보는 envelope에 한 번만 저장한다.

## 2. 예측 key와 충분 통계

key ID는 아래 Key의 RFC8785 JCS bytes SHA-256 hex다. scope(ref/PR 포함)는 저장/권한 경계이며 key ID에 ref/run/head SHA를 중복 포함하지 않는다. 같은 환경에서 PR→base branch fallback 조회가 가능하다.

- TaskExact: kind, group, workspace, task, shard, totalShards, taskRunner, timingEnvironment.
- TaskFallback: TaskExact에서 workspace만 제외. task·shard layout·runner·environment를 섞지 않는다.
- PreparationExact: kind, group, taskRunner, timingEnvironment, packageManager+version, installMode, lockfileDigest, checkoutDigest, workspaceSetDigest.
- PreparationFallback: PreparationExact에서 checkoutDigest/workspaceSetDigest만 제외.

Key JSON의 필드는 생략하지 않는다. kind는 `taskExact`, `taskFallback`, `preparationExact`, `preparationFallback` 중 하나다. Task의 shard/totalShards는 둘 다 null이거나 1 ≤ shard ≤ totalShards인 정수 쌍이다. 나머지 이름/환경은 config에서 해석한 nonempty string이다. preparation 필드는 `packageManager`, `packageManagerVersion`, `installMode`, `lockfileDigest`, `checkoutDigest`, `workspaceSetDigest`로 고정한다. lockfileDigest는 실제 lockfile bytes SHA-256이며 파일이 없으면 준비 예측은 unknown이다. checkoutDigest/workspaceSetDigest는 중복 제거 후 UTF-8 byte 순으로 정렬한 POSIX 상대 경로/워크스페이스 이름 배열의 JCS SHA-256이다. taskRunner는 실제 실행 도구를 사용한다. exact/fallback Key는 지정된 필드만 포함하며 추가 필드를 허용하지 않는다. taskExact JSON과 keyId 기준값은 OpenAPI `x-contract-examples`를 따른다.

모든 JSON 숫자는 0..2^53−1 범위의 정수다. count/sum overflow는 저장 전에 거부한다. 배열은 prediction rows/model entries를 keyId 순, buckets를 날짜 오름차순, receipts를 batchId 순, aggregate rows를 (keyId,day) 순으로 정렬한다. JCS만으로 배열 순서가 정규화되지 않으므로 이 규칙을 함께 적용한다. 같은 model/기준 날짜의 projection bytes와 ETag는 항상 같아야 한다.

artifact compiler가 현재 attempt의 성공 실행 ID를 먼저 dedup한 뒤 위 key별로 집계한다. task 관측 하나는 exact와 대응 fallback에 각각 반영한다. 준비 시간도 같은 규칙이다. 한 key의 관측이 서로 다른 task 종류를 대표하지 않게 한다. 서버는 권한을 받은 compiler의 집계를 검증해 적용하며 GitHub 실행을 독립적으로 재증명하지 않는다.

각 key의 날짜 bucket은 `[utcEpochDay, observationCount, totalDurationMs, lastObservedAtMs]`다. 오늘 UTC 날짜와 직전 29일 범위에서 **가장 최근 관측이 있었던 7개 날짜**만 남긴다. 30일 경계의 부분 날짜 전체를 제외할 수 있으므로 보관은 보수적으로 최대 30일이다. 버킷 개수는 실행 횟수와 무관하게 최대 7이다.

예측값은 최근 날짜에 가중치를 주는 평균이다. 가장 최근 bucket 날짜를 D, 각 날짜를 d라고 할 때:

```text
weight(d) = 2 ^ (-(D - d) / 7)
estimatedMs = round_half_up(
  sum(totalDurationMs[d] * weight(d)) /
  sum(observationCount[d] * weight(d))
)
```

bucket을 날짜 순서로 정렬한 뒤 공용 Rust 함수에서 계산한다. 정수 count/sum을 먼저 병합하므로 동시에 도착한 batch의 순서와 무관하게 같은 bucket이 된다. 부동소수점은 최종 최대 7항의 계산에만 사용한다. key의 관측 수는 보존 bucket count 합, lastObservedAt은 최댓값이다. `validUntilMs = (가장 오래된 포함 bucket의 epochDay + 30) * 86400000`; 그 시각 이후에는 그 추정값을 그대로 사용하지 않는다.

**정확한 최근 7개 실행의 중앙값에서 추정 방식이 바뀐다.** 평균은 이상치에 더 민감할 수 있다. compact하다는 이유만으로 예측 품질 향상을 주장하지 않는다. outlier, 작업량 급변, 드문 실행, 동일 시간 다수 관측 trace에서 기존 방식과 다음 실행 예측 오차·실제 배분 makespan을 비교하고 차이를 보고한다. 전역 task DAG나 머신러닝 학습 pipeline은 추가하지 않는다.

## 3. 재시도와 분산 집계

합계를 다시 더하면 중복 학습되므로 sample을 버리기 전에 **불변 batch** 경계를 둔다.

- scope/runId/runAttempt마다 history compiler가 batch 하나를 확정한다. batchId = SHA256(JCS([scopeId,runId,runAttempt])). 모든 aggregate row는 현재 실제 실행 attempt의 관측만 포함한다. 같은 (keyId, UTC day)는 compiler가 미리 하나로 합치고, 서버 입력에 중복 row가 있으면 거부한다.
- 같은 attempt의 telemetry가 일부 누락되면 그 사실을 기록하고 사용 가능한 성공 관측으로 한 번 확정한다. 나중에 다른 내용으로 같은 batch를 재발행하지 않는다. 다음 실제 실행 attempt는 별도 batch다. task 정확성은 telemetry 완전성과 분리한다.
- HTTP Idempotency-Key는 원래 전송 body bytes digest다. retry는 같은 bytes를 사용한다. 같은 batchId/digest면 200 unchanged, 같은 batchId/다른 digest면 409다.
- model에는 `[batchId,bodyDigest,producedAtMs]` receipt만 보관한다. 원본 batch나 sample은 저장하지 않는다. receipt는 producedAt 이후 8일, 입력 batch는 최초 producedAt 이후 7일 미만만 허용한다. 7일 지난 동일 body는 다시 학습되지 않는다. 새 producedAt으로 오래된 batch를 재생성하는 것은 client 계약 위반이다.
- scope당 receipt 최대 4096개. 살아 있는 receipt를 몰래 제거하지 않는다. 한도를 넘으면 기존 상태를 보존한 409이며 telemetry degraded다. 4096개/8일은 이 v1의 명시적 수용 한도다.
- S3 CAS의 단일 객체에 모델과 receipt를 **같이 저장**한다. stats 반영 후 receipt 저장 전에 crash하는 두 단계 commit을 만들지 않는다. 충돌 시 최신 state로 다시 dedup/merge한다.
- 동일 날짜 count/sum은 정수 덧셈, lastObservedAt은 max, 오래된 bucket 제거는 공통 clock cutoff로 결정한다. key별 최신 7개 날짜 이후의 오래된 bucket은 재전송으로 부활하지 않는다. overflow는 전체 batch 오류다.

중복 방지용 짧은 receipt가 있을 뿐 실행 기록 보존 API나 영구 exactly-once 감사 ledger는 없다. clock 기준은 UTC, 허용 미래 편차는 5분. ModelState에 pruning day/batch acceptance watermark를 저장하고 CAS에서 기존 값보다 뒤로 돌리지 않는다.

## 4. Artifact 경로: 학습 파일을 affected에 전달하지 않는다

history job은 같은 state에서 두 파일을 만든다.

1. model artifact를 업로드한다.
2. model artifact name/digest와 작은 PredictionTable을 담은 prediction artifact를 마지막에 업로드한다. prediction artifact가 publish marker다.

affected는 이전 성공 run의 **prediction artifact 하나만** 읽는다. model/raw sample artifact는 읽지 않는다. history job만 해당 prediction이 가리키는 model을 읽고 갱신한다. model이 유실·손상되면 현재 관측으로 새 학습 상태를 시작하며 이전 평균을 가짜 관측으로 주입하지 않는다. 유효한 prediction은 자체 validUntil 범위에서 사용할 수 있지만 재구축된 상태의 데이터 부족은 명시한다.

PredictionTable rows는 `[keyId, estimatedMs, observationCount, lastObservedAtMs, validUntilMs]`로 고정한다. column names는 schema에 고정하고 배열 형식을 문서화한다. 긴 key metadata나 provenance는 rows에 반복하지 않는다. planner는 자신이 만든 Key로 ID를 계산하므로 hash를 문자열 workspace 이름으로 역변환할 필요가 없다.

읽기 자체가 필요 없는 경우를 먼저 제거한다. affected work item이 0개이거나, 모든 관련 group에서 item이 1개/assignment 상한이 1개/배분 설정이 없어 기존 item별 assignment를 그대로 사용하는 경우에는 배분을 바꿀 선택지가 없다. 이 group은 prediction 조회를 생략하고 `history_not_needed`를 기록한다. 모든 group이 해당하면 metadata 요청을 포함한 history read I/O가 0회여야 한다. 일부 group만 해당하면 그 scope를 조회 대상에서 제외한다. 예측 비용이 unknown인 것과 이력이 불필요한 것은 구분한다.

기본 read budget:

- prediction artifact 압축 전 JSON 최대 8 MiB, archive 최대 4 MiB.
- affected의 이력 조회·다운로드·파싱 전체 budget 3초, scope별 후보 body 다운로드 최대 2개, 모든 scope의 metadata+archive 수신 합계 최대 8 MiB. metadata·재시도도 같은 deadline에 포함한다. API 요청, archive unzip, bounded JSON/JQ/CLI parse는 남은 deadline으로 취소한다. scope마다 3초를 새로 부여하지 않는다.
- artifact metadata의 size를 먼저 확인하고, 스트리밍 수신 크기와 unzip 후 JSON 크기를 다시 제한한다. Content-Length만 신뢰하지 않는다. budget 초과는 즉시 warning/cold다.
- model JSON 최대 16 MiB, aggregate batch JSON 최대 16 MiB. model은 planning critical path에서 받지 않는다.
- task/fallback/preparation 전체 key 합계 최대 50,000개. 7개 날짜 bucket과 4096 receipt도 최종 byte budget에 포함한다. 만료/prune 후 검사하고, 초과하면 저장하지 않는다. byte budget이 key 개수보다 먼저 찰 수 있다.

8 MiB/4 MiB는 목표 크기가 아니라 최후 상한이다. 생성 fixture에서 key 수별 실제 bytes와 압축률을 측정한다. 같은 key에 관측이 늘어도 파일 크기가 선형 증가하면 실패다. 정상 개발 PR이 한도 초과로 반복 cold 처리되면 성능 완료로 인정하지 않는다.

GHES의 같은 run sample 다운로드는 기존 v3 transport 제약을 따른다. planner의 과거 prediction 조회는 완료된 run에서 exact artifact만 읽으므로 학습 파일을 함께 받지 않는다. 병렬 artifact producer의 상태는 선택한 부모 model 기준의 best-effort 학습이며 모든 분기 관측의 자동 합류를 보장하지 않는다. 신규 SDK/DB/분산 partition manifest는 추가하지 않는다.

## 5. 서버와 유효 기간

서버는 ModelState 한 객체를 S3에 저장한다. 공개 GET /snapshot은 그 객체 자체가 아니라 **PredictionTable projection**만 반환한다. GET 시 날짜 만료를 적용해 projection을 계산하되 S3를 쓰지 않는다. HTTP ETag는 prediction response bytes의 SHA-256, S3 ETag는 모델 CAS token으로 분리한다. 비교 cutoff가 바뀌어 projection이 달라지면 HTTP ETag도 바뀐다.

artifact는 생성 시의 prediction을 받으므로 validUntil이 지난 row는 버린다. 서버는 남은 유효 bucket으로 즉시 projection을 계산할 수 있다. 이 차이는 저장소 접근 시점의 차이이며 두 경로가 같은 관측 상태·같은 기준 날짜를 사용하면 같은 예측을 만든다. 유효 row가 없으면 404/cold다. GET으로 model lifecycle을 연장하지 않는다.

S3 versioning off 기본, noncurrent opt-in 1일, 비활성 current 객체 45일은 유지한다. raw measurement artifact는 1일 후 만료되므로 오래 지난 history job 재실행에서 수집 자료가 없을 수 있다. 이때 degraded로 끝내며 task rerun용 Plan artifact 30일은 유지한다. 분석용 원본 영구 보관은 이번 범위에 없다.

## 6. 구현과 인수 조건

- 기존 History v2 원본 sample wire 제안을 폐기하고 PredictionTable/ModelState v3으로 바꾼다. 아직 미배포 설계이므로 HTTP prefix /v1은 유지한다. 이전 v0.6/versionless/V2 파일은 추측 변환하지 않고 cold/bootstrap한다.
- artifact/server 양쪽이 같은 compile-batch, apply-batch, project-predictions Rust 함수를 사용한다. scheduler는 PredictionTable만 읽고 raw sample을 탐색하지 않는다.
- Key golden vectors, bucket/count/sum/validUntil, daily cutoff, outlier/step change 예측을 검증한다. 반복·재정렬·동시 CAS·응답 유실이 동일 관측을 두 번 학습하지 않는지 검증한다.
- 1,000/10,000/30,000 task key와 fallback/preparation key를 포함한 상태에서 raw V2 대비 model/projection raw·archive bytes를 별도로 기록한다. 동일 key 100,000회 관측은 aggregate로 묶어 상한 내 불변 row/bucket 수를 검증한다. receipt cap을 넘기는 입력은 성공 테스트로 가장하지 않고 명시적인 거부를 확인한다.
- planning 경로에 model/sample download 0회, scope별 후보 수·전체 bytes·공유 3초 예산을 fake HTTP와 실제 hosted fixture에서 각각 확인한다. 느린 stream/허위 size/압축 폭탄은 cold로 중단하고 task 정확성을 유지한다.
- 원본 명령/로그/SHA가 없는 compact projection이라는 것을 schema로 보장한다. 통계 정확성과 실제 네트워크 지연을 구분한다. 문서/합성 payload 측정만으로 hosted 속도 개선을 주장하지 않는다.

CI 성능 인수는 history off와 비교해 historyFetchMs와 history 갱신 후처리를 포함한 실제 workflow 시작→종료 wall time을 측정한다. 병렬 구간의 시간을 단순 합산하지 않는다. scope 하나마다 예산을 새로 부여하거나 취소된 요청이 background에서 계속 다운로드하는 구현은 거부한다. timer/abort를 연결하고 JSON 해석도 bounded worker 또는 동등한 중단 가능한 경로로 제한한다. 작은 PR/짧은 task와 지연된 이력 응답을 포함한다. 예측 배분이 절약한 시간보다 조회 비용이 크면 그 결과를 성능 개선으로 표시하지 않는다. 3초는 대기해도 되는 목표 시간이 아니라 예외 상황의 최대 예산이다.


## 7. 문서 단계 합성 크기 측정 — 2026-09-24

[재실행 스크립트](validation/check_prediction_spec.py)는 OpenAPI/examples/hash vector 검증과 아래 payload 생성을 함께 수행한다. stdlib ZIP DEFLATE level 6, 파일 하나의 archive이며 실제 GitHub artifact upload/download 측정은 아니다. 각 시나리오는 task key에 fallback 3개 + preparation 100개, key당 관측 날짜 7개, 최댓값인 receipt 4096개를 포함한다. 비교용 raw는 관측별 identity/provenance를 반복하는 **합성 데이터**이며 배포된 v0.6 직렬화 benchmark가 아니다. 단위는 MiB(2^20 bytes)다.

| task keys / 전체 keys | 합성 raw 7회분 JSON / ZIP | updater ModelState JSON / ZIP | planner prediction JSON / ZIP |
|---|---:|---:|---:|
| 1,000 / 1,103 | 3.31 / 0.41 | 0.96 / 0.39 | 0.11 / 0.05 |
| 10,000 / 10,103 | 30.33 / 3.78 | 4.01 / 1.07 | 1.02 / 0.41 |
| 30,000 / 30,103 | 90.36 / 11.28 | 10.79 / 2.57 | 3.04 / 1.23 |

마지막 행의 정확한 planner bytes는 JSON 3,191,014 / ZIP 1,284,724다. updater model JSON은 11,315,444 bytes로 16 MiB cap 안에 있었다. 이 결과는 해당 synthetic shape의 크기만 보여준다. key 수 50,000까지 항상 16 MiB에 맞는다는 보장은 아니다.

같은 key의 관측 수를 7회에서 700,000회로 늘린 합계 상태에서는 bucket 7개를 유지했다. entry bytes는 정수 자릿수 증가로 352→427이며 실행 수에 비례하는 raw 배열은 없다. 실제 compile/apply의 dedup·동시성 검증을 이 측정으로 대신하지 않는다.

회당 다운로드가 작아져도 GitHub의 run별 artifact 사본 총량이 0이 되지는 않는다. 마지막 시나리오의 model+prediction ZIP을 하루 100회, 30일 보관한다고 계산하면 약 11.1 GiB다(측정 artifact/Plan 제외). planner는 그 합계를 다운로드하지 않는다. 운영 시 총 저장량은 별도로 확인하고 보관기간을 낮출 때의 학습 재구축 빈도를 검증한다. 추가 GC 서비스는 도입하지 않는다.
