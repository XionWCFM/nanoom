# Nanoom CI의 이상향과 철학

이 문서는 사용자가 리뷰한 목표 템플릿이다. **현재 릴리즈에서 그대로 실행된다는 보장이 아니라, 제품과 워크플로 변경을 판단하는 기준**이다. 실제 워크플로는 별도의 사용자 승인 후 변경한다.

## 추구하는 사용자 경험

CI는 항상 최신 공개 릴리즈 Nanoom을 사용한다. 기본 템플릿만으로 효율적인 실행과 정확한 최종 상태가 가능해야 한다. 소비자가 jq나 shell로 제품의 부족한 동작을 보완하거나, 저장소에 이미 선언된 Nx·Turbo·패키지 매니저를 Action에 다시 가르치게 하지 않는다.

세 잡은 계획·실행·최종 상태라는 책임을 드러낸다. 실행 잡의 네 단계는 checkout → Node 환경 설정 → focused install → run이다. 공식 Action의 책임과 Nanoom의 책임을 구분하고, 환경 준비와 설치를 run Action 안에 숨기지 않는다. 관심사별 Nanoom Action을 하나의 command 기반 Action으로 합치지 않는다.

Nanoom은 무엇을 가져오고 설치하고 실행할지 계산한다. 사용자는 Action의 입력·출력을 연결한다. affected의 matrix는 사람이 읽을 이름과 정확한 소스 SHA, assignment별 sparse checkout 경로를 제공한다. 경로에는 실행 workspace·내부 dependency closure·루트 설정과 lockfile·추가 필수 경로를 포함하며 무관한 소스는 제외한다. Plan은 권위 있는 실행 계약으로 유지한다.

## 목표 템플릿

`ci`는 Nanoom 설정의 실행 그룹이고 Node 22는 이 예시의 프로젝트 선택이다. 버전 숫자나 모든 입력 이름을 영구 고정하는 것이 목적은 아니다. 핵심 책임과 사용자 경험을 바꾸려면 먼저 사용자 승인을 받는다.

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
      groups: ${{ steps.affected.outputs.groups }}
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
    name: ${{ matrix.displayName }}
    needs: affected
    if: needs.affected.outputs.has_change == 'true'
    runs-on: ${{ matrix.runnerLabels || 'ubuntu-latest' }}
    strategy:
      fail-fast: false
      matrix:
        include: ${{ fromJSON(needs.affected.outputs.groups).ci.include }}
    steps:
      - name: Checkout planned source
        uses: actions/checkout@v7
        with:
          ref: ${{ matrix.checkout.ref }}
          fetch-depth: 1
          sparse-checkout-cone-mode: false
          sparse-checkout: ${{ matrix.checkout.sparseCheckout }}

      - name: Set up Node.js
        uses: actions/setup-node@v4
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

## 각 단계의 책임

- **affected:** 이벤트별 비교 SHA와 필요한 Git 객체 확보, 영향 범위·작업 분배·checkout 메타데이터 계산, 읽기 쉬운 이름과 Summary 제공.
- **checkout:** Nanoom이 제공한 SHA와 sparse 패턴으로 정확한 실행 소스 확보.
- **Node 설정:** 프로젝트가 선택한 Node 환경을 공식 Action으로 준비.
- **install:** Plan 검증과 assignment 선택, 패키지 매니저 자동 판별·활성화, root 도구와 내부 dependency closure를 포함한 focused install, run에 전달할 검증 결과 제공.
- **run:** 검증된 assignment 실행, Nx·Turbo 자동 판별, 결과와 측정값 기록.
- **status:** 변경 없음의 정상 생략과 필수 실행 실패·취소·생략 구분, 이력 게시와 최종 Summary 제공.

## 구현 검토 기준

현재 부족한 계약은 displayName·checkout 메타데이터, install의 Plan 선택과 출력 연결, status의 필수 실행 판단과 이력 처리다. 이를 사용자 커스텀 로직으로 우회하지 않고 제품에서 해결한다.

실제 checkout 파일 집합과 SHA, 집중 설치 대상과 dependency closure, 모든 assignment 실행, no-change 생략, 실패·취소·필수 실행 생략의 status 실패를 검증한다. 로컬 테스트와 공개 릴리즈 소비 증거는 구분한다. 최신 릴리즈로 두 저장소 CI와 양성 변경 fixture가 통과하기 전에는 제품이 이상향을 충족했다고 선언하지 않는다.
