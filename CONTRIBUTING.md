# 개발과 기여

## 처음 시작하기

저장소 루트에서 다음 명령을 실행합니다. Node는 `mise.toml`, Yarn은
`package.json`의 `packageManager`를 기준으로 선택합니다.

```sh
mise install
mise exec -- corepack enable
mise exec -- yarn install --immutable
mise exec -- yarn prepare
```

`yarn prepare`는 커밋 hook을 설치합니다. hook은 staged 파일의 포맷과 해당
언어 검사를 실행합니다. 전체 저장소 검사와 build는 아래 명령으로 별도 실행합니다.
hook 실패를 생략해 커밋하지 말고 같은 명령의 오류를 수정합니다.

## 수정과 검증

```sh
mise exec -- yarn format
mise exec -- yarn format:check
mise exec -- yarn check
```

`format`은 파일을 수정하고 `format:check`는 수정하지 않습니다. VS Code의
추천 확장을 설치하면 같은 formatter로 저장 시 포맷을 적용합니다.

버그 보고에는 실행 버전, OS, 최소 재현 입력, 예상 결과와 실제 결과를 남깁니다.
PR에는 사용자에게 달라지는 동작과 실행한 검증을 기록합니다. 공개 동작 변경은
회귀 테스트와 문서를 함께 수정합니다. Nanoom 운영 CI는 최신 공개 Action과
해당 릴리즈 binary를 사용하며, 로컬 build를 공개 제품 실행 증거로 대신하지 않습니다.

워크플로 변경은 AGENTS.md의 diff 승인 절차를 따릅니다.

## Rust와 공개 패키지

`rust-toolchain.toml`과 `mise.toml`은 같은 Rust 버전을 사용합니다.
전체 수용 검증은 `bash scripts/verify-completion.sh --local`이며
`cargo-llvm-cov`와 `llvm-tools-preview`가 필요합니다. macOS와 Windows의
기본 검사도 `yarn check`로 수행할 수 있습니다. 공개 npm wrapper 검증은
`cargo build` 후 `node packages/cli/smoke-test.js`로 실행합니다.

포맷 명령의 대상은 Rust와 유지하는 Node 코드·설정입니다. 기존 문서,
예제와 Action 자산은 해당 계약 검증으로 따로 검토하고 대량 재포맷하지 않습니다.
