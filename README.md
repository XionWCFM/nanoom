# nanoom

`nanoom`은 변경된 JavaScript/TypeScript workspace와 transitive dependent를 찾고, 실행시간 이력을 이용해 GitHub Actions assignment를 만드는 CLI입니다. Nx/Turbo/Yarn/pnpm의 로그 형식을 해석하지 않고 실제 subprocess wall time을 공통 Rust 경계에서 측정합니다.

## 설치와 설정

```bash
npm install --save-dev @nanoom/cli
```

```json
{
  "$schema": "./nanoom.schema.json",
  "group": {
    "ci": {
      "tasks": ["lint", "test", "build"],
      "distribution": {
        "small":  { "maxAffectedPercent": 25,  "concurrency": 3 },
        "medium": { "maxAffectedPercent": 60,  "concurrency": 6 },
        "full":   { "maxAffectedPercent": 100, "concurrency": 12 }
      },
      "rules": [
        { "name": "@repo/e2e", "shard": [{ "task": "test", "shard": 2 }] }
      ]
    }
  },
  "globalDependencies": ["yarn.lock", "tsconfig.json"]
}
```

`affectedPercent = affected workspace 수 / 전체 발견 workspace 수 * 100`입니다. 경계는 inclusive이고 `small`, `medium`, `full` 중 처음 일치하는 tier를 선택합니다. 세 tier와 오름차순 임계값이 필수이며 `full.maxAffectedPercent`는 정확히 `100`, `concurrency`는 1 이상입니다.

`concurrency`는 Nanoom assignment 수의 상한이며 GitHub `strategy.max-parallel`이 아닙니다. 실제 assignment 수는 `min(concurrency, work item 수)`입니다. `distribution`이 없으면 v0.2와 같이 work item 하나당 matrix entry 하나를 냅니다. 실질적 격리를 보장하지 못했던 `isolate`는 v0.3.0에서 제거했습니다. 독립 작업은 shard 또는 별도 group으로 표현합니다.

```text
nanoom affected --base <revision> [--head <revision>] [--history <json>] [--json]
nanoom run <group> <task> [--filter <workspace>] [--all]
           [--shard N --total-shards N] [--continue-on-error] [--json]
nanoom install [--package-manager auto|pnpm|yarn|npm] [--filter <workspace>]...
nanoom history --input <sample-or-history.json>... --output <history.json>
nanoom status <job,...> --results job=status,... [--json]
nanoom schema [--output <file>]
```

`run --json`은 성공 실행마다 `workspace`, 실제 `runner`, `durationMs`를 냅니다. 첫 실패 뒤에는 새 작업을 시작하지 않고 `completed`, `failed`, `pending`을 남깁니다. `install`은 assignment의 workspace union을 한 번에 focused install할 수 있습니다.

## 실행시간 기반 정적 배치

`scheduler: artifact`는 최근 성공 history artifact를 best-effort로 읽습니다. exact key는 `group/workspace/task/shard/runner/environment`이고 최근 성공 7개의 median을 사용합니다. exact sample이 없으면 같은 group median, 그것도 없으면 가중치 `1`입니다. 긴 work item부터 예상 합계가 가장 작은 bucket에 넣는 deterministic LPT를 사용합니다.

history가 없거나 손상됐거나 권한이 없으면 CI를 실패시키지 않고 deterministic equal-weight 배치로 폴백합니다. `result.scheduling.historyStatus`, `reason`, `historyDownloadMs`로 이유와 비용을 확인할 수 있습니다. 성공한 `run`만 sample artifact를 올리고 `history` Action이 다음 실행용 artifact로 병합합니다. 보관 기간은 30일입니다.

```yaml
- id: affected
  uses: XionWCFM/nanoom/.github/actions/affected@v0.3.0
  with:
    scheduler: artifact

- uses: XionWCFM/nanoom/.github/actions/install@v0.3.0
  with:
    matrix: ${{ toJSON(matrix) }}
    packageManager: pnpm

- uses: XionWCFM/nanoom/.github/actions/run@v0.3.0
  with:
    matrix: ${{ toJSON(matrix) }}
    group: ci
    scheduler: artifact

- uses: XionWCFM/nanoom/.github/actions/history@v0.3.0
  with:
    scheduler: artifact
```

artifact는 correctness/status 전달 수단이 아닙니다. aggregate job은 계속 `status` Action에 `${{ toJSON(needs) }}`만 넘겨 판정합니다.

## HTTP continuous assignment

`scheduler: http`는 Nanoom 서버를 배포하지 않고 HTTPS `/v1` client contract만 제공합니다. 인증은 로그나 config가 아니라 `NANOOM_COORDINATOR_TOKEN` bearer token으로만 전달합니다.

- `POST /v1/runs`: repository/run/group/workItems/tier/concurrency/environment 등록
- `POST /v1/runs/{runId}/claims`: worker의 atomic lease 요청
- `PATCH /v1/runs/{runId}/claims/{itemId}`: heartbeat, success+duration, failure
- `POST /v1/runs/{runId}/complete`: 최종 완료

모든 변경 요청은 `Idempotency-Key`를 사용합니다. agent는 30초 heartbeat를 보내고 빈 claim이 올 때까지 다음 work item을 받습니다. 이미 시작된 HTTP run에서 coordinator 장애가 나면 중복 실행을 피하기 위해 job을 실패시킵니다. lease 만료 1회 재할당과 두 번째 만료 시 run failure 확정은 coordinator가 구현해야 하는 계약입니다.

## 경계와 검증

`affected` Action이 GitHub event를 explicit `--base`/`--head`로 변환하고 CLI는 플랫폼 독립적으로 계산합니다. `status`는 timing/history/coordinator를 해석하지 않고 `needs`만 집계합니다. Task DAG, remote task cache, flaky retry, agent type routing, Nx assignment rules와 공식 SaaS/server는 v0.3.0 범위가 아닙니다.

설정 schema는 `nanoom schema --output nanoom.schema.json`으로 생성합니다. 결정과 fixture acceptance 기준은 [ADR-0009](docs/adr/0009-runtime-aware-distribution.md)에 기록되어 있습니다.

## License

MIT
