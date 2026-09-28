# ADR-0016: 간결하고 재현 가능한 개발 환경

## 결정

Suspensive의 staged hook, 에디터와 formatter 연결, 공통 설정과 기여 문서를
Nanoom과 nanoom-fixtures의 기존 도구에 통합한다. Yarn과 Cargo를 유지하며
Husky/lint-staged나 추가 설정 검증 프레임워크를 중복 도입하지 않는다.

Node는 mise, Yarn은 packageManager, Rust는 mise와 rust-toolchain의 동일한
정확한 버전으로 선택한다. Lefthook은 저장소 의존성의 실행 파일을 사용해
전역 설치 버전에 영향을 받지 않는다. format은 수정, format:check는 검사다.
commit hook은 staged 파일 검사이며 전체 검사는 check 명령과 기존 CI 작업에 둔다.

fixture의 반복 Next TypeScript 옵션과 runtime 버전은 공통 설정 및 기존
catalog로 모은다. 생성 타입은 추적하지 않고 native next typegen으로 만든다.
Turbo는 실제 build 산출물을 캐시하고 공통 설정 변경을 입력에 포함한다.

## 검증

immutable install, 실제 Git hook의 성공과 실패, formatter 비수정 검사,
전체 test/typecheck/build, cache hit의 산출물 복원을 확인한다. 로컬 검증과
공개 최신 릴리즈를 사용하는 두 저장소 CI는 별도의 증거로 기록한다.
운영 workflow 변경은 AGENTS.md의 승인 절차를 유지한다.

Windows에서도 checkout과 formatter가 같은 LF를 사용하도록 .gitattributes로
줄바꿈을 고정한다. editor 설정만으로 Git autocrlf 변환을 막을 수는 없다.
