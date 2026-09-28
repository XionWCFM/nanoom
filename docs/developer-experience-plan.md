# Suspensive 기반 개발 경험 통합

참조: toss/suspensive `5b76bda7c5225c86634bdfd76ede4e171d6d3c23`.
실제 확인한 package.json, .lintstagedrc, .husky/pre-commit, .vscode,
configs/tsconfig, CONTRIBUTING.md, knip.json, packlint.config.mjs를 기준으로 한다.

## 수용 기준

1. 두 저장소의 mise 환경에서 immutable install과 hook 설치가 성공한다.
2. format은 수정하고 format:check는 수정하지 않는다. 저장 시 formatter도 동일하다.
3. 작은 staged 파일 커밋은 전체 fixture build/typecheck를 실행하지 않는다.
4. Rust/Node 버전과 Yarn 선택은 선언과 실행 결과가 일치한다.
5. 기존 전체 검사, 실제 workspace 인식, build/test/typecheck 경로를 유지한다.
6. 공통 TypeScript/Vite 설정의 중복, root devDependencies, Knip 예외와
   Turbo cache output을 실제 소비 코드와 대조해 개선한다.
7. PR/기여 문서와 필요한 editor 설정으로 새 기여자가 위 경로를 따라갈 수 있다.
8. workflow 제안은 완성된 diff 및 trigger/권한/필수 check 영향을 준비한 뒤
   AGENTS.md 승인 절차를 따른다. 최신 공개 제품의 두 저장소 CI로 검증한다.

## 통합 방향

Suspensive의 staged hook, editor formatter 연결, 공통 설정, 기여 경로와
불필요한 의존성 검사를 기존 Yarn/Cargo/Turbo 도구에 통합한다.
Husky/lint-staged/packlint/sherif를 중복 도입하지 않고 기존 Lefthook,
Oxfmt, Knip과 workspace catalog를 사용한다. pnpm, React 전용 ESLint 설정이나
웹 문서 배포는 두 프로젝트의 현재 개발 경로에 맞지 않아 그대로 복제하지 않는다.

## 수용 기준별 검증 결과

2026-09-28 기준. 구현은 [Nanoom PR #103](https://github.com/XionWCFM/nanoom/pull/103)과
[fixture PR #29](https://github.com/XionWCFM/nanoom-fixtures/pull/29)로 통합했다.

| 기준 | 구현 및 확인한 결과 |
| --- | --- |
| 1. 새 checkout 설치 | 두 저장소의 clean checkout에서 immutable install, hook 설치, format:check 성공. fixture의 새 checkout에서 Next 타입 생성과 service typecheck 성공. |
| 2. formatter 연결 | format과 format:check를 분리하고 VS Code도 Oxfmt를 사용. Rust는 rust-analyzer를 사용. Windows의 실제 CI에서 발견한 CRLF 차이는 .gitattributes의 LF 고정으로 수정하고 Windows CI로 재검증. |
| 3. staged hook | 실제 Git hook에서 잘못된 JSON을 거부하고 수정된 JSON을 허용. 공백이 있는 파일명도 검증. Lefthook 2.1.14를 저장소 의존성으로 선택해 기존 전역 1.11.5의 간섭 제거. 전체 build/typecheck는 check에 유지. |
| 4. 도구 선택 | 두 저장소 Node 24.18.0, Yarn 4.11.0. Nanoom Rust 1.98.0을 mise와 rust-toolchain.toml에서 일치시킴. 릴리즈 workflow의 moving stable 문제는 아래 승인 대기 항목으로 분리. |
| 5. 전체 검사 | Nanoom yarn check와 npm wrapper smoke 성공. fixture 최종 yarn check에서 typecheck 134개, test 134개, build 132개 작업 성공. typecheck/test 수에는 의존 build 2개가 포함됨. 설정 전용 workspace의 가짜 build/test/typecheck 제거. |
| 6. 공통 설정과 의존성 | 128개 Next service의 TS 옵션을 루트 tsconfig.next.json으로 통합하고 Next/React 버전을 기존 catalog에 모음. root Vite/Vitest 중복 의존성 제거. Knip의 오래된 turbo 예외 제거. Turbo cache hit에서 실제 dist 파일 복원 확인. |
| 7. 기여 경로 | 두 저장소 CONTRIBUTING, PR template, 버그 제보 양식, editor 설정 갱신. 버전은 manifest와 도구 설정을 기준으로 안내. |
| 8. 공개 CI | 아래의 PR 및 main 실행에서 최신 공개 Action과 대응 binary, 실제 fixture 작업과 aggregate status를 확인. 이번 통합에서는 운영 workflow를 변경하지 않음. |

fixture의 짧은 Vite 설정 세 개는 그대로 유지한다. 각 파일은 기본 library build
옵션만 선언하며, 공유 factory나 새 설정 package를 추가하면 import 및 workspace
의존성 관리가 늘어난다. 128개 service에서 반복되던 TypeScript 옵션과 달리
현재 규모에서는 별도 추상화를 만들 이득이 없다.

생성되는 next-env.d.ts는 추적하지 않고 native next typegen으로 만든다.
사용하지 않는 vitest/globals 타입도 제거했다. 실제로 import 없는 describe를
추가하면 tsc가 실패하는 것을 확인하고 임시 변경을 복원했다.

## 공개 실행 증거

이번 변경은 개발 환경과 검사 연결을 개선했으며 새 binary 릴리즈를 발행하지 않았다.
두 저장소 운영 CI가 사용한 공개 버전은 **v0.7.7**, Action과 binary의 소스 SHA는
**b76e6da976523b20ca2eba7f3988d179945427cb**다.

| 저장소 | 검사 대상 | 실행 및 결과 |
| --- | --- | --- |
| Nanoom | PR 최종 fb37b9b1c0bc7573d3d958f29bd1c52798d64d45 | [CI 36431955329](https://github.com/XionWCFM/nanoom/actions/runs/36431955329): Linux/macOS/Windows와 status 성공 |
| Nanoom | main ba11a1793d7b7088277a521cbe24e93cf454a22e | [CI 36433455815](https://github.com/XionWCFM/nanoom/actions/runs/36433455815): Linux/macOS/Windows와 status 성공 |
| nanoom-fixtures | PR 최종 b0cefc6063aba1435214d9c7a385af42e6812977 | [CI 36432484626](https://github.com/XionWCFM/nanoom-fixtures/actions/runs/36432484626): Plan, 24개 matrix assignment, status 성공 |
| nanoom-fixtures | main 10524738aa1a07a98839c05ae8b3162752ab8b0d | [CI 36433488785](https://github.com/XionWCFM/nanoom-fixtures/actions/runs/36433488785): 성공 |

fixture 최종 PR의 measurement artifact 24개에서 중복 없는 executionId 640개와
128개 workspace 각각의 build/test/typecheck/format:check/lint 실행을 확인했다.
모든 실행의 durationMs가 양수였다. 이는 정상 생략이나 단순 job 성공만으로
양성 실행을 대신하지 않는 증거다. 공개 v0.7.7 CLI의 affected 결과도 같은
24개 assignment와 640개 work item을 계획했다.

## 승인 대기 항목

release.yml의 dtolnay/rust-toolchain@stable은 moving stable에 target을 설치한다.
소스의 고정 toolchain과 stable이 달라지면 release target이 다른 toolchain에
설치될 수 있다. 소스 toolchain을 선택하는 native rustup target add로 교체하는
완성된 diff와 trigger·권한·필수 check 영향 설명을 사용자에게 제시했다.
5개 target과 기존 발행 순서는 유지하는 제안이다.

AGENTS.md의 workflow 승인 절차에 따라 이 변경은 아직 적용하지 않았다.
로컬에서 RUSTUP_TOOLCHAIN override를 제거한 상태로 소스의 1.98.0 선택과
target 설치를 확인했으나, 이는 수정된 release workflow의 공개 실행 증거가 아니다.
이 항목의 승인 또는 현행 유지 결정 전에는 전체 목표의 완료를 선언하지 않는다.
