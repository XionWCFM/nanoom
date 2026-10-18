# Go 개발 CI 환경 설정 제안

전체 diff: `proposal.patch`. 2026-10-18 사용자가 전체 diff와 Go 개발 CI 예외를 명시적으로 승인하여 반영했다.

- CI의 pull_request/merge_group/push-main trigger와 contents/actions read 권한 유지.
- job·필수 aggregate check 이름과 조건·의존성 유지.
- run job의 공식 Node setup 다음에 공식 setup-go@v7을 추가하여 go.mod의 버전을 준비한다.
- Nanoom 자체 Go 개발 CI에만 언어 도구 설정 예외를 명시한다. Node 소비자 대표 네 단계와 nanoom-fixtures에는 변경 없음.
- 원칙 예외를 숨기지 않도록 AGENTS.md, ci-philosophy.md, ADR-0015 변경도 전체 diff에 포함한다.
- 근거: PR #131, run 39122829078 macOS job 117661772388의 `gofmt: command not found`.
