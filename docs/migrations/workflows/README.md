# Go 릴리즈 워크플로 변경 제안

2026-10-18 diff를 제시한 뒤 사용자가 작업 진행을 지시하여 `.github/workflows/release.yml`에 반영했다. 전체 diff는 `release.patch`, 적용 후 파일은 `release.yml.proposed`다.

- Trigger: `v*` tag push와 기존 tag를 입력하는 `workflow_dispatch` 유지.
- 권한: `actions: read`, `contents: write`, `id-token: write` 유지.
- 필수 check: 운영 CI workflow와 aggregate status 이름·의존성·조건은 변경하지 않는다. Release의 기존 job 이름과 5개 matrix target 이름도 유지한다.
- 빌드: Rust target/cache/cross를 `actions/setup-go@v7`과 `go.mod`의 Go 버전, CGO 없는 Go 교차 빌드로 대체한다. 플랫폼 archive 이름·checksum·Sigstore 서명·draft 검증 후 공개 순서·npm publish·latest 갱신은 유지한다.
- 실행 검증: builder의 OS/arch와 일치하면 binary version과 Go CLI 테스트를 실행한다. 교차 빌드 플랫폼은 archive 검사와 별도로 실제 runner 증거가 필요하다.
- 운영 CI: `affected/install/run/status@latest` 연결은 유지한다. Go 소스와 모듈은 기존 sparse checkout 설정에서 포함한다.

승인 범위는 이 diff의 Go 빌드 전환이다. 승인은 새 릴리즈 공개 성공이나 Worker 운영 CPU 검증을 대신하지 않는다.
