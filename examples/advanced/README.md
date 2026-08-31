# Advanced example

실전에서 쓰는 고급 기능을 전부 담은 예제입니다.

```
advanced/
├── nanoom.config.json          # 2개 그룹 + 규칙 + globalDependencies + workspace 오버라이드
├── pnpm-workspace.yaml
├── packages/
│   ├── design-system/          # test를 3개 샤드로 분할
│   ├── db-migrations/          # 일반 build work item
│   └── legacy-admin/           # ci 그룹에서 완전 제외(ignore)
├── apps/
│   ├── web/                    # e2e를 2개 샤드로, design-system에 의존
│   └── mobile/                 # 독립 앱
└── tools/release-bot/          # pnpm 워크스페이스엔 있지만 nanoom 설정으로 제외
```

## 설정 포인트

| 기능 | 어디서 | 무엇을 하는지 |
| --- | --- | --- |
| `ignore: true` | `@adv/legacy-admin` | 이 프로젝트는 ci 그룹 매트릭스에서 아예 빠짐 |
| `distribution` | `ci` | affected 비율에 따라 Nanoom assignment 수 상한을 선택 |
| `shard: [{ task, shard }]` | `@adv/design-system`, `@adv/web` | 긴 테스트를 N개 조각으로 쪼개 병렬 실행 |
| `globalDependencies` | 락파일, 워크플로우 파일 | 이 파일들이 바뀌면 모든 프로젝트가 영향받음으로 처리 |
| `workspace.include/exclude` | `tools/*` 제외 | 발견기가 자동 찾은 후보 중 불필요한 디렉토리 드롭 |

## 직접 실행해보기

```bash
cd examples/advanced
git init -b main && git add . && git commit -m init

# 패키지 하나 수정 후 커밋한 뒤:
nanoom affected --base main --head HEAD
nanoom affected --base main --head HEAD --json  # matrix와 선택 이유를 함께 출력
nanoom run ci test                 # 영향받은 프로젝트만 순서대로 실행

# 샤드 나눠 실행 (GitHub Actions의 각 잡에서)
nanoom run ci test --shard 1 --total-shards 3
nanoom run ci test --shard 2 --total-shards 3
```

## 기대 동작 예시

- `packages/design-system` 수정 → design-system(test×3샤드, build, typecheck) + **web**(의존성 전파) 감지.
  web의 e2e는 e2e 그룹에서 2샤드로 추가 생성.
- `apps/mobile`만 수정 → mobile 항목만 생성.
- `pnpm-lock.yaml` 커밋 → 모든 프로젝트가 영향받음 처리.
- `legacy-admin`을 아무리 수정해도 매트릭스에 절대 등장하지 않음.

## GitHub Actions 연동

```yaml
jobs:
  matrix:
    runs-on: ubuntu-latest
    outputs:
      groups: ${{ steps.affected.outputs.groups }}
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      - id: affected
        uses: XionWCFM/nanoom/.github/actions/affected@main

  test:
    needs: matrix
    strategy:
      matrix: ${{ fromJSON(needs.matrix.outputs.groups).ci.matrix }}
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: npm install -g @nanoom/cli && pnpm install
      - run: nanoom run ci ${{ matrix.task }} ${{ matrix.shard && format('--shard {0} --total-shards {1}', matrix.shard, 3) || '' }}
```
