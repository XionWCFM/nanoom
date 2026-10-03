# Advanced example

`ci`와 `e2e` 두 그룹, 제외 규칙, task별 shard와 변경 규모별 assignment 상한을
보여주는 pnpm 저장소다. 모든 그룹은 하나의 기본 matrix에 포함된다.

| 설정 | 동작 |
| --- | --- |
| `ci.tasks` | test, build, typecheck 실행 |
| `ci.rules` | legacy-admin 제외, design-system test를 3개 shard로 생성 |
| `e2e.rules` | web의 test:e2e만 2개 shard로 생성; 해당 task가 없는 나머지는 제외 |
| `distribution` | small/medium/full 변경 비율에 따라 3/6/12 assignment 상한 |
| `workspace` | packages와 apps 발견; tools 제외 |
| `globalDependencies` | 기본 전역 입력 외에 workflow 파일 변경도 전체 실행 |

`concurrency`는 GitHub max-parallel이 아닌 Nanoom assignment 상한이다.
샤드 실행은 자식 프로세스에 NANOOM_SHARD_INDEX와 NANOOM_SHARD_TOTAL을 전달한다.
실제 테스트 분할은 task가 이 값을 읽어 구현해야 한다.

## 직접 실행

이 디렉터리를 별도 위치로 복사하고 실행한다.

```bash
example_dir=$(mktemp -d)
cp -R examples/advanced/. "$example_dir"
cd "$example_dir"
pnpm install --frozen-lockfile
git init -b main
git add .
git commit -m "example baseline"
git switch -c example/change
printf 'changed\n' > packages/design-system/change.txt
git add .
git commit -m "change design system"
nanoom affected --base main --head HEAD --json
nanoom run ci test --base main --head HEAD
nanoom run ci test --all --filter @adv/design-system --shard 1 --total-shards 3
nanoom run e2e test:e2e --all --filter @adv/web --shard 1 --total-shards 2
```

공유 design-system 변경은 ci의 design-system과 web, e2e의 web을 선택한다.
mobile 변경은 ci의 mobile만 선택한다. legacy-admin은 두 그룹에서 제외한다.

## GitHub Actions

이 예제를 별도 저장소의 루트로 사용할 때 아래 템플릿을 연결한다.
Nx/Turbo를 도입해도 각 도구의 설정을 저장소에 선언하면 자동 판별하며,
일반적인 사용에서 timingRunner·monorepoTool·packageManager를 반복 지정하지 않는다.

```yaml
name: CI

on:
  pull_request:
  merge_group:
  push:
    branches: [main]

permissions:
  contents: read
  actions: read

jobs:
  affected:
    name: Plan affected work
    runs-on: ubuntu-latest
    outputs:
      has_change: ${{ steps.affected.outputs.has_change }}
      plan: ${{ steps.affected.outputs.plan }}
      matrix: ${{ steps.affected.outputs.matrix }}
    steps:
      - name: Checkout workspace manifests
        uses: actions/checkout@v7
        with:
          fetch-depth: 1
          sparse-checkout-cone-mode: false
          sparse-checkout: |
            /*
            !/*/
            **/package.json
      - name: Plan affected work
        id: affected
        uses: XionWCFM/nanoom/.github/actions/affected@latest

  run:
    name: Run affected work (${{ matrix.displayName }})
    needs: affected
    if: needs.affected.outputs.has_change == 'true'
    runs-on: ${{ matrix.runnerLabels || 'ubuntu-latest' }}
    strategy:
      fail-fast: false
      matrix: ${{ fromJSON(needs.affected.outputs.matrix) }}
    steps:
      - name: Checkout planned source
        uses: actions/checkout@v7
        with:
          ref: ${{ matrix.checkout.ref }}
          fetch-depth: 1
          sparse-checkout-cone-mode: false
          sparse-checkout: ${{ matrix.checkout.sparseCheckout }}

      - name: Set up Node.js
        uses: actions/setup-node@v7
        with:
          node-version: '22'

      - name: Focus install planned workspaces
        id: install
        uses: XionWCFM/nanoom/.github/actions/install@latest
        with:
          plan: ${{ needs.affected.outputs.plan }}
          group: ${{ matrix.group }}
          assignmentId: ${{ matrix.assignmentId }}

      - name: Run planned work
        uses: XionWCFM/nanoom/.github/actions/run@latest
        with:
          plan: ${{ needs.affected.outputs.plan }}
          assignmentFile: ${{ steps.install.outputs.assignment-file }}
          installResult: ${{ steps.install.outputs.result }}

  status:
    name: CI status
    if: always()
    needs: [affected, run]
    runs-on: ubuntu-latest
    steps:
      - name: Publish history and check CI results
        uses: XionWCFM/nanoom/.github/actions/status@latest
        with:
          needs: ${{ toJSON(needs) }}
```

그룹을 별도 job으로 나눌 때에만 각 그룹의 양성 변경에 해당하는 job을
status의 requiredJobs로 명시한다. 명시한 배열은 자동 추론보다 우선하며,
다른 그룹의 정상 no-change 생략은 실패로 취급하지 않는다.
