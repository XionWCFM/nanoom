# Nanoom 공개 계약

이 문서는 현재 소스가 제공할 공개 계약을 설명한다. 배포된 기준은 v0.7.7이며
아직 릴리즈되지 않은 변경은 ADR-0017과 정합성 검토에서 구분한다.
최상위 원칙은 [AGENTS.md](AGENTS.md)와
[ADR-0015](docs/adr/0015-released-ci-and-template-first.md)다.
이전 v0.6.0 기반 후보 구현의 계획·인계·체크리스트는 역사 기록으로 보존한다.
이번 철학 정합성 수정의 진행 및 미출시 항목은
[정합성 검토](docs/philosophy-alignment.md)에 별도로 기록한다.

## 사용자 경로

대표 CI는 affected → run → status의 세 잡이다. run 잡은 공식 checkout →
공식 Node 설정 → Nanoom focused install → Nanoom run 네 단계다.
운영 Nanoom Action은 @latest와 해당 소스 버전의 공개 binary를 사용한다.
후보 binary의 로컬 회귀는 공개 제품 소비 증거를 대신하지 않는다.

- affected는 이벤트를 explicit base/head로 변환하고 영향 범위, Plan,
  모든 그룹의 compact matrix와 assignment별 checkout SHA/경로를 제공한다.
- install은 Plan을 검증·선택하고 패키지 매니저를 자동 판별하여 루트 도구와
  실행 workspace의 내부 dependency closure를 설치한다.
- run은 검증된 assignment의 모든 item을 실행하고 실제 subprocess 시간을 측정한다.
- status는 needs를 평가하고 양성 실행의 필수 잡 판단과 성공 이력 게시를 담당한다.

기본 템플릿은 소비자의 jq/shell, Plan 선택 스크립트 또는 저장소 도구의
반복 override를 요구하지 않는다. 상세 연결은 [README](README.md)와
[목표 템플릿](docs/ci-philosophy.md)에 있다.

## Git과 workspace

명시적 base/head를 우선한다. PR은 이벤트 base와 실행 merge SHA,
merge_group은 이벤트 base/head SHA, push는 before/실행 SHA를 사용한다.
마지막 성공 push SHA는 기본 비교 기준이 아니다. zero SHA는 실패한다.
CLI는 GitHub 이벤트나 API로 비교 범위를 암묵적으로 정하지 않는다.

workspace는 명시적 include/exclude 또는 저장소의 pnpm/package.json 선언에서
발견한다. 내부 링크와 로컬 버전에 맞는 semver 의존성으로 그래프를 구성한다.
변경은 transitive dependent로 전파한다. manifest 삭제/rename은 남은 workspace
전체를 보수적으로 선택하며, sparse checkout의 manifest 누락은 실패한다.

필요한 Git 이력은 tree/blob 없이 제한된 깊이로 확보한다. 기본 최대 깊이는
2048이다. 입력 revision은 실제 commit으로 해석하며 tree/blob을 거부한다.

## 작업과 Plan

work item은 group/workspace/task/shard/totalShards로 구분한다.
assignment는 한 잡에서 순차 실행할 item 묶음이다. concurrency는 Nanoom
assignment 상한이며 GitHub max-parallel이 아니다. distribution이 없으면
item마다 assignment를 만든다. configured tier의 경계는 inclusive다.

Plan v1은 상세 계획 artifact와 작은 digest/provenance reference다.
정적 matrix는 group, assignmentId, displayName, checkout.ref와
checkout.sparseCheckout을 제공한다. install/run은 원본 Plan digest,
repository/workflow/run/attempt/head, assignment 내용과 실제 HEAD를 검증한다.
선택한 설정 경로를 configPath로 보존해 install/run이 같은 설정을 사용한다.
설정 경로도 assignment checkout에 포함되며 cwd 밖의 설정은 Plan에서 거부한다.
하위 프로젝트는 선택적 workingDirectory로 실행 위치를 보존한다. 값과 item/
checkout 경로는 Git 루트 기준이고 configPath는 프로젝트 기준이다. 공식 checkout은
하위 프로젝트의 루트 메타데이터와 assignment 소스를 가져오며 install/run은
Plan에서 cwd를 복원한다. 다른 하위 디렉터리와 symlink redirect는 거부한다.
저장소 루트 실행은 필드를 생략한다. 이 계약은 미릴리즈 v0.8.0 후보에 해당한다.
상세 affected workspace path는 프로젝트 기준 상대 경로다. 운영체제의 canonical
절대 경로 표현은 workspace 출력과 Plan 경로의 계약에 포함하지 않는다.
그룹당 256 assignment와 compact 출력 UTF-16 1 MiB 제한을 넘으면 실패한다.
변경 없음은 assignment 0개인 정상 Plan이다.

공식 checkout이 기본 경로다. prepare 기반 .nanoom/ 격리 checkout도 지원한다.
선택적 cleanup은 검증된 격리 경로만 삭제한다.
하위 프로젝트 cwd의 cleanup도 격리 checkout 전체를 삭제하며 기본 저장소를
가리키는 하위 폴더는 거부한다.

## 실행과 상태

실행 도구와 패키지 매니저는 저장소 선언에서 자동 판별한다. ambiguity를
자동 판별할 수 없거나 의도적 변경이 필요할 때 override한다.
install Action의 패키지 매니저 활성화는 Plan/assignment 검증 후 같은 실행 경계에서
수행한다. prepare 경로에서도 같은 활성화를 수행하고 설치 프로세스에 즉시 shim을
제공한다. 활성화 실패는 설치 전에 실패한다.
Yarn Berry와 pnpm의 focused install은 루트 도구·내부 closure·개발 의존성을 포함한다.
npm 전체 설치/실행은 지원하지만 focused install은 공개 v0.7.7에서 미지원이다.
미릴리즈 정합성 수정은 npm ci의 native workspace 선택으로 focused install을 제공한다.
현재 브랜치의 변경은 ADR-0017과 정합성 검토 기록을 참고한다.

계획한 workspace 실행이 없는 성공은 거부한다. assignment는 첫 실패에
중단하고 남은 item을 pending으로 남긴다. CLI의 continue-on-error도 최종
실패 exit code와 completed/failed/pending/executions JSON을 유지한다.
shard 환경변수는 자식 프로세스에만 전달한다.

status는 실패·취소와 필수 실행의 누락·생략을 실패로 처리한다.
no-change의 의도적 생략은 성공이다. 명시적 results의 중복 job ID는 거부한다.
이력 장애는 task 성공을 실패로 바꾸지 않고 degraded로 기록한다.

## 실행시간 이력

기본 scheduler는 artifact다. 성공 measurements를 ModelState와 작은
PredictionArtifact v3로 병합한다. prediction을 마지막 publish marker로 게시한다.
planner는 model/raw measurement를 받지 않고 scope별 compact prediction만 받는다.
PR scope를 먼저 확인하고 base branch를 fallback으로 사용한다.

exact task/layout/runner/environment key → workspace를 제외한 동일 key → cold 1
순서로 예측한다. 최근 30 UTC일 안의 최대 7개 일별 count/sum과 7일 half-life
가중 평균을 사용한다. 준비 이력이 충분할 때에만 preparation+task 예상 비용으로
assignment 수를 자동 선택하며, unknown/cold에서는 선택 tier의 상한을 유지한다.

planning 이력 I/O와 parse의 공유 예산은 3초다. 손상·만료·시간/크기 제한·네트워크
오류는 cold fallback이다. 선택지가 없으면 이력 metadata 요청도 생략한다.
데이터/산식/한도는 [PredictionState v3](docs/prediction-model-spec.md)가 정의한다.

선택적 History Worker는 Workers/D1의 prediction 조회·관측 병합 backend다.
기본 artifact 경로를 대체하도록 강제하지 않는다.
scheduler=http는 별도 coordinator의 등록/claim/heartbeat/완료 client이며,
History Worker가 queue/lease 서버 역할까지 구현하는 것은 아니다.

status/cache-key는 설정 파싱과 독립적으로 동작한다. cache-key는 선택한 설정,
manifest, 패키지 매니저 설정과 lockfile을 해시하며 파일 읽기 오류는 실패한다.
작업 source의 산출물 캐시나 remote cache는 제공하지 않는다.

## 증거와 미검증 범위

최신 공개 제품의 producer/consumer CI와 실제 양성 assignment 실행,
aggregate status를 로컬 gates와 별도로 확인해야 한다. 과거 green run을
새 소스의 released 검증으로 승계하지 않는다. 실제 GHES, 임의 self-hosted pool,
성능 개선은 해당 환경의 증거 없이는 주장하지 않는다.
