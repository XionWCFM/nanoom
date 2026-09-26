# Nanoom 작업 원칙

모든 작업은 아래 두 원칙에서 시작한다. 기존 문서·스킬·구현이 충돌하면 이 원칙을 기준으로 바로잡는다. 원칙을 변경하거나 예외를 적용하려면 사용자에게 구체적인 변경안을 제시하고 명시적 승인을 받는다.

## 1. CI는 항상 최신 릴리즈 Nanoom에 의존한다

- nanoom과 nanoom-fixtures의 운영 CI는 관심사별 공개 Nanoom Action을 `@latest`로 사용하고, 그 Action 소스에 대응하는 최신 공개 릴리즈 binary를 사용한다.
- 운영 경로를 로컬 Action, 후보 커밋, `version: local`, `NANOOM_LOCAL_BINARY`로 대체하지 않는다. 최신 릴리즈에 문제가 있으면 제품을 수정하고 새 버전을 발행해 다시 사용한다.
- 개발 중인 코드의 빌드·단위 테스트·계약 테스트는 허용한다. 이 증거를 최신 공개 제품을 실제 사용한 증거로 대신하지 않는다.
- 완료 보고에는 실제 사용한 릴리즈 버전·소스 SHA와 두 저장소의 CI 결과를 남긴다. 기존 릴리즈 아티팩트는 덮어쓰지 않는다.

## 2. 직관적인 기본 템플릿만으로 best practice가 가능해야 한다

- 대표 템플릿은 `affected → run → status`로 구성한다. 사용자는 checkout과 공개 Nanoom Action을 연결한다.
- `affected`, `install`, `run`, `status`는 별도 Action으로 제공한다. 기존 관심사 분리를 유지하며 하나의 Action과 `command` 입력으로 합치지 않는다.
- `run` 잡은 checkout → Node 환경 설정 → Nanoom focused install → Nanoom run의 네 단계를 명시한다. checkout과 Node 설정은 공식 Action을 사용한다. 환경 준비와 설치를 run Action 안에 숨기지 않는다.
- Nanoom은 assignment별 checkout 대상과 정확한 소스 SHA, focused install 대상과 dependency closure, 실행할 작업을 제공한다. 단계 연결은 Action의 입력·출력만으로 가능해야 한다. 사용자가 jq·shell로 이벤트 해석, Plan 선택, 설치 대상 계산, 요약, 집계를 보완해야 한다면 제품의 미완성으로 취급하고 Nanoom에서 해결한다.
- affected는 Plan에서 assignment별 `checkout.ref`와 `checkout.sparseCheckout`을 matrix에 제공한다. sparse 패턴은 실행 workspace·내부 dependency closure·루트 설정과 lockfile·설정된 추가 필수 경로를 포함하고 무관한 workspace 소스는 제외한다. run의 공식 checkout은 이 값을 그대로 사용하며 소비자에게 경로를 다시 계산하거나 나열하게 하지 않는다. install/run은 matrix를 권위 있는 Plan과 대조해 검증한다.
- 패키지 매니저와 Nx·Turbo 등 실행 도구는 저장소의 manifest·lockfile·설정에서 자동으로 알아낸다. 일반적인 저장소에서 이를 Action 입력으로 다시 가르치게 하지 않는다.
- 명시적 override는 자동 판별이 불가능하거나 사용자가 의도적으로 기본값을 바꾸는 경우에만 제공한다. 대표 템플릿의 필수 입력으로 만들지 않는다.
- 필요한 Git 객체 확보와 계획한 소스의 실행을 제품이 책임진다. 최소 checkout으로 정확도를 유지한다.
- job·step 이름과 Summary만으로 작업 목적과 결과를 이해할 수 있어야 한다. 테스트 전용 시나리오 생성·assertion을 대표 사용자 템플릿에 섞지 않는다.

## 변경 절차와 완료 기준

- 모든 소프트웨어 작업에 `user-first-engineering`을 적용하고, 실제 사용자 경로의 수용 기준을 먼저 정한다. 기존 구현을 재사용하고 중복 워크플로·추상화를 추가하지 않는다.
- `.github/workflows/**`의 추가·수정·삭제(의존 Action 버전 변경 포함)는 반영 전에 완성된 diff와 trigger·권한·필수 check 영향을 제시하고 사용자 승인을 받는다. 승인된 내용이 달라지면 다시 승인받는다.
- 승인을 기다리는 동안 문서·제품 코드·테스트와 임시 위치의 워크플로 제안은 준비할 수 있다. 승인 문구나 파일을 생성하는 것으로 실제 승인을 대신하지 않는다.
- 공개 동작 변경은 회귀 검증과 문서를 함께 갱신한다. 양성 변경의 실제 matrix 실행, 변경 없음의 정상 생략, 실패·취소·필수 실행 생략의 status 실패를 검증한다.
- 로컬 테스트, producer CI, 릴리즈, 최신 공개 제품의 fixture 실행, aggregate status를 구분한다. 필요한 공개 실행 증거가 없으면 완료로 선언하지 않는다.

## 필수 설계 기준

CI·Action·설치·실행 경로를 설계하거나 수정하기 전에 [CI의 이상향과 목표 템플릿](docs/ci-philosophy.md)을 읽는다. 현재 구현이 아니라 이 사용자 경험을 기준으로 차이를 식별한다. 구현을 쉽게 만들기 위해 이상향을 축소하거나 소비자 커스텀 로직으로 우회하지 않는다.

Nanoom 저장소의 `.codex/skills/nanoom-change-review/SKILL.md`로 변경을 검토한다. 정책 근거는 Nanoom 저장소의 `docs/adr/0015-released-ci-and-template-first.md`에 있다.
