# ADR-0017: Go CLI와 Cloudflare Go/Wasm Worker로 전환

- Status: Accepted for implementation; public rollout pending
- Date: 2026-10-18

## 결정

사용자의 전체 Go 전환 요청과 Cloudflare Workers + D1 유지 조건에 따라 CLI·계획/분배·예측 엔진·서버 인증/HTTP/SQL을 Go로 구현한다. 서버는 `workers-go`의 Go/Wasm adapter를 사용한다. npm wrapper와 GitHub Action YAML·플랫폼 연결 스크립트는 해당 플랫폼의 배포 경계로 유지한다.

기존 v3 JSON·canonical digest·scope/profile 분리·replay/CAS·retention과 기존 D1 schema/key, HTTP endpoint, Plan/reference, CLI command/JSON/exit, npm 5개 archive 계약을 보존한다. Go runtime에서 Rust binary로 proxy하지 않는다. 버전 source of truth는 `packages/cli/package.json`이며 Go build에 주입한다.

## 전환 경계

기본 개발 검사와 빌드는 Go를 사용한다. 기존 Rust 소스와 test harness는 전환 중 oracle로 보존하고, 새 공개 제품의 사용자 경로가 검증된 뒤 제거한다. 같은 checkout의 다른 작업 변경은 덮어쓰지 않는다. 기존 공개 release asset은 변경하지 않으며 새 version을 발행한다.

운영 CI의 관심사별 `@latest`와 공식 checkout → Node setup → focused install → run 템플릿은 ADR-0015를 그대로 따른다. 후보 binary는 개발 검증 대상이며 공개 소비 증거로 대체하지 않는다. workflow 변경은 완성된 diff의 사용자 승인 이후 반영한다.

Cloudflare에 Go version을 업로드하는 것과 운영 traffic 전환은 구분한다. 로컬 workerd/D1 E2E·압축 크기·startup 시간으로 요청 CPU와 memory 예산 통과를 추정하지 않는다. 플랫폼 한계가 발견되면 숨기거나 다른 배포 환경으로 바꾸지 않고 같은 Workers/D1 조건에서 해결한다.

## 검증

입력→관찰 결과→검사 매핑과 실제 검증 상태는 [Go 마이그레이션](../migrations/go.md)에 둔다. 예측/스케줄러 Rust oracle, 실제 binary 비교, 기존 CLI/Action 계약, 실제 focused install, 전체 Go CLI+Worker D1 E2E, 5개 release archive 검사를 수행한다. Native Windows와 공개 producer/release/fixture/aggregate 증거가 없으면 완료가 아니다.
