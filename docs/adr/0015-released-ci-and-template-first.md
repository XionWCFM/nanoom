# ADR-0015: 최신 릴리즈 CI와 템플릿 우선 제품 설계

- Status: Accepted
- Date: 2026-09-27

## 결정

최상위 원칙은 루트 `AGENTS.md`에 둔다. nanoom과 nanoom-fixtures는 동일한 핵심 원칙을 각 저장소에서 독립적으로 읽을 수 있게 유지한다. 스킬은 이 원칙을 검토 절차에 연결하며 별도 정책을 만들지 않는다.

1. CI는 항상 최신 릴리즈 Nanoom에 의존한다.
2. 모든 사용자는 직관적인 Nanoom 기본 템플릿만으로 best practice를 사용할 수 있어야 한다.

운영 CI의 Nanoom은 관심사별 공개 `@latest` Action과 대응 릴리즈 binary다. `affected`, `install`, `run`, `status`는 별도 Action으로 유지하고 하나의 Action과 `command` 입력으로 통합하지 않는다. 개발 코드 테스트는 별도 증거다. 공개 경로가 실패하면 소비자에 우회를 붙이지 않고 제품 수정과 새 릴리즈로 해결한다.

`run` 잡은 공식 checkout → 공식 Node 설정 → Nanoom focused install → Nanoom run의 네 단계를 명시한다. run Action에 checkout·Node 설정·설치를 숨기지 않는다. Nanoom은 정확한 checkout SHA·assignment 경로, 집중 설치 대상과 dependency closure, 계획된 작업을 제공하고 Action 입력·출력으로 연결한다.

affected의 compact matrix에는 Plan에서 계산한 `checkout.ref`와 `checkout.sparseCheckout`을 제공한다. non-cone sparse 패턴은 루트 설정·lockfile, 실행 workspace와 내부 dependency closure, 설정된 추가 필수 경로를 포함한다. run의 공식 checkout은 이를 그대로 소비한다. matrix의 checkout 메타데이터는 Plan을 대체하지 않으며 install/run에서 정확한 소스 SHA와 assignment를 검증한다. 수용 검증은 필요한 파일 포함과 무관한 workspace 소스 제외를 실제 checkout 결과로 확인한다.

이벤트 처리·Plan 검증과 선택·설치 대상 계산·이력 처리·요약·집계는 제품 책임이다. 일반적인 Nx·Turbo·패키지 매니저 설정은 저장소에서 자동 판별한다. 기본 템플릿이 소비자 jq나 도구별 필수 override를 요구하면 이 원칙을 충족하지 못한다.

워크플로 변경은 완성된 diff와 실행 영향을 준비한 뒤 사용자의 명시적 승인으로 반영한다. 기존 승인 범위를 벗어나는 변경은 다시 승인받는다. 원칙 변경과 예외도 승인 대상이다.

## 기존 결정과의 관계

- ADR-0004의 회귀·문서·실제 소비 경로 검토는 유지한다. 현재 저장소에 없는 ADR-0002 대신 이 문서의 완료 기준과 적용되는 기능별 ADR을 참조한다.
- ADR-0010의 마지막 성공 push 기준은 사용자의 새 결정으로 대체한다. 기본 비교는 PR·merge queue의 이벤트 SHA와 push의 `before/after`를 사용하고 명시적 base/head 입력을 우선한다. 구현과 문서는 별도 변경에서 맞춘다.
- ADR-0008의 세 잡 구조와 GitHub `needs` 기반 집계는 유지한다. 필수 run 판단에 필요한 Nanoom 출력 연결은 기본 템플릿과 Action에서 제공하며 사용자 보조 스크립트로 해결하지 않는다.

## 수용 기준과 현재 상태

최신 공개 릴리즈를 사용하는 두 저장소 CI와 실제 fixture의 양성 변경 matrix·aggregate status 성공을 확인한다. 변경 없음은 정상 생략, 필요한 실행의 실패·취소·생략은 status 실패여야 한다. 릴리즈 버전·소스 SHA와 각 실행 증거를 별도로 보고한다.

릴리즈의 artifact 조회는 이력 조회 예산을 설정하지 않아도 동작해야 한다. 공용 전송 함수를 재사용하더라도, 이력 경로에서 명시한 시간·용량 제한은 유지하고 릴리즈 경로에 초기화되지 않은 이력 제한을 적용하지 않는다. 이 두 호출 방식은 별도로 회귀 검증한다.

릴리즈 워크플로는 artifact archive 다운로드에 필요한 `actions: read` 권한을 명시한다. 2026-09-27 사용자는 전체 CI 전환과 이 릴리즈 권한 수정을 승인하고, 목표 달성에 필요한 후속 작업도 추가 승인 없이 진행하도록 허용했다.

공개 binary의 checksum은 파일명 표기가 아닌 파일 바이트로 검증한다. Windows runner의 역슬래시 경로가 GNU checksum 출력의 escape marker를 만들더라도 digest에 포함되지 않아야 한다. 설치와 이력의 파일 digest 계산은 같은 기준을 유지하고, 역슬래시 경로의 실제 다운로드·검증·실행을 회귀 검사한다.

이 ADR은 작업 기준의 확정이다. 기존 워크플로와 공개 인터페이스가 이미 준수한다고 주장하지 않는다. 워크플로 정리와 관심사별 Action의 사용자 경로 개선은 제품·테스트 준비 및 워크플로 diff 승인 후 수행한다.

PR synchronize의 `after`는 branch HEAD이므로 Plan의 head로 사용하지 않는다. PR 실행의 merge SHA와 matrix checkout·install 검증의 소스 정체성이 같아야 한다. merge_group은 head_sha를 사용한다.

명시적 workspace.include가 없으면 pnpm-workspace.yaml의 packages 또는 package.json의 workspaces 선언에서 범위를 자동 판별한다. 제외 패턴과 명시적 override를 보존하고, 잘못된 선언을 기존 기본 경로로 조용히 대체하지 않는다.
