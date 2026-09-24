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
  "globalDependencies": ["yarn.lock", "tsconfig.json"],
  "workspace": { "include": ["packages/*", "tools/*"] },
  "affected": { "maxFetchDepth": 2048 },
  "checkout": { "always": ["scripts"] }
}
```

`affectedPercent = affected workspace 수 / 전체 발견 workspace 수 * 100`입니다. 경계는 inclusive이고 `small`, `medium`, `full` 중 처음 일치하는 tier를 선택합니다. 세 tier와 오름차순 임계값이 필수이며 `full.maxAffectedPercent`는 정확히 `100`, `concurrency`는 1 이상입니다.

`concurrency`는 Nanoom assignment 수의 상한이며 GitHub `strategy.max-parallel`이 아닙니다. 실제 assignment 수는 `min(concurrency, work item 수)`입니다. `distribution`이 없으면 v0.2와 같이 work item 하나당 matrix entry 하나를 냅니다. 실질적 격리를 보장하지 못했던 `isolate`는 v0.3.0에서 제거했습니다. 독립 작업은 shard 또는 별도 group으로 표현합니다.

```text
nanoom affected --base <revision> [--head <revision>] [--history <json>] [--json]
               [--plan-output <file> --plan-context <file>]
nanoom plan select --input <file> --reference <file> --group <name>
                   --assignment <id> --output-dir <directory>
nanoom run <group> <task> [--filter <workspace>] [--all]
           [--shard N --total-shards N] [--continue-on-error] [--json]
nanoom install [--package-manager auto|pnpm|yarn|npm] [--filter <workspace>]...
              [--filter-file <file>]
nanoom history --input <sample-or-history.json>... --output <history.json>
nanoom status <job,...> --results job=status,... [--json]
nanoom schema [--output <file>]
```

GitHub Actions의 `affected`는 명시적인 `base`/`head`가 없을 때 이벤트별 revision을
해석합니다. pull request와 merge queue는 대상 revision을 사용하고, `push`는 현재
workflow·branch의 마지막 성공한 push SHA부터 비교합니다. 따라서 main CI가 연속
실패해도 그 사이의 변경이 다음 실행에서 빠지지 않습니다. 최초 실행이거나
`actions: read` 권한이 없으면 Action은 축소 실행하지 않고 실패하므로, `actions: read`
와 `contents: read`를 부여하거나 `base: <commit>`을 명시해 bootstrap합니다.

`affected` Action의 canonical `result.revisionResolution`에는 `baseSource`, 실제
full SHA의 `baseCommit`/`headCommit`, push일 때 `successfulRunId`가 포함됩니다.
로그와 Step Summary에도 같은 값이 출력됩니다. CLI는 계속 명시적인
`nanoom affected --base ... --head ...`만 받아 플랫폼 독립적으로 동작합니다.

## Sparse checkout

공개 workflow는 `@latest`를 사용합니다. 릴리스가 검증된 뒤 `latest` Action tag가 이동하고,
setup은 GitHub 최신 Release의 실제 versioned asset과 checksum을 사용합니다.

`affected` job은 non-cone으로 root `package.json`, `nanoom.config.json`, 그리고
`workspace.include`에 해당하는 모든 workspace `package.json`만 checkout할 수 있습니다.
Nanoom은 Nx/Turbo 설정을 읽지 않고 이 manifest들로 graph를 만들며, 빠진 manifest가
있으면 불완전한 결과를 내지 않고 실패합니다. shallow history가 부족하면 tree와 blob 없이
32 → 128 → 512 → `affected.maxFetchDepth` 순서로만 가져옵니다.
configured workspace의 `package.json`이 삭제되거나 rename되면 현재 graph만으로 이전
dependency를 복원하지 않고, 남아 있는 workspace 전체를 보수적으로 affected 처리합니다.

각 matrix entry의 `checkout`은 affected workspace, 내부 dependency closure,
`checkout.always`의 합집합입니다. run job은 이를 cone mode에 그대로 전달합니다.
cone mode는 선택한 디렉터리와 root 파일을 함께 checkout하므로 lockfile과 root 설정은
별도 pattern이 필요 없습니다.

```yaml
# affected job
- uses: actions/checkout@v7
  with:
    fetch-depth: 1
    sparse-checkout-cone-mode: false
    sparse-checkout: |
      /package.json
      /nanoom.config.json
      /packages/*/package.json
      /tools/*/package.json

# run matrix job env: self-hosted runner의 이전 worktree와 격리
env:
  NANOOM_WORKDIR: .nanoom/${{ github.run_id }}-${{ github.run_attempt }}-${{ github.job }}-${{ strategy.job-index }}

- uses: actions/checkout@v7
  with:
    path: ${{ env.NANOOM_WORKDIR }}
    fetch-depth: 1
    sparse-checkout-cone-mode: ${{ matrix.checkout.coneMode }}
    sparse-checkout: ${{ matrix.checkout.sparseCheckout }}

- uses: XionWCFM/nanoom/.github/actions/run@latest
  with:
    cwd: ${{ env.NANOOM_WORKDIR }}
    matrix: ${{ toJSON(matrix) }}
    group: ci
    cleanupCheckout: true
```

`cleanupCheckout`은 명시적으로 켠 경우에만 동작하며, `cwd`가 `.nanoom/` 아래의
격리 경로가 아니면 삭제를 거부합니다.

`run --json`은 성공 실행마다 `workspace`, 실제 `runner`, `durationMs`를 냅니다. 명시한 `--all --filter`가 workspace를 찾지 못하면 실패하며, run Action도 계획된 workspace 실행이 없으면 assignment를 실패시키고 후속 item을 시작하지 않습니다. 첫 작업 실패 뒤에는 `completed`, `failed`, `pending`을 남깁니다. static assignment의 빈 install은 거부합니다. `install`은 assignment의 workspace union을 한 번에 focused install하며, standalone no-filter install과 continuous scheduler의 전체 install은 유지됩니다.

## 실행시간 기반 정적 배치

historical scheduler는 기본으로 켜져 있습니다. 같은 workflow와 branch의 마지막 성공 run에서 history를 읽고, exact key `group/workspace/task/shard/totalShards/runner/environment`의 최근 성공 7개 median을 사용합니다. exact sample이 없으면 workspace를 제외한 동일 group/task/shard layout/runner/environment의 median을 사용하고, 그것도 없으면 가중치 `1`로 시작합니다.

배치는 예상 runtime makespan을 먼저 최소화합니다. runtime이 같은 후보에서는 모든 assignment의 sparse checkout path 수 합계가 가장 작은 bucket을 선택해 중복 checkout을 줄입니다. `result.scheduling`의 `historyStatus`, `historySourceRunId`, `predictionSources`, `totalCheckoutPathCount`, `uniqueCheckoutPathCount`, `duplicatedCheckoutPathCount`로 근거를 확인할 수 있습니다.

group 또는 distribution tier의 `runnerLabels`로 matrix job의 runner를 정할 수 있습니다. 배열은 fallback 순서가 아니라 모든 라벨을 만족해야 하는 AND 조건입니다. tier 설정이 group 설정을 덮어쓰며, 생략하면 workflow의 `ubuntu-latest` fallback을 사용합니다.

```json
{
  "group": {
    "ci": {
      "tasks": ["test"],
      "runnerLabels": ["self-hosted", "linux", "large"],
      "timingEnvironment": "linux-large-image-v3",
      "distribution": {
        "small": { "maxAffectedPercent": 25, "concurrency": 3 },
        "medium": { "maxAffectedPercent": 60, "concurrency": 12 },
        "full": {
          "maxAffectedPercent": 100,
          "concurrency": 24,
          "runnerLabels": ["self-hosted", "linux", "xlarge"],
          "timingEnvironment": "linux-xlarge-image-v3"
        }
      }
    }
  }
}
```

```yaml
strategy:
  matrix: ${{ fromJSON(needs.affected.outputs.groups).ci.matrix }}
runs-on: ${{ matrix.runnerLabels || 'ubuntu-latest' }}
```

`timingEnvironment`을 생략하면 정렬된 runner label 배열로 안정적인 history identity를 만듭니다. 성능이 다른 runner가 같은 라벨 집합을 공유하는 autoscaled pool에서는 image/pool revision을 명시하세요. PR이 수정할 수 있는 config로 privileged self-hosted runner를 선택하면 신뢰되지 않은 코드를 그 runner에서 실행할 수 있으므로, fork PR은 고정 hosted runner 또는 격리된 pool만 사용하고 동적 label routing은 trusted push/`workflow_dispatch`에 제한하세요.

첫 실행은 `bootstrap-fallback` cold scheduling으로 정상 실행됩니다. 성공한 `run`만 sample artifact를 올리고 표준 `history` job이 다음 실행용 artifact로 병합합니다. 이전 성공 run에 sample만 있고 merged history가 없으면 history job 누락으로 실패합니다. historical scheduling이 필요 없는 경우에만 affected/run/history 모두 `scheduler: off`를 명시합니다.

## Plan v1 파일 CLI

Plan v1 producer는 상세 계획을 파일에 저장하고 작은 reference/matrix JSON만 stdout에 출력합니다. context 파일에는 repository, workflow, run ID, producer attempt, planning job, 비교한 전체 base/head SHA, 실행 tool을 넣습니다.

```bash
nanoom affected --base "$BASE_SHA" --head "$HEAD_SHA" \
  --plan-context plan-context.json --plan-output plan-v1.json > affected-result.json
jq -c '.plan' affected-result.json > plan-reference.json
nanoom plan select --input plan-v1.json --reference plan-reference.json \
  --group ci --assignment ci-0001 --output-dir selected
```

`selected/assignment.json`에는 검증된 assignment context가, `selected/paths.txt`에는 sparse checkout 경로가 기록됩니다. selector는 계획 파일의 raw bytes SHA-256, schema, repository/workflow/run/head를 검증합니다. 재실행은 같은 run의 이전 producer attempt(`producerAttempt <= current.attempt`)만 재사용할 수 있으며 reference의 `current` identity는 실행 중인 caller가 제공해야 합니다. 각 group의 compact matrix는 최대 256 assignment, 전체 결과는 UTF-16 인코딩 1 MiB 이하입니다. 이를 넘으면 group과 이유를 출력하고 실패합니다. no-change는 assignment 0개인 정상 Plan입니다. 기존 `affected --json` 상세 report와 bounded Plan output은 함께 요청할 수 없습니다. Artifact 업로드와 checkout/install/run 연결은 별도 후속 단계입니다.

Planned install은 `nanoom install --filter-file FILE`로 non-empty JSON string array를 전달할 수 있습니다. 상대 경로는 working directory 기준입니다. JSON이 아니거나 배열이 아닌 값, 문자열이 아닌 값, 빈 문자열, 제어문자는 package manager를 실행하기 전에 오류가 됩니다. 기존 `--filter`와 동시 사용도 거부합니다. 두 옵션을 생략한 standalone `nanoom install`은 계속 root 전체 설치를 수행합니다.

```yaml
- id: affected
  uses: XionWCFM/nanoom/.github/actions/affected@latest

- uses: XionWCFM/nanoom/.github/actions/install@latest
  with:
    matrix: ${{ toJSON(matrix) }}
    packageManager: pnpm

- uses: XionWCFM/nanoom/.github/actions/run@latest
  with:
    matrix: ${{ toJSON(matrix) }}
    group: ci

- uses: XionWCFM/nanoom/.github/actions/history@latest
```

history job은 run 성공 뒤 실행하고 aggregate `status`의 dependency에 포함합니다. 작은 workflow는 `${{ toJSON(needs) }}`를 그대로 전달할 수 있습니다. 대규모 matrix에서는 outputs까지 포함한 JSON이 runner process 한도를 넘을 수 있으므로 `results`에 필요한 job 결과만 `job=${{ needs.job.result }}` 형식으로 전달합니다.

기본 `run`과 `history`는 GitHub.com용 `actions/upload-artifact@v4.6.2`를 사용합니다. GHES에서는 같은 입력과 결과 계약을 공유하는 `run-ghes`와 `history-ghes`가 Node 24 보안 백포트 `actions/upload-artifact@v3.2.2`를 사용합니다. composite Action의 `uses:`는 파라미터화할 수 없고 조건부 step도 사전 다운로드되므로 진입점을 분리했으며 서버를 자동 감지하지 않습니다. v3는 Actions Runner `2.327.1` 이상이 필요합니다. GitHub-hosted timing environment는 OS/architecture, self-hosted는 OS/architecture/runner name으로 분리됩니다. autoscaled pool은 안정적인 pool 또는 image revision을 `timingEnvironment`로 지정하세요.

```yaml
# GHES only; GitHub.com은 기본 run/history를 그대로 사용합니다.
- uses: XionWCFM/nanoom/.github/actions/run-ghes@latest
  with:
    matrix: ${{ toJSON(matrix) }}
    group: ci

- uses: XionWCFM/nanoom/.github/actions/history-ghes@latest
```

## HTTP continuous assignment

`scheduler: http`는 Nanoom 서버를 배포하지 않고 HTTPS `/v1` client contract만 제공합니다. 인증은 로그나 config가 아니라 `NANOOM_COORDINATOR_TOKEN` bearer token으로만 전달합니다.

- `POST /v1/runs`: repository/run/group/workItems/tier/concurrency/environment 등록
- `POST /v1/runs/{runId}/claims`: worker의 atomic lease 요청
- `PATCH /v1/runs/{runId}/claims/{itemId}`: heartbeat, success+duration, failure
- `POST /v1/runs/{runId}/complete`: 최종 완료

모든 변경 요청은 `Idempotency-Key`를 사용합니다. agent는 30초 heartbeat를 보내고 빈 claim이 올 때까지 다음 work item을 받습니다. 이미 시작된 HTTP run에서 coordinator 장애가 나면 중복 실행을 피하기 위해 job을 실패시킵니다. lease 만료 1회 재할당과 두 번째 만료 시 run failure 확정은 coordinator가 구현해야 하는 계약입니다.

## 경계와 검증

`affected` Action이 GitHub event를 explicit `--base`/`--head`로 변환하고 CLI는 플랫폼 독립적으로 계산합니다. `status`는 timing/history/coordinator를 해석하지 않고 `needs`만 집계합니다. Task DAG, remote task cache, flaky retry, agent type routing, Nx assignment rules와 공식 SaaS/server는 v0.3.0 범위가 아닙니다.

설정 schema는 `nanoom schema --output nanoom.schema.json`으로 생성합니다. v0.5.0의 GHES history와 checkout-cost 결정은 [ADR-0012](docs/adr/0012-ghes-history-checkout-cost.md)에 기록되어 있습니다.

## License

MIT
