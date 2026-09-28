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

## 진행 증거

- 두 저장소 immutable install, hook 설치와 staged 검사 통과.
- Nanoom yarn check와 npm wrapper smoke 통과.
- fixture 공통 TypeScript 설정으로 typecheck 136개, test 136개, build 133개 작업 통과.
- Turbo cache hit에서 실제 dist 파일 복원 확인.
- 생성되는 next-env.d.ts 추적을 제거하고 native next typegen을 사용.
- Next/React runtime 버전은 기존 catalog에 모음.

## 남은 검증

전체 formatter 적용 범위, hook의 실제 staged 파일 동작, 공통 TS/Vite 설정,
개발 및 CI 검사 연결, clean install과 released hosted CI를 확인한다.
