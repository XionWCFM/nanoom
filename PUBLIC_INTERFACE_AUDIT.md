# Nanoom 공개 인터페이스·UX·기본값 감사

- 기준 커밋: `93884a7d1673bf8a2a1fcb78a131cbb5823c6194` (`main`)
- 감사일: 2026-08-26
- 범위: CLI, `nanoom.config.json`, composite Actions, npm wrapper/release, 영·한 문서와 예제, fixture, 기여자 경로
- 정책: v0.2.x의 잘못된 계약은 다음 수정 릴리스에서 호환 alias나 deprecation 기간 없이 제거한다. `affected`의 유일한 기계 출력은 canonical report를 쓰는 `--json`이다. 이 문서는 코드 변경이 아닌 다음 수정 세션의 단일 백로그다.

## 요약

Nanoom의 핵심 방향은 좋다. 명시적 revision, 순수 JSON stdout, Action 경계의 GitHub 문맥 해석, checksum 검증, canonical result, 그리고 released consumer fixture를 완료 조건으로 둔 점은 유지해야 한다. 그러나 실제 공개 계약은 문서·Action·CLI 사이에서 이미 갈라져 있고, 일부 기본값은 사용자에게 실패하거나 잘못된 성공을 보인다.

| 영역 | 평가 | 확정 문제 | 다음 세션의 우선순위 |
| --- | --- | ---: | --- |
| 설치·릴리스 | C | 2 | P1 |
| CLI·설정 | C | 4 | P1 |
| GitHub Actions | C | 3 | P1 |
| 문서·예제 | D | 2 | P1 |
| 실제 소비자·기여자 | C | 1 | P1 |

가장 위험한 여정은 **문서의 오래된 Action 태그를 복사한 소비자가 현재 인터페이스·바이너리와 다른 릴리스를 실행하고, `status` 또는 matrix 출력이 커질 때 원인을 재현하기 어려운 상태**다. 이 문서는 P1 7건, P2 3건, P3 1건을 기록한다. P0는 없다.

## 방법과 비교 기준

모든 판정은 구현, `--help`, 공개 문서, Action metadata, 테스트 및 fixture 경로를 교차 확인한 결과다. 아래 기준은 기능을 베끼기 위한 것이 아니라 Nanoom의 작은 정적 바이너리/Action 모델에 적용되는 계약 원칙을 확인하기 위한 것이다.

| 기준 | Nanoom에 적용한 원칙 |
| --- | --- |
| [Nx affected](https://nx.dev/docs/features/ci-features/affected) | CI 비교 revision을 분명히 하고, affected 선택을 preview·설명 가능하게 유지한다. |
| [Turborepo run](https://turborepo.dev/docs/reference/run) · [CI](https://turborepo.dev/docs/crafting-your-repository/constructing-ci) | Git 기반 filter는 필요한 history와 함께 명시하며, 실행과 preview의 계약을 하나로 유지한다. |
| [GitHub Action metadata](https://docs.github.com/en/actions/reference/workflows-and-actions/metadata-syntax) | 모든 Action input/output은 설명과 안정된 크기·형식을 가진다. job output은 1 MB 제한이다. |
| [GitHub CLI JSON formatting](https://cli.github.com/manual/gh_help_formatting) | 사람용 기본 출력과 한 가지 기계용 JSON 경로를 분리한다. |
| [Biome installation](https://biomejs.dev/guides/getting-started/) · [manual install](https://biomejs.dev/guides/manual-installation/) | 설치 방법·지원 플랫폼·버전 고정을 정확히 문서화하고, 설치 도구의 숨은 전제조건을 만들지 않는다. |

원격 캐시, daemon, watch mode는 Nanoom이 의도적으로 제공하지 않으므로 결함으로 세지 않았다. 각 P1 수정은 focused regression, public-boundary test, 정상·오류 문서 예제, 그리고 해당하는 경우 release tag를 이용한 non-skipped `nanoom-fixtures` run을 모두 요구한다.

## Keep: 수정 중 보존할 계약

- CLI의 GitHub 비종속성: `affected` CLI는 `--base`/`--head`를 받고, GitHub event 해석은 Action 경계가 맡는다.
- JSON 모드의 stdout은 JSON 한 문서만 쓰고, 진단과 하위 프로세스 출력은 stderr로 보낸다.
- install/run은 실제 subprocess command와 cwd를 출력하며 Action도 이를 canonical result와 함께 노출한다.
- 릴리스 binary/archive의 checksum 검증 및 `action` 값이 Action ref에서 release version을 유도하는 방식.
- 완료 판단은 coverage가 아니라 producer → released Action/binary → fixture matrix → aggregate `status` 흐름으로 한다.

## 수정 백로그

### AUD-001 — 모든 공개 문서가 오래된 `v0.2.6`으로 소비자를 고정한다

- 심각도/순서: P1 · Wave 1
- 영향: README, 영·한 설치·Action reference·how-to를 복사하는 모든 사용자
- 근거: 현재 package/Cargo 버전은 `0.2.9`인데 README와 docs는 `@v0.2.6` release URL과 Action ref를 반복한다. `rg 'v0\\.2\\.6' README.md docs/content/docs`로 재현한다.
- 실패: 사용자는 현재 문서가 현재 binary/Action 계약이라고 믿지만 이전 릴리스를 설치한다. 이후의 output, input, 진단, 보안 수정이 달라도 문제를 현재 코드에서 재현할 수 없다.
- 목표 계약: 예제는 release-automation이 동기화한 현재 안정 태그만 사용한다. 버전을 문서에 하드코딩해야 한다면 버전 동기화 검사가 모든 영·한 문서와 README를 검사한다.
- 제거: 개별 문서에 남은 수동 `v0.2.6` 값.
- 최소 변경: version-consistency 검사를 docs/README까지 확장하고, release bump가 모든 예제 태그를 갱신한다.
- 검증: stale-tag regression test, docs build, published tag의 `nanoom version`, 해당 tag를 쓰는 fixture의 non-skipped matrix 및 `status` 성공.

### AUD-002 — `version` 기본값이 실제 Action과 reference에서 다르다

- 심각도/순서: P1 · Wave 1
- 영향: `affected`, `install`, `run` Action의 재현성·업그레이드 모델
- 근거: `.github/actions/{affected,install,run}/action.yml`은 `version: {default: action}`이고 setup은 Action ref가 release tag가 아니면 실패한다. English reference는 기본값을 `latest`로 설명한다.
- 실패: 사용자가 reference에 따라 자동 최신 버전을 기대하거나 `latest`를 명시하면, Action ref와 binary가 달라져 재현성과 계약 호환성을 잃는다.
- 목표 계약: 기본값은 `action` 하나다. 이는 소비자가 pin한 Action tag와 정확히 같은 binary tag를 뜻한다. `latest`는 제거하고, 테스트용 `local`만 명시적으로 허용한다.
- 제거: `latest` value 및 문서의 `latest` 기본값 설명.
- 최소 변경: setup parser, metadata descriptions, 영·한 reference 및 action-contract tests를 같은 change에서 정렬한다.
- 검증: tag pin, branch pin 실패, `version: action`, `version: local`, 제거된 `latest` 각각의 Action contract test와 released fixture rerun.

### AUD-003 — 한 기능에 여러 JSON 스위치를 제공해 자동화 계약이 모호하다

- 심각도/순서: P2 · Wave 1
- 영향: `affected`, `status` CLI와 이를 스크립트로 소비하는 사용자
- 근거: `affected`는 `--json`, `--format json`, `--matrix`, `--report`를 동시에 제공하고, `status`는 `--json`과 `--format json`이 같은 pretty JSON을 출력한다. `nanoom status ci --results ci=success --format json`과 `--json`의 stdout이 동일함을 확인했다.
- 실패: 호출자는 어떤 JSON shape가 안정 계약인지 알 수 없고, flags 조합의 precedence를 추론해야 한다.
- 목표 계약: `affected`는 사람이 읽는 text 기본 출력과, `{affected, matrix}` canonical report를 출력하는 `--json`만 지원한다. `status`는 사람이 읽는 text 기본 출력과 `--json`만 지원한다. Action은 `affected --json`만 호출해 matrix를 읽는다.
- 제거: `affected --format`, `affected --matrix`, `affected --report`, `status --format`, 그리고 중복 JSON format. 같은 결과를 내는 aliases는 남기지 않는다.
- 최소 변경: Clap args, stdout/stderr tests, shell Action invocations, CLI reference, 영·한 examples를 함께 갱신한다.
- 검증: 도움말 snapshot, 각 정상/오류 경로의 JSON schema assertion, JSON stdout 단일 문서 assertion, released Action fixture.

### AUD-004 — CLI `status`가 cross-job 결과를 현재 step의 `GITHUB_OUTPUT`에서 읽는다고 주장한다

- 심각도/순서: P1 · Wave 1
- 영향: `nanoom status`를 CI aggregate 용도로 직접 쓰는 사용자
- 근거: `src/commands/status.rs`는 `--results`가 없으면 `GITHUB_OUTPUT` 파일에서 `<job>_result=...`를 검색한다. GitHub의 job output은 downstream job에서 `needs` context로 소비하며, `GITHUB_OUTPUT`은 현재 step output을 기록하는 파일이다. public `status` Action은 이미 `needs: ${{ toJSON(needs) }}`를 사용한다.
- 실패: job 간 result를 읽을 수 있다는 CLI 문서가 실제 GitHub data flow와 맞지 않아, aggregate command가 missing result로 실패하거나 임의의 현재-step 값만 읽는다.
- 목표 계약: CLI `status`는 명시적 `--results`만 받는 순수 local aggregator로 축소한다. GitHub job aggregate는 `needs`를 입력으로 받는 `status` Action만 지원한다.
- 제거: `GITHUB_OUTPUT` fallback, 이를 설명하는 docs/tests 및 cross-job status CLI 사용 예제.
- 최소 변경: status parser/validation, integration tests, reference/how-to를 변경하고 Action의 `needs` path는 그대로 유지한다.
- 검증: `--results` 누락은 행동 가능한 error, success/failure/cancelled/skipped table, Action needs JSON success/failure matrix, released fixture aggregate status.

### AUD-005 — `--all` 실행이 config의 `ignore` 규칙을 우회한다

- 심각도/순서: P1 · Wave 1
- 영향: `nanoom run <group> <task> --all` 사용자와 Action adapter
- 근거: `src/commands/run.rs`의 `--all` project filter는 shard/isolate/filter만 확인하고 `Rule.ignore`를 확인하지 않는다. 반면 affected matrix는 group rule의 ignore를 적용한다. 따라서 `ignore: true` package가 `--all`에서 실행될 수 있다.
- 실패: config가 “이 group에서 실행하지 않음”을 뜻한다고 믿는 사용자가 full run에서 금지한 legacy/e2e package를 실행한다. affected와 full run이 서로 다른 task 집합을 약속한다.
- 목표 계약: group의 `ignore`는 affected와 `--all` 모두에서 절대 제외다. full run은 affected restriction만 풀며 group policy는 풀지 않는다.
- 제거: `--all`이 ignore를 우회한다는 암묵적 동작.
- 최소 변경: shared project-selection predicate 하나를 두 경로에서 재사용하고, advanced example의 ignored workspace를 회귀 fixture로 사용한다.
- 검증: ignore package의 direct change, `--all`, filter/shard/isolate 조합 모두에서 zero execution; non-ignored package는 실행되는 CLI regression.

### AUD-006 — Action output이 GitHub의 1 MB job 제한을 넘을 수 있는데 사전 검증이 없다

- 심각도/순서: P1 · Wave 2
- 영향: 큰 monorepo의 `affected` Action consumer
- 근거: affected Action은 dynamic matrix를 `groups` output으로 기록한다. GitHub composite output은 job당 1 MB 제한이다.
- 실패: workspace/path/reason 수가 커지면 matrix가 필요한 정상 CI가 output-file 제한으로 실패하며, 사용자는 입력 크기나 대안을 알 수 없다.
- 측정 및 결정: 실제 output key/value와 UTF-16 계산으로, 이전 entry shape(`group`, `label`, `path` 반복)는 4,122 entries(1,048,512 bytes)까지이고 4,123에서 초과한다. compact entry shape(`name`, `task`, 필요한 shard/isolate만)은 9,250 entries(1,048,570 bytes)까지이고 9,251에서 초과한다. 200 / 1,000 / 5,000 entries는 각각 50,462 / 251,484 / 1,272,644 bytes에서 23,302 / 113,924 / 567,084 bytes로 줄었다. 수백 workspace는 한도와 거리가 멀며, 압축/복호화 format은 도입하지 않는다.
- 목표 계약: dynamic matrix에 필요한 `groups`는 compact entry shape로 유지한다. group은 workflow 경계에서 한 번 `run.group`으로 전달하고, 중복 matrix를 포함하던 `result`는 revision, `hasChange`, group별 entry count만 담는 compact canonical summary로 바꾼다. `has_change`, `groups`, `result` key/value를 포함한 모든 emitted job output의 UTF-16 합계가 1 MB 한도를 넘으면, 어느 값도 쓰기 전에 total byte count, 가장 큰 group, group 분할을 안내하고 exit 2로 실패한다.
- 제거: 제한 없는 full report `result` output 약속과 output 한도보다 먼저 실패하지 않는 동작.
- 최소 변경: output shape/size guard, compact result schema, Action metadata/reference, near-limit contract test를 수정한다. 전체 diagnostics는 always-visible log 및 Step Summary에 남긴다.
- 검증: 200-entry representative matrix와 5,000-entry matrix pass, total output budget을 넘기는 deterministic error before any output write, existing small fixture output compatibility, released positive fixture.

### AUD-007 — Action metadata에 사용자용 input 설명이 없어 Marketplace/Workflow UI에서 계약을 알 수 없다

- 심각도/순서: P3 · Wave 2
- 영향: YAML editor와 Action metadata만 보고 설정하는 사용자
- 근거: public Action metadata의 `inputs`는 대부분 `{default: ...}` 또는 `{required: true}` 축약형이며 `description`이 없다. GitHub metadata는 input/output description을 지원하며, output은 이미 설명한다.
- 실패: `matrix`, `cwd`, `version`, `packageManager`, `monorepoTool`, `needs`의 JSON shape·기본값·보안 의미를 workflow 작성 위치에서 확인할 수 없다.
- 목표 계약: 모든 public input은 가능한 값, default 의미, JSON shape, failure effect를 한 문장 description으로 가진다. internal `_setup`은 공개 surface가 아니므로 제외한다.
- 제거: 설명 없는 public input metadata.
- 최소 변경: 네 action.yml과 action-contract script만 갱신하고 reference의 표와 동일한 vocabulary를 사용한다.
- 검증: script가 모든 public input description과 모든 output description을 요구하고, docs example의 inputs가 metadata와 대조된다.

### AUD-008 — npm wrapper의 release-download fallback은 문서에 없는 `curl`/`tar`를 요구한다

- 심각도/순서: P1 · Wave 2
- 영향: optional platform package가 생략된 npm 사용자, 특히 최소 Windows/CI image
- 근거: `packages/cli/bin/nanoom.js`는 optional package를 찾지 못하면 `curl`과 `tar`를 `spawnSync`한다. Installation은 wrapper requirement를 Node.js 18+라고만 설명한다.
- 실패: optional dependency가 생략된 정상 npm install이 runtime에서 host utility 누락 또는 archive extraction failure로 끝난다. 사용자는 재설치·직접 binary install 중 무엇을 해야 하는지 알 수 없다.
- 목표 계약: npm package는 optional platform package가 있으면 Node만으로 실행한다. 없으면 runtime download fallback을 제거하고, 지원 플랫폼·package manager와 정확한 reinstall/manual-install 복구 명령을 포함한 error를 낸다.
- 제거: 숨은 `curl`/`tar` 의존성 및 first-run network download.
- 최소 변경: wrapper/postinstall, package docs, platform-package smoke를 변경한다. direct binary install은 checksum 검증 경로로 계속 지원한다.
- 검증: optional package present, `--omit=optional`, unsupported platform, no-network CI 각각의 deterministic result; npm pack/install smoke와 published package smoke.

### AUD-009 — `--workspace-install`과 숨은 `--root-install` alias가 실제 동작을 반대로 암시한다

- 심각도/순서: P2 · Wave 2
- 영향: `nanoom install` CLI 사용자
- 근거: root install은 항상 실행되는데 `--workspace-install`은 추가 per-workspace install을 한다. 구현에는 undocumented `--root-install` alias가 이 동작을 가리킨다.
- 실패: 이름만 보고 root-only install을 기대한 사용자가 모든 workspace에서 legacy install을 실행해 시간·lockfile 변경·실패 지점을 늘린다.
- 목표 계약: 기본 root install과 focused `--filter`만 지원한다. legacy per-workspace install은 제거한다.
- 제거: `--workspace-install`, `--root-install` alias, 관련 docs/tests.
- 최소 변경: install args/branch 삭제, help/reference/example 정리, focused-install fixture 보존.
- 검증: help snapshot, root/focused Yarn·pnpm behavior, npm focused rejection, root tool+dependency closure assertion.

### AUD-010 — 영·한 문서의 동일 계약이 이미 서로 다른 세부사항을 보인다

- 심각도/순서: P2 · Wave 2
- 영향: 한국어 문서 사용자와 locale 전환 사용자
- 근거: English CLI reference는 `affected --report` diagnostics contract를 설명하지만 Korean reference에는 그 설명이 없다. 두 locale 모두 stale tag를 반복하지만 이후 변경이 한 쪽에만 반영될 위험이 현재 구조에 없다.
- 실패: 언어에 따라 사용 가능한 output·디버깅 정보가 달라지고, 한 언어의 수정이 다른 언어의 공개 계약을 조용히 깨뜨린다.
- 목표 계약: 영문 reference를 canonical source로 삼고, 한글 reference는 각 heading, option, default, example, output field를 1:1 대응시킨다. 번역의 자연스러운 문장만 달라질 수 있다.
- 제거: locale별 독립적인 계약 누락.
- 최소 변경: docs parity checker를 추가하고 all reference/how-to pages를 함께 갱신한다.
- 검증: page-pair heading/link/code-fence/flag/version parity test, docs build, reviewer checklist.

### AUD-011 — 지원한다고 말하는 pnpm/Nx 경로에 실제 Action E2E가 없다

- 심각도/순서: P1 · Wave 3
- 영향: pnpm 또는 Nx consumer
- 근거: CLI/Action은 pnpm, Yarn, Turbo, Nx runner를 공개하지만 representative hosted fixture는 Yarn Berry + Turbo다. 기존 producer argument tests는 package-manager install과 published Action matrix execution을 대체하지 못한다.
- 실패: public branch가 unit/argv test를 통과해도 pnpm focused install, Nx runner path, Action runner environment에서 실패할 수 있다.
- 목표 계약: Yarn Berry + Turbo fixture를 유지하고, 별도 pnpm + Nx representative consumer가 released tag에서 changed workspace → dependency closure install → run → aggregate status를 실행한다.
- 제거: argv/unit test만으로 runner/package-manager 지원을 완료로 판정하는 관행.
- 최소 변경: nanoom-fixtures에 pnpm/Nx path, producer CI trigger, completion verifier의 matrix evidence를 추가한다.
- 검증: direct leaf/global/no-change, root dev tool/transitive closure/unrelated exclusion, positive non-skipped matrix, published tag and aggregate `status` success.

## 수정 순서와 완료 정의

| Wave | 처리 | 선행 조건 | 완료 증거 |
| --- | --- | --- | --- |
| 1 | AUD-001~005: version/CLI/status/selection root contract | 없음 | focused tests, docs parity, action contract, local gate 두 번 |
| 2 | AUD-006~010: Action output/metadata, install, locale UX | Wave 1 API 확정 | producer CI, release smoke, published tag |
| 3 | AUD-011: pnpm+Nx consumer | Wave 1~2 release | released non-skipped fixture matrix와 aggregate status |

한 wave는 이전 wave의 공개 계약이 확정된 뒤에만 시작한다. 어떤 Action/배포 관련 수정도 source-tree test나 skipped matrix로 닫지 않는다. 최종 완료는 같은 immutable source commit에 대해 다음이 모두 성립할 때다.

1. 공개 command/action/config의 정상·빈·invalid·failure·recovery regression이 있다.
2. README와 영·한 docs, Action metadata, generated schema, help snapshot이 목표 계약과 일치한다.
3. `bash scripts/verify-completion.sh --local`이 깨끗한 checkout에서 두 번 통과한다.
4. producer CI와 release verification이 통과하고, package/archive/Action tag가 같은 version을 가리킨다.
5. Yarn/Turbo와 pnpm/Nx fixture가 모두 released public path에서 positive matrix entries, focused-install assertions, run jobs, aggregate `status`를 성공한다.

## 감사 커버리지 기록

| Surface | 결과 |
| --- | --- |
| `affected` | AUD-003, AUD-006; explicit revision과 reason diagnostics는 Keep |
| `run` | AUD-005; runner command/cwd diagnostics는 Keep |
| `install` | AUD-008, AUD-009; root/focused closure는 Keep |
| `status` | AUD-003, AUD-004; Action `needs` aggregation은 Keep |
| `schema`, `cache-key`, `version` | 이번 snapshot에서 public-contract defect 미확인; Wave 1의 help/JSON contract snapshot에 포함 |
| config/schema defaults | AUD-005, AUD-009; validation과 explicit workspace defaults는 Keep |
| affected/install/run/status Actions | AUD-001, AUD-002, AUD-006, AUD-007 |
| npm/platform/release | AUD-001, AUD-008 |
| English/Korean docs/examples | AUD-001, AUD-002, AUD-003, AUD-004, AUD-009, AUD-010 |
| hosted fixture/contributor completion | AUD-011; fixture-backed quality gate는 Keep |
