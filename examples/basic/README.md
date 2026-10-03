# Basic example

pnpm workspace의 공유 라이브러리 `@basic/ui`, 이를 사용하는 `@basic/web`,
독립 앱 `@basic/api`로 구성한다. 패키지 매니저와 workspace 범위는 저장소 선언에서
자동 판별한다. root manifest와 lockfile을 포함해 frozen install이 가능하다.

## 직접 실행

이 디렉터리를 별도 위치로 복사하고 실행한다. 원본 Nanoom 저장소 안에 새 Git
저장소를 만들지 않는다. 아래 명령은 Nanoom CLI와 pnpm이 설치된 환경 기준이다.

```bash
example_dir=$(mktemp -d)
cp -R examples/basic/. "$example_dir"
cd "$example_dir"
pnpm install --frozen-lockfile
git init -b main
git add .
git commit -m "example baseline"
git switch -c example/change
printf 'changed\n' > packages/ui/change.txt
git add .
git commit -m "change ui"
nanoom affected --base main --head HEAD --json
nanoom run ci test --base main --head HEAD
```

`ui` 변경은 `ui`와 `web`을 선택하고 `api`는 선택하지 않는다. `api` 변경은
`api`만 선택한다. root manifest·lockfile·기본 실행 설정 변경은 모든 workspace를
선택한다. 전체 실행은 `nanoom run ci test --all`로 명시한다.

## GitHub Actions

이 예제를 별도 저장소의 루트로 사용할 때 아래 템플릿을 연결한다.
운영 Action은 최신 공개 릴리즈와 대응 binary를 사용한다. 첫 push처럼
비교 base가 없는 이벤트는 affected의 `base` 입력을 명시해야 한다.

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

`status`는 `needs`의 `has_change`로 양성 실행과 변경 없음의 정상 생략을 구분하고
성공한 실행의 이력을 게시한다. 소비자가 필수 잡 계산이나 이력 병합을 보완하지 않는다.
