# ADR-0006: Coverage floor 폐지와 사용자 동작 중심 검증

- Status: Superseded (2026-10-06 사용자 요청)
- Original date: 2026-08-25

## 현재 결정

기존 repository-wide Rust line coverage 96% 하한을 폐지한다. 사용자가 요청한
“95% 규칙 제거”는 현재 적용 중인 96% 강제 gate까지 포함한다.
CI와 `scripts/verify-completion.sh`는 커버리지 계측으로 전체 테스트를 재실행하거나
`cargo-llvm-cov`를 설치하지 않는다. 수동 coverage 분석은 누락된 사례를 찾는
선택적 진단이며, 합격 기준이나 테스트 추가 이유가 아니다.

테스트는 실제 입력 → 관찰 가능한 출력·exit code·파일/설치/실행 결과를 검증한다.
잘못된 구현과 동일하게 기대값을 계산하거나 소스 문자열·내부 함수명·의존 Action의
patch 버전을 고정하지 않는다. 공개 metadata의 연결과 GHES/dotcom artifact protocol처럼
실행 환경의 호환성을 결정하는 계약은 유지한다.

실제 Git·CLI·Action 계약, focused install의 선택 workspace·내부 closure·root 도구·
무관한 workspace 제외, no-change와 실패·취소·필수 실행 생략의 status 결과,
최신 공개 제품의 양성 fixture 실행과 aggregate status는 계속 검증한다.
로컬 테스트가 공개 릴리즈 소비 증거를 대신하지 않는 ADR-0015 원칙은 유지한다.

삭제·유지 이유와 검증 범위는 [테스트 정리 기록](../validation/behavior-test-cleanup.md)에 둔다.
