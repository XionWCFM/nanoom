# ADR-0017: 기본 템플릿과 제품 동작의 정합성

## 문제

v0.7.7의 기본 사용자 경로에서 root 내부 도구 checkout/설치, root 설정 변경
감지, 기본 준비 시간 기록이 빠져 있었다. 중단된 warm 재계획은 유효한 cold
Plan을 덮어쓸 수 있었고, 명시적 requiredJobs도 무관한 skipped 잡을 필수로 만들었다.
GHES 이력 다운로드는 v3 Action이 지원하지 않는 pattern/merge 입력을 사용했다.
공개 예제의 설치 선언과 task 범위, 과거 문서의 현재 계약 표기도 일치하지 않았다.

## 결정

AGENTS.md와 ADR-0015의 두 원칙을 유지한다. 사용자 보조 단계나 필수 override를
추가하는 대신 기존 계산·설치·이력·status 경계에서 원인을 수정한다.

- root manifest의 내부 도구와 transitive closure, 선언된 Yarn release/plugin 경로를
  모든 assignment checkout에 포함한다. registry dependency와 무관한 workspace는 제외한다.
- 지원 root manifest/lockfile/workspace/Nx/Turbo/TypeScript 설정과 root 도구 closure
  변경은 기본 전역 입력이다. globalDependencies는 추가 저장소 입력이다.
- pnpm focused install은 root 도구 closure를 별도 선택하고 production 환경에서도
  개발 의존성을 설치한다. npm은 native ci의 workspace 선택에 root와 실행 workspace
  양쪽 closure를 명시한다. Yarn Classic focused install은 명확히 미지원으로 거부한다.
- 잘못된 packageManager 선언과 모호한 여러 패키지 매니저 lockfile을 조용히
  다른 도구로 바꾸지 않는다. 의도적인 override는 허용한다.
- 로컬 run은 --all 또는 --base를 요구하며 --head는 --base와 함께 사용한다.
- matrix displayName에 [assignmentId]를 표시한다. 기본 run은 현재 실행 시도의
  GitHub jobs API에서 해당 진행 중 잡을 유일하게 찾아 시작 시각을 사용한다.
  준비 시간은 job 시작부터 첫 task 시작까지이며 checkout, Node 설정, 설치와
  잡 초기화를 포함한다. 조회는 3초/1 MiB 이내다. 누락·중복·API 오류는 측정만
  생략하고 task 결과를 바꾸지 않는다. preparedAtMs override는 유지한다.
- warm Plan은 별도 후보 파일에 쓴다. 재계획이 성공했을 때만 cold Plan을 교체한다.
- status는 비어 있지 않은 명시적 requiredJobs를 그대로 사용한다. 전체 needs의
  failure/cancelled는 여전히 실패이며, requiredJobs 생략 시 자동 추론을 유지한다.
- GHES는 기존 bounded API 다운로드로 현재 run/attempt의 측정 파일만 가져온다.
  다른 Plan/모델/이전 시도/만료 아티팩트는 읽지 않는다. 조회와 다운로드는
  60초/25 MiB 이내이며 실패는 기존 continue-on-error 경로에서 이력 저하로 기록한다.
- 기본/고급 예제는 독립적으로 frozen install 가능한 manifest/lockfile과 대표
  네 단계 템플릿을 제공한다. 과거 계획은 현재 계약을 설명하는 문서와 구분한다.

- 사용자 지정 설정 경로를 Plan/assignment에 보존하고 install/run에 전달한다.
  manifest-only 계획에서 누락된 설정 blob은 정확한 head에서 확보한다.
  cwd 밖 또는 안전하지 않은 경로와 checkout에 포함되지 않은 설정은 거부한다.
- 표준 history/status와 GHES history 모두 같은 bounded API downloader로 현재
  attempt 측정만 수집한다. 임시 디렉터리는 run/attempt로 분리한다.
- 명시된 npm 버전은 Corepack의 npm shim도 활성화하고, 선언 없는 npm은 Node의
  npm을 사용한다. shrinkwrap이 있으면 준비 예측/측정도 그 digest를 사용한다.
- 측정 artifact 이름에 전체 job/assignment identity digest를 붙여 truncation 충돌을 막는다.
- status/cache-key는 설정 파싱과 독립적으로 실행한다. cache-key는 선택한 설정과
  패키지 매니저 설정/shrinkwrap을 포함하고 읽기 오류를 숨기지 않는다.

## 수용 기준과 증거

실제 sparse checkout, root 도구·실행 workspace의 양쪽 dependency closure,
production 환경의 개발 도구, 무관한 workspace 제외를 검증한다. native pnpm·npm·Yarn Berry 검증은 `bash scripts/focused-install-test.sh`로 재현한다. Action 계약은 기존
`scripts/action-contract.sh`에 status, 중단된 재계획, GHES 다운로드, 준비 시간
회귀를 연결한다. 예제는 깨끗한 별도 위치에서 frozen install과 문서의 Git 비교·실행을 확인한다.

전체 로컬 gates와 변경 검토, producer CI, 새 릴리즈, 두 저장소의 최신 공개 제품
실행과 aggregate status가 모두 필요하다. 현재 이 ADR은 미릴리즈 수정의 결정이며
공개 실행의 완료 증거가 아니다. 진행과 최종 run/version/SHA는
[정합성 검토 기록](../philosophy-alignment.md)에 남긴다.

릴리즈 target 설치를 고정 Rust toolchain과 일치시키는 workflow 제안은
[완성 diff](../validation/release-toolchain.patch)에 있다. 적용 전 사용자 승인이 필요하다.
