# 다음 실행시간 예측 품질

상태: v0.9.0 공개 및 첫 released fixture 검증 완료, 후속 실행의 정확도 검증 진행 중. 준비 시간 해시 수정, 최근 batch 요약 기반 예측기, 실제 runner 환경 자동 수집과 profile별 compiler·학습·projection·pool 배분을 연결했다. 로컬 D1의 cold → merge → duplicate replay → warm affected/run 및 실제 fingerprint 보존을 검증했고, 공개 artifact 경로에서 수집·학습·게시와 다음 Plan 반영을 확인했다. 전체 수용 기준은 아직 완료되지 않았다.

## 목표와 수용 기준

사용자가 원하는 결과는 평균 산술의 정확성을 넘어 다음 실행의 소요시간과 assignment makespan을 잘 예측하는 것이다. 동일 task 이름이라도 workspace별 작업량이 다르며, 같은 runner label 안에서도 실제 하드웨어가 다를 수 있다.

| 입력 또는 위험 | 기대 결과 | 검증 |
| --- | --- | --- |
| 동일 build, workspace 비용 5배 차이 (5초/1초) | 각각의 exact history를 사용해 두 assignment를 6초씩 배분 | compile → apply → projection → scheduler 통합 테스트 |
| CPU/코어 수/메모리/OS/이미지가 다른 runner | 실제 환경 fingerprint로 이력을 구분 | 자동 수집 → 정규화 → 해시 → 측정 artifact → 다음 Plan |
| 동일 라벨에 다른 하드웨어 | 라벨을 실제 하드웨어로 간주하지 않고, 계획 시 미확정 환경을 명시 | 이질적인 pool fixture와 환경 변경 회귀 |
| 새 workspace 또는 새 환경 | fallback/cold와 그 불확실성을 드러냄 | exact/group/cold source 및 다음 실행 오차 |
| 안정·잡음·이상치·급변·동일 날짜 다수 실행·드문 실행 | 각 실행을 학습하기 전에 예측하고 후보 모델과 비교 | 재현 가능한 prequential trace |
| 준비 시간 기록·조회 | 동일 JSON 배열이 동일 exact key로 연결 | Action의 실제 측정 파일을 독립 encoder로 검증 |
| 실제 CI | 새 릴리즈의 학습 결과가 후속 Plan에 사용되고 총 CI 시간이 검증됨 | 두 저장소 CI, released fixture, aggregate status |

정확도 비교에서는 다음 실행 자체를 먼저 학습해 답을 미리 알려주지 않는다. 작업 예측의 WAPE는 `sum(abs(predicted-actual))/sum(actual)`이며, assignment 단위와 개별 task 단위를 구분한다. workflow 시간은 checkout/setup/install, 이력 조회·갱신을 포함한 시작→종료 시간으로 비교한다.

## 로컬 trace 기준값

재실행: `cargo test --locked --test prediction_quality_tests -- --nocapture`.

아래는 정해진 합성 workload이며 실제 CI 개선 증거가 아니다. baseline은 v0.8.0의 날짜 가중 평균을 같은 보존 bucket으로 재생한 값이다. 로컬 제품 후보는 최신 3개 batch 평균 중 최신 관측과 7일 이내인 값의 중앙값을 쓴다. median3/latest/EWMA0.5는 테스트 전용 비교 구현이다. 현재 통합 회귀는 안정 trace 오차 0, 잡음의 baseline 대비 증가 0.5%p 이내, 나머지 trace의 baseline 대비 오차 감소를 요구한다.

| trace | v0.8.0 날짜 가중 평균 | 로컬 제품 후보 | 최근 3회 중앙값 | 최근값 | EWMA α=0.5 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 안정 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% |
| 잡음 | 7.36% | 7.69% | 7.69% | 11.54% | 8.22% |
| 단발 이상치 | 121.52% | 70.37% | 70.37% | 140.74% | 136.34% |
| 비용 급증 | 51.92% | 32.56% | 32.56% | 16.28% | 31.54% |
| 비용 감소 (5초 → 1초) | 63.79% | 40.00% | 40.00% | 20.00% | 38.75% |
| 동일 날짜 비용 급증 | 57.60% | 32.56% | 32.56% | 16.28% | 31.54% |
| 12일 간격 실행 | 34.54% | 28.00% | 56.00% | 28.00% | 49.00% |

이상치가 발생하는 실행 자체는 과거만으로 맞힐 수 없다. 여기서 비교하는 것은 그 실행의 오차와 이후 정상 실행에 오염이 얼마나 남는지도 포함한다. 단일 합계 점수가 나아졌다고 모든 시나리오가 좋아졌다고 주장하지 않는다. 최근 3회 중앙값은 이상치에 유리하지만 드문 실행에서 더 나쁘므로 그대로 기본값으로 교체할 근거가 부족하다. 동일 날짜의 count/sum만 보관하면 날짜 안에서의 순서를 복원할 수 없어 최근 실행의 변화에 대응하는 데 한계가 있다.

## 환경 수집과 크기 검증

`run` Action이 Node 표준 라이브러리로 OS/arch, CPU 모델·가용 코어, 전체 메모리, cgroup-v2 CPU quota/메모리 한도, 이미지, Node와 실제 설치 패키지 매니저 버전을 수집한다. 정렬한 profile JSON의 SHA-256과 profile을 `runnerEnvironment`에 저장하고 Rust가 hash 및 값의 범위를 검증한다. runner 이름·job/run ID·현재 부하는 해시에 넣지 않는다. cgroup-v1 및 확인하지 못한 제한은 `containerLimits=unknown`으로 명시한다. metadata 수집 실패는 task 실행을 실패시키지 않는다. history compiler는 같은 logical Scope 안에서 profile별 child batch/model/table을 만든다. workspace별 예측은 각 profile에서 독립적으로 학습하고, runner 미정 pool에서는 관측 비중 평균과 여러 profile의 task 비용 범위를 제공한다. native collector → history CLI → 저장된 model/prediction → scheduler 통합 검증을 추가했다.

로컬 native collector → Rust 검증과 Action → 실제 measurement JSON 경계를 검증했다. 새로운 최근 요약을 포함한 30,103-key 합성 ModelStateBundle은 JSON 15,165,699 bytes / ZIP 3,117,405 bytes이고, compact prediction은 JSON 3,190,496 bytes / ZIP 1,284,677 bytes다. 이 크기는 profile을 분리하지 않은 합성 모델 기준이며 다중 profile의 추가 비용을 증명하지 않는다. OpenAPI 검증·크기 합성은 실제 네트워크 비용과 구분한다.

## 실제 fixture 기준값

[CI 37308453121](https://github.com/XionWCFM/nanoom-fixtures/actions/runs/37308453121)은 v0.8.0, 공개 Action SHA `ee834254f875c9f88d9173c9ea075f6917bd9901`을 사용했다. 이전 prediction은 run `37298255789`에서 읽었다. 640개 작업 모두 exact source였지만 632개는 관측 1회짜리였다.

- assignment 24개의 예측 작업 합계 범위: 73.386~73.483초; 실측 범위: 43.746~122.360초.
- assignment WAPE 28.03%; 개별 task WAPE 31.83%.
- 전체 작업 시간 합계: 예측 1,762.5초, 실측 2,097.958초. 병렬 workflow wall time이 아니다.
- 준비 시간 측정은 24개지만 준비 예측은 unknown으로 `cold-cap` 24개를 유지했다. workspace-set digest 24개 전부 기록 시 trailing newline을 포함해 planner와 달랐다. 이것만으로 unknown의 모든 원인을 설명하지는 않는다.
- 모델 1,317개 항목의 평균·관측 수·만료 시각은 독립 계산과 일치했다.

원본 측정 artifact 보관은 짧으므로 장기 trace를 실제 사용자 CI에서 평가하려면 명시적인 검증용 기록 확보가 필요하다. 대표 템플릿에 시나리오 assertion을 섞지 않는다.

## 이어서 구현할 범위

1. 실제 환경 정보 수집을 제품 안에 둔다. 안정적인 성능 정보(OS/arch, CPU 모델·사용 가능한 코어, 메모리/컨테이너 제한, 이미지·도구 환경)와 job/run ID·현재 부하처럼 매번 달라지는 값을 구분한다. canonical payload의 해시와 진단 가능한 필드를 함께 남기되 hostname, IP, serial 등 머신 식별자를 요구하지 않는다.
2. 계획 시점의 runner pool과 실행 시점의 실제 환경을 구분한다. affected runner의 하드웨어를 미래 run runner의 하드웨어라고 추정하지 않는다. 동일 pool이 이질적이면 알려진 profile 범위와 불확실성을 사용하며, 정확한 profile을 선택할 수 있는 경우에만 exact environment prediction을 사용한다. 사용자에게 기본 템플릿의 추가 계산·수집 단계를 요구하지 않는다.
3. 날짜 단위 합계만으로 잃은 최근 실행 정보를 어떻게 최소 크기로 보존할지 결정한다. 후보를 안정·이상치·상승/하강·sparse·동일 날짜 trace와 실제 연속 실행에 비교한다. 보관/expiry를 맞춘 비교 후 예측기를 선택하고 artifact와 Worker의 공용 계산·schema·digest·중복·재정렬 검증을 함께 변경한다.
4. workspace/환경/작업·shard/실행 도구의 교차 조합, cold bootstrap, missing telemetry, 실패/취소, 모델 손상, 이미지와 lockfile 변경, scope 경계 및 bounded 상태 크기를 검증한다.
5. 로컬 검증 이후 새 공개 릴리즈를 발행해 두 저장소와 실제 warm fixture를 확인한다. workflow 변경이 필요하면 완성된 diff와 trigger/권한/필수 check 영향을 제시한 뒤 승인받는다. 코드·테스트·릴리즈 준비를 먼저 완료하며 이 문서를 승인으로 간주하지 않는다.

GitHub의 라벨은 runner를 선택하는 조건이며, 일치하는 여러 runner 중 하나가 선택될 수 있다. 그래서 라벨 해시만으로 실제 컴퓨팅 환경을 식별했다고 주장하지 않는다. [GitHub self-hosted runner routing](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/use-in-a-workflow), [hosted runner specifications](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## 환경 교체 회귀

이전 이미지에서 1초 관측 100개, 10일 뒤 교체 이미지에서 5초 관측 1개인 경우 전체 보관 관측 수를 섞으면 1.04초로 과소예측했다. 최신 key 관측과 7일 이내인 환경 및 날짜 bucket의 관측 비중으로 pool을 구성하도록 수정했다. 다음 pool 예측은 5초/관측 1개이고 진단 범위도 5초다. 이전 해시의 exact 조회는 1초/100개로 유지한다. 7일 기준은 관측 기반 휴리스틱이며 실제 runner 제거를 증명하는 정보는 아니다. profile별 상태는 다른 profile의 write에서도 날짜 horizon을 prune해 만료된 key가 live profile의 용량을 차지하지 않도록 한다.

## 릴리즈 후보 로컬 검증

`codex/historical-runtime-environments`에서 v0.9.0 후보의 Cargo/npm 플랫폼 패키지/lockfile을 동기화하고 정상 Yarn install 및 version contract를 확인했다. 최종 v0.9.0 후보에서 전체 local completion gate가 통과했다(라인 coverage 96.15%). 환경 회귀 11개와 전체 Rust 테스트, Clippy, Action 계약, npm wrapper/platform package 및 formatting 검증을 포함한다. 실제 D1 로컬 E2E는 collected fingerprint와 logical Scope 보존, cold 2 assignment → 적용 → immutable duplicate → warm sample 8개 → warm run을 확인했다. 공개 Cloudflare 배포와 공개 릴리즈 실행 증거로 대신하지 않는다.

실제 Rust compile/apply/projection 경로의 10,000 workspace × 2 profile(각 workspace 비용 5배, 환경 비용 2배) 회귀는 pool 10,001 row와 child 20,002 row를 생성했다. 로컬 profile의 model JSON 4,317,665 bytes, prediction JSON 3,126,882 bytes로 16/8 MiB 한도 안에 있었다. key당 최근 요약 1개인 입력이며 최대 보관 상태·전송 ZIP·네트워크 latency 검증은 아니다.

환경 수집은 표준 `/sys/fs/cgroup`의 프로세스 cgroup과 접근 가능한 상위 cgroup의 CPU/메모리 제한 중 최솟값을 반영한다. namespaced root 밖 경로는 읽지 않으며 v1 또는 확인 불가 계층은 unknown이다. group 경로는 profile과 해시에 포함하지 않는다. 최근 미분류 관측이 7일 밖의 분류된 환경보다 새로우면 현재 workspace 이력을 유지해 오래된 profile로 덮어쓰지 않는다.

## Producer CI와 명세 예제 보완

[PR #115 CI 37342038038](https://github.com/XionWCFM/nanoom/actions/runs/37342038038)은 구현 커밋 `afb0251aef33967867fab0847fa47ecc0e7d4a1e`에서 계획·Linux check·macOS test·Windows test·CI status가 모두 성공했다. 운영 단계는 공개 v0.8.0을 사용했으며 내부 개발 검증이 v0.9.0 후보 소스를 검사했다. 이 결과는 v0.9.0 released consumer 증거가 아니다.

후속 보완은 5초 → 1초 비용 감소의 prequential 검증과 공개 예제의 런타임 검증이다. 감소 trace WAPE는 기존 평균 63.79% → 후보 40.00%였다. 기존 ModelState 예제는 pruningDay가 bucket보다 과거여서 실제 core가 거부했다. 날짜를 맞추고 단일 state digest와 model bundle digest를 분리해 publish marker가 bundle을 가리키게 수정했다. Rust 회귀는 예제의 validate → projection → snapshot/marker table 및 digest 일치를 검사한다. 후속 commit의 producer CI는 별도로 확인한다.

## v0.9.0 공개 경로 검증

[릴리즈 37345422739](https://github.com/XionWCFM/nanoom/actions/runs/37345422739)은 5개 플랫폼 빌드·자산 검증·GitHub/npm 발행을 모두 통과했다. `v0.9.0`과 `latest`의 소스는 `f900c08350d647ff53562e367ebdfce22053ad09`다. 공개 macOS arm64 archive의 SHA-256 `dbe3d4bc01c3f6b981f7535c12f02aa05f3f28fb54ab0a2c9f401a5da429282d`가 게시된 checksum과 일치했고 binary가 `nanoom 0.9.0`을 출력했다. npm `@nanoom/cli`의 latest도 `0.9.0`이다.

보완 커밋 `e7cb9168d2f84b8d638549c4db226443b714a6fd`의 [producer CI 37343562730](https://github.com/XionWCFM/nanoom/actions/runs/37343562730)이 통과했고, merge SHA의 [CI 37345335081 attempt 2](https://github.com/XionWCFM/nanoom/actions/runs/37345335081/attempts/2)는 공개 v0.9.0 Action/binary로 계획·Linux·macOS·Windows·CI status를 모두 통과했다. Plan 로그에서 Action SHA와 checksum 검증된 binary 버전을 확인했다.

첫 [released fixture 37346867363](https://github.com/XionWCFM/nanoom-fixtures/actions/runs/37346867363)은 24 assignment·640개 작업 및 aggregate status가 모두 성공했다. Plan과 measurement의 작업 identity를 대조해 누락·중복이 없었다. 준비 시간 24개를 포함한 관측 664개가 적용됐고 거부된 measurement 및 degraded scope는 없었다. 같은 `ubuntu-latest`에서 AMD EPYC 7763/9V74/9V45, Intel Xeon Platinum 8573C/6973P-C의 5개 computing profile을 수집했다. 모든 profile의 정렬 JSON SHA-256을 독립적으로 계산해 fingerprint와 대조했다. 게시된 model과 prediction의 PR #33 scope에 같은 5개 profile 및 최근 batch 요약이 보존됐다. 전체 bundle의 model JSON은 270,430 bytes, prediction JSON은 214,157 bytes였다.

이 실행은 과거 push run `37298255789`의 이력으로 계획했다. assignment WAPE 25.69%, 작업 합계 예측 1,762.5초 / 실측 2,297.247초, task makespan 예측 73.483초 / 실측 132.563초였다. 아직 새 profile 학습 전의 예측이므로 새 모델의 정확도 개선 증거가 아니다. 초기 큐 대기를 포함한 CI 완료는 619초, 첫 잡 시작부터 마지막 잡 완료는 616초, status 잡은 28초, 가장 오래 걸린 checkout은 330초였다. task 합계나 makespan을 전체 CI wall time으로 대체하지 않는다.

[후속 fixture 37348388715](https://github.com/XionWCFM/nanoom-fixtures/actions/runs/37348388715)의 Plan은 첫 실행의 prediction을 1.375초에 읽었다. 640개 작업 모두 exact history였고 예측 작업 합계 2,297.247초가 이전 실측 합계와 일치했다. 24 assignment·640개 작업·aggregate status가 성공했다. 실측 작업 합계 2,079.574초, assignment WAPE 23.84%, task makespan 예측 96.044초 / 실측 149.095초였다. 첫 실행보다 WAPE는 낮지만 makespan은 커졌으며, 다른 CPU 분포와 네트워크 조건의 두 실행만으로 일반적인 개선을 주장하지 않는다. 초기 큐 대기를 포함한 CI 완료는 441초, 잡 실행 구간은 438초, status 잡은 14초, 가장 오래 걸린 checkout은 174초였다.

누적 profile에는 Xeon Platinum 8370C가 추가돼 6개가 됐다. 두 번째 history도 관측 664개를 받아들였고 거부된 measurement·degraded scope가 없었다. 모든 profile entry에 최근 batch 요약이 있었고 226개 entry는 요약이 두 개 이상이었다. 여러 profile에 존재하는 key는 450개였으며 task뿐 아니라 group fallback 및 preparation key도 포함한다. 전체 model JSON은 386,445 bytes, prediction JSON은 266,808 bytes였다.

[다음 fixture 37349596782](https://github.com/XionWCFM/nanoom-fixtures/actions/runs/37349596782)의 Plan은 이 누적 prediction을 1.27초에 읽었다. 640개 작업·24 assignment 중 444개 작업이 여러 profile의 비용 범위를 가졌고 24개 assignment 모두 `environmentUncertainty`를 보존했다. 해당 작업들의 최소 비용 합계는 1,263.564초, 최대는 1,788.290초였다. 전체 작업의 예측 합계는 2,188.569초다. 이 범위는 일부 작업의 관측 비용 범위이며 전체 assignment/CI의 신뢰구간이 아니다. 이 실행의 작업·aggregate status는 아직 진행 중이다.

Cloudflare Worker도 같은 제품 소스로 배포했고 version ID는 `debbe97b-a644-4d5b-b861-4e20e9bb3630`이다. `/health`와 D1 `/ready` 응답이 정상이다. 로컬 D1 통합 검증과 구분하며, 공개 서버의 인증된 merge·snapshot 및 CPU 한도 내 성능은 이 검증으로 증명되지 않았다.
