# 사용자 동작 중심 테스트 정리 (2026-10-06)

## 수용 기준

| 입력·조건 | 기대 결과 | 검증 경로 |
| --- | --- | --- |
| 일반 개발/CI 검사 | 커버리지 하한·계측 재실행·도구 설치 없이 동작 검사 | `scripts/ci-task.sh`, `scripts/verify-completion.sh` |
| schema CLI stdout 및 파일 출력 | 공개 schema와 JSON 의미가 동일 | `schema_stdout_and_file_match_the_published_configuration_contract` |
| Git 변경·shallow checkout·Plan 선택 | 정확한 SHA·assignment·sparse 파일 집합, 변조 거부 | Git/Plan CLI와 Action 계약 |
| focused install | root 개발 도구와 선택 workspace의 내부 closure 사용 가능, 무관한 workspace 제외 | `scripts/focused-install-test.sh`의 native pnpm/npm/Yarn |
| 계획된 실행의 성공·실패·취소·필수 생략 | 성공만 통과; no-change 생략은 통과 | CLI status, `scripts/status-action-test.sh`, fixture completion |
| 생성된 Next.js smoke | 실제 React 페이지가 해당 workspace를 렌더링; 잘못된 페이지는 실패 | 생성기 임시 실행 및 mutation 확인, 공개 fixture PR |
| 최신 공개 제품의 양성 변경 | 계획한 assignment 실행 및 aggregate status 성공 | 두 저장소 PR CI, Plan artifact와 실행 로그 |

## 삭제와 통합 이유

- `scripts/action-contract.sh`의 소스 문자열·함수명·JSON 생성 표현식·정확한 Action patch 버전 검사는 실행 동작을 증명하지 않는다. YAML을 읽어 공개 input 설명·output 참조 연결·GHES v3/dotcom v4 artifact 호환성을 검증하고, 기존 실행 계약 스크립트는 모두 유지했다.
- `affected`의 가짜 JSON과 텍스트 테스트는 CLI를 호출하지 않거나 테스트에서 만든 문자열을 확인했다. 실제 affected CLI/Action 테스트로 검증한다.
- `main`의 세 테스트는 출력 검증 없이 로그/dispatch/verbose 호출이 예외 없이 끝나는지만 봤다. 실제 CLI help/version/JSON 오류와 stdout/stderr 계약을 유지했다.
- emoji와 부분 문자열 serialization, workspace lookup/relative path 접근자, 직접 만든 기본 Config, schema 내부 타입명 검사를 제거했다. 실제 설정 파일 로딩·workspace discovery·status JSON·schema CLI 계약은 유지했다.
- Git primitive 6개는 `src/git.rs`와 중복되거나 SHA가 비어 있지 않음만 검사했다. 더 강한 원본 단위 검사와 shallow linked worktree·Unicode/구분자 파일명 회귀는 유지했다.
- schema stdout/file smoke와 Action 스크립트의 별도 schema 생성은 하나의 실제 CLI 계약 테스트로 통합했다. 공개 `nanoom.schema.json`과 양쪽 결과를 비교한다.
- 생성기의 문자열 자기 비교를 실제 페이지 렌더링으로 교체했다. 대표 service에서는 `createElement(Page)`를 렌더링하여 React component 경계를 사용한다.

128개 Next service는 독립적인 제품 기능 회귀 128개를 의미하지 않는다. 큰 monorepo의 실제 Next/Vitest/TypeScript 작업 부하를 제공하는 scale fixture이므로 유지한다. 작은 app/core/shared fixture도 dependency 연결을 실제 import로 확인하므로 유지한다. 예측 trace 비교는 성능 모델의 제한과 regression을 검증하므로 유지하며, 일반적인 정확도 보장으로 해석하지 않는다.

## 정책과 실행 영향

ADR-0006의 96% 하한(사용자 요청의 95% 규칙)을 폐지했다. 정책·인계 문서의 강제 기준도 갱신했다. 과거 실행 결과의 coverage 수치는 역사적 증거로 유지한다.

워크플로 파일은 변경하지 않는다. trigger, permissions, job/required check 이름과 공개 `@latest` 연결은 동일하다. Linux check에서 llvm-tools-preview와 cargo-llvm-cov 설치 및 계측 테스트 재실행을 제거하며, macOS/Windows의 기존 실제 테스트를 유지한다. 릴리즈 binary/Action 공개 동작은 바뀌지 않으므로 새 릴리즈는 필요하지 않다.

## 로컬 결과

- macOS 개발 환경: 전체 Rust **261개 통과**, 실패/ignored 없음. Clippy(all-targets/all-features, warnings denied), formatting, diff check 통과.
- 모든 기존 Action 실행 계약 통과: status, coordinator, assignment, preparation clock, artifact/history/GHES, setup, Plan, native focused install, revision/shallow fetch, cleanup, fixture completion.
- 플랫폼 package 5개 계약과 npm wrapper smoke 통과.
- fixture 대표 Next service와 app→core→shared 테스트 통과. 생성기에서 임시 100개 service를 만들고 한 service를 실제 Vitest로 실행했다. 페이지를 잘못된 identity로 바꾸면 같은 smoke가 실패했다.
- 커버리지 계측 재실행 제거는 명령 경로에서 확인했다. 비교 wall-time 측정을 하지 않았으므로 실행 속도 개선율은 주장하지 않는다.

공개 PR CI·실제 Plan assignment·aggregate status는 별도로 확인하고 아래에 실행 링크와 소비 릴리즈를 기록한다. 로컬 green을 공개 경로 완료로 대신하지 않는다.
