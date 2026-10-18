# Go 마이그레이션

상태: 진행 중. Go 소스가 추가됐다는 이유만으로 운영 전환 또는 완료로 간주하지 않는다.

## 범위와 순서

1. Rust 공용 prediction-core를 Go `internal/prediction`으로 옮긴다. 기존 v3 JSON과 SHA-256 identity, 모델 검증, replay, pruning, runner profile 분리를 보존한다.
2. 서버 HTTP 계약·인증·저장소 CAS를 Go로 옮긴다. 기존 D1 데이터와 endpoint를 바꾸는 배포는 대상 플랫폼과 데이터 이전/복구 방법을 확정한 뒤 수행한다.
3. CLI의 config/workspace/Git/affected/scheduler/Plan/install/run/history/status/cache-key/schema를 옮긴다. 기존 JSON, stdout/stderr, exit code 및 실제 자식 프로세스 동작을 비교한다.
4. 플랫폼 binary·npm wrapper·개발 도구·문서를 Go 빌드로 연결한다. npm 소비를 위한 JavaScript wrapper, Action YAML과 실행 환경 연결 스크립트는 해당 플랫폼 경계로 유지한다.
5. 완성된 workflow diff와 trigger/권한/필수 check 영향을 제시하고 승인 후 CI/release를 전환한다. 새 공개 릴리즈 및 두 저장소 소비 검증 후 대체된 Rust 소스를 제거한다.

## 수용 기준

| 입력/전제 | 관찰 결과 | 검증 |
| --- | --- | --- |
| 기존 scope/key/profile/batch JSON | Rust와 동일한 canonical bytes 및 SHA-256 | 고정 digest, 언어 간 differential 검사 |
| 기존 measurement/model/prediction v3 | 양쪽 구현에서 읽고 동일한 학습·projection 결과 | 기존 예제와 생성 fixture |
| 동일 batch 재전송 / 동일 identity의 다른 body | 중복은 무변경, 충돌은 실패 | 모델 bytes 비교 및 HTTP 검사 |
| 오래된/미래/oversized/unknown-field 입력 | 제한과 검증을 보존하고 거부 | 입력 경계 계약 검사 |
| 여러 runner profile / 순서가 다른 batch | profile 오염 방지, 최신 요약과 유효기간 보존 | 모델·projection 비교 |
| 인증 없음/오류/권한 없는 scope | 401/403, 저장소 무변경 | 실제 HTTP·저장소 검사 |
| 동시 merge / 재시도 / 서비스 재시작 | 갱신 유실 없음, receipt 보존 | 저장소 CAS·영속성 검사 |
| 실제 monorepo 양성 변경 | 정확한 SHA·sparse 경로·focused dependency closure와 모든 assignment 실행 | 실제 checkout/install/run fixture |
| 변경 없음 / 실패·취소·필수 실행 생략 | 정상 생략 또는 aggregate 실패 | CLI·Action·hosted fixture |
| 각 지원 OS/arch의 패키지 설치 | 기존 binary 이름·archive·checksum·npm 실행 유지 | 플랫폼 build·smoke와 공개 릴리즈 |
| 최신 공개 Go 릴리즈 | Nanoom과 nanoom-fixtures의 CI/aggregate 통과 | 버전·소스 SHA·run URL 기록 |

## 전환 및 복구

기존 Rust 운영 경로는 Go의 해당 사용자 경로가 검증되기 전까지 제거하지 않는다. Go CLI가 내부에서 Rust binary를 호출하는 방식은 마이그레이션으로 인정하지 않는다. 기존 미커밋 사용자 변경은 보존한다. 기존 릴리즈 artifact를 덮어쓰지 않는다.

서버는 **Cloudflare Workers + D1 유지가 필수**다(사용자 선택). Go/Wasm과 D1 binding으로 구현하며, 일반 HTTP 서버/컨테이너로 배포 환경을 바꾸지 않는다. `github.com/syumai/workers-go@v0.36.0`의 Worker HTTP·cron·D1 adapter를 사용한다. 해당 라이브러리는 실험 단계임을 명시하고 있으므로 실제 Cloudflare 실행의 CPU·메모리 예산은 배포 수용 기준으로 남긴다. 생성 JavaScript는 Wasm 런타임 연결이며 인증·이력·예측·SQL 로직은 Go다.

검증되지 않은 Go 구현으로 운영 `@latest` Action을 대체하지 않는다. 로컬 검증, producer CI, 릴리즈, fixture matrix, aggregate status는 각각 별도 증거로 남긴다.

## 구현된 후보 경로

| 경로 | 상태 |
| --- | --- |
| `internal/prediction` | v3 scope/profile/key digest, measurement compile, model validate/apply/prune/replay, profile별 projection, artifact bundle |
| `internal/history`, `cmd/history-worker` | 기존 인증 JSON, HTTP snapshot/merge/ETag/errors, D1 CAS·checksum·scope 검증, scheduled retention |
| `internal/plan` | 기존 Plan/reference 검증, assignment 선택과 출력, sparse pattern 생성 |
| `internal/workspace` | 설정 자동 탐색, manifest discovery, 내부 dependency closure, root 도구와 Yarn asset 경로, 위상 순서 |
| `cmd/nanoom`, `internal/cli` | `affected`, `run`, `install`, `history`, `plan select`, `status`, `cache-key`, `schema`, `version` 전체 명령 |
| `internal/affected` | Git 비교·shallow fetch·영향 범위·dependency 전파·분배·preparation 비용·runner별 prediction과 authoritative Plan 생산 |
| 빌드·개발 검사·npm wrapper | Go 빌드·race/vet·version 동기화·기존 5개 archive 계약 연결 |
| 운영 Worker / 공개 release | 운영 Rust 유지. Go Worker version upload와 release workflow 제안은 별도 검증 |

Workspace 발견은 `.gitignore`·`.ignore`·전역 Git ignore·중첩 working directory·숨김 경로·manifest symlink를 검사한다. 디렉터리 symlink는 따라가지 않는다. Windows `.cmd`는 인수 escaping과 신뢰된 `cmd.exe` 경로를 사용하며, native Windows 실행 검사는 해당 OS의 Go 테스트에 포함한다. 로컬 macOS의 cross build는 Windows 실행 증거를 대신하지 않는다.

## 로컬 검증 (2026-10-18)

개발 배포 명령은 저장소 루트에서 `npx --yes wrangler@4.138.0 deploy --config crates/history-worker/wrangler.jsonc`다. 기본 config는 Go 빌드이고, 전환 중 rollback reference는 `wrangler.rust.jsonc`에 둔다. 운영 traffic은 검증 후 전환한다.

환경: macOS arm64, Go 1.27.1, Node 24.18.0, Wrangler 4.138.0, workerd의 임시 로컬 D1. 운영 D1에는 쓰지 않았다.

```sh
go test -race ./...
go vet ./...
bash scripts/go-cli-contract-test.sh
NANOOM_TEST_BINARY="$PWD/dist/nanoom" HISTORY_E2E_WORKER_LANGUAGE=go bash scripts/history-server-e2e-test.sh
npx --yes wrangler@4.138.0 deploy --dry-run \
  --config crates/history-worker/wrangler.jsonc \
  --outdir /tmp/nanoom-go-worker-dry-run
```

- `go test -race ./...`, `go vet ./...` 통과. 인증 권한 분리·잘못된 credential·잘못된 JSON/Unicode·Plan/reference 위변조와 거부 시 무출력·설정과 closure 등을 검사했다.
- Rust oracle의 12개 시간순/지연 도착/profile 시나리오에서 canonical batch digest·모델·예측·pruning 수가 일치했다. 모델 입력을 변경하지 않고 새 state를 반환하며 duplicate는 무변경, body conflict는 거부했다. corpus 생성 소스는 `examples/go_prediction_oracle.rs`, 고정 결과는 `internal/prediction/testdata/rust-v3.json`이다.
- `go-cli-contract-test.sh`는 실제 Rust/Go binary의 version·네 status 결과·아홉 cache input·history 파일 바이트·잘못된 measurement의 degraded 카운터를 비교한다. Rust가 만든 Plan을 양쪽 binary로 선택한 assignment JSON과 paths.txt도 일치했다. 실제 npm focused install에서 app/shared/root tools가 설치되고 unrelated workspace가 제외됐다.
- 기존 `history-server-e2e-test.sh`를 Go Worker 모드로 실행했다. HTTP 오류 14개, cold fallback → 실제 assignment 2개 실행 → D1 merge → duplicate no-op → warm sample 8개 재사용과 warm run, 1,900,000-byte row 경계, checksum/schema/scope 손상 거부와 복구, 12개 동시 batch와 duplicate retry의 정확히 한 번 적용, 46일 row 삭제와 44일 row 보존이 통과했다. `NANOOM_TEST_BINARY`로 Go CLI를 지정한 전체 Go CLI/Worker E2E도 통과했다. 공개 릴리즈 소비 증거는 아니다.
- 후보 버전은 0.10.0이며 Go build, npm manifests, Yarn lockfile과 reference Cargo version을 동기화했다. Go CLI는 Linux amd64/arm64, macOS amd64/arm64, Windows amd64의 CGO 없는 교차 빌드가 통과했다. 실행 검증은 macOS arm64이며 다른 OS의 runtime 검증을 대신하지 않는다.
- Wrangler dry-run은 gzip 2,248.97 KiB를 보고했다. 크기 검사는 CPU·메모리·공개 배포 성공을 증명하지 않는다.

- 실제 Rust/Go `affected`의 양성/변경 없음 JSON, compact matrix, authoritative Plan 내용과 reference digest가 일치했다. scheduler의 cold/warm/preparation/fallback/shards/labels/expired 7개 고정 시나리오도 일치했다.
- 기존 Rust test harness를 Go binary로 실행한 CLI 36개, install 4개, Plan 10개가 통과했다. runner environment suite 14개도 통과했지만 일부는 Rust library를 직접 호출하므로 14개 전체를 Go 증거로 세지 않는다.
- `NANOOM_TEST_BINARY="$PWD/dist/nanoom" bash scripts/action-contract.sh` 전체 통과. 실제 sparse checkout, nested working directory, focused install, 실행, history, revision, 실패 집계, cleanup을 포함한다. 후보 source Action 검사이며 공개 `@latest` 소비 증거는 아니다.
- 5개 플랫폼 Go binary를 실제 archive로 만들고 `scripts/release-smoke.sh`와 npm wrapper·platform package·version 검사를 통과했다. 로컬 archive는 새 공개 릴리즈가 아니다.

## 공개 전환 gate

1. [release workflow 전체 diff](workflows/release.patch)와 [적용 후 파일](workflows/release.yml.proposed)을 준비했다. trigger·권한·check 영향은 [제안 설명](workflows/README.md)에 기록했다. 사용자의 후속 진행 지시에 따라 제시한 diff를 반영했다. 운영 CI workflow는 변경하지 않았다.
2. Go Worker는 운영 트래픽을 전환하지 않고 Cloudflare version으로 업로드했다. Version ID `b6bf087c-7d0c-426a-9b21-af459d04d23e`, gzip 2,248.96 KiB, Worker startup 11 ms. Startup은 요청 CPU 시간이 아니다. version URL에서 `/health` 200, D1 `/ready` 200, 인증 없는 snapshot 401, 없는 경로 404를 확인했다. HTTP 기본 경로와 실사용 payload의 CPU·메모리 검증을 구분한다.
3. producer CI, 새 immutable release, 최신 공개 Go binary를 사용하는 두 저장소 CI, 양성 fixture matrix·변경 없음·실패/취소/필수 생략 aggregate 증거가 필요하다.
4. 공개 Go 경로 검증 후 기존 Rust 운영 구현·toolchain·비교 전용 harness 제거를 마무리한다. 현재 Rust reference 소스가 남아 있으므로 전체 기술 전환 완료를 선언하지 않는다.

현재 검토는 **BLOCKED**다. 위 공개 실행 gate와 native 플랫폼 검증이 남아 있다.

## Hosted 검증 및 남은 배포 판단

- 독립 branch `codex/go-runtime-migration`, [PR #131](https://github.com/XionWCFM/nanoom/pull/131)에서 검증한다. 원래 checkout의 별도 테스트 품질 변경은 포함하지 않았다. E2E의 HTTP·merge·storage·contention 검사는 Go 이식 검증에 필요한 계약으로 함께 가져왔다.
- `scripts/review-change.sh origin/main`이 실제 commit diff를 검토하고 PASS했다. 전체 수용 검토의 완료 판정과 구분한다.
- [producer CI 39123629507](https://github.com/XionWCFM/nanoom/actions/runs/39123629507), source `02b99963f3c6a3aabd0bb1613e4a45d3eac38aec`: Linux·macOS·Windows와 aggregate status 모두 성공. 운영 Action/binary는 최신 공개 v0.9.0을 사용하고 Go 소스가 검증 대상이다.
- macOS의 Go 미설치를 공식 setup-go로 해결했다. 사용자가 CI diff와 Nanoom 자체 Go 개발 CI의 언어 도구 예외를 명시적으로 승인했다. Windows에서는 Turbo의 Go cache 환경 보존과 실제 quote/percent/metacharacter 인수 전달을 수정·검증했다.
- CLI의 명령별 help, `help plan select`, verbose 반복과 붙여 쓴 짧은 cwd/config option도 유지한다. help 텍스트는 기존 공개 CLI 출력에서 고정하고 Go binary에 embed한다.
- 공개 임시 Worker + 별도 임시 D1에서 실제 Go CLI가 만든 batch 1,073 bytes/aggregate 4개를 검사했다. 최초 merge 적용, duplicate no-op, warm snapshot, ETag 304, 잘못된 digest 409 및 모델 무변경이 통과했다. [비밀값을 제거한 HTTP/CPU 기록](evidence/remote-go-worker.json)에 요청 헤더와 token은 저장하지 않는다.
- 표준 Go/Wasm의 작은 요청 CPU는 10~157ms로 관측됐다. [Cloudflare 공식 한도](https://developers.cloudflare.com/workers/platform/limits/)의 Free 10ms를 보장하지 못한다. 현재 OAuth의 subscription API가 403이어서 요금제는 확인되지 않았다. 요금제 변경은 하지 않았고 운영 전환 전에 사용자 확인이 필요하다. TinyGo 0.43.0 실험은 압축 763.74KiB까지 줄었지만 초기화 panic으로 HTTP가 실패하여 채택하지 않았다.
- 운영 D1을 읽기만 하여 현재 모델 1개(최대 3,218 bytes)의 strict schema, scope storage key, checksum, Go canonical bytes의 완전 일치, prediction projection을 검증했다. 운영 데이터에 쓴 row는 0개다. 기존 Rust 모델 corpus도 별도로 검증한다.

공개 Go 릴리즈·최신 공개 Go 제품의 nanoom-fixtures matrix/aggregate, 운영 Worker 전환과 최종 Rust 제거는 아직 완료되지 않았다.
