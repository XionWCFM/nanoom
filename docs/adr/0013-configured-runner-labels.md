# ADR-0013: config 기반 runner label routing

## Status

Accepted for v0.6.0.

## Context

Nanoom은 assignment 수와 내용을 만들지만 consumer workflow의 `runs-on`은 고정되어 있었다. 규모별 tier가 다른 self-hosted pool을 사용하면 runtime history도 실제 pool별로 분리되어야 한다. GitHub Actions의 label 배열은 우선순위나 fallback이 아니라 모든 라벨을 만족하는 runner를 선택하는 AND 조건이다.

## Decision

- group과 `distribution.small|medium|full`에 선택적 `runnerLabels: string[]`와 `timingEnvironment: string`을 둔다.
- 선택 우선순위는 tier의 explicit environment, tier labels에서 파생한 environment, group의 explicit environment, group labels에서 파생한 environment, 기존 Action runner environment 순이다. `runnerLabels`는 tier가 group을 덮어쓴다.
- matrix의 label 순서는 입력대로 보존한다. 파생 history identity는 정렬된 배열의 canonical JSON으로 만들어 순서만 바뀌어도 history가 갈라지지 않게 한다.
- 빈 배열, 빈/제어문자 label, 중복 label, 빈/제어문자 environment는 config validation error다.
- consumer는 `runs-on: ${{ matrix.runnerLabels || 'ubuntu-latest' }}`로 라우팅한다. Nanoom은 runner availability API를 조회하거나 fallback pool을 선택하지 않는다.
- config가 만든 matrix environment는 run sample의 environment로 사용하여 affected prediction과 다음 run sample key를 일치시킨다.

## Alternatives

- live runner availability 조회와 자동 fallback은 race condition, 추가 권한, 예측 불가능한 history 혼합을 만들기 때문에 제외한다.
- label별 runtime cost나 heterogeneous assignment optimizer는 측정 근거가 없으므로 제외한다.
- GitHub runner group object까지 config에 추가하는 것은 현재 배열 label 요구에 필요하지 않아 제외한다.

## Acceptance criteria

- group labels가 legacy와 distribution matrix entry에 배열로 나오고 실제 hosted matrix `runs-on`에서 소비된다.
- tier labels/environment가 group 값을 덮어쓰고 해당 environment의 history sample만 예측에 사용한다.
- label 순서 변경은 같은 파생 history identity를 만들고 pool label 변경은 다른 identity를 만든다.
- 설정 생략은 기존 matrix shape와 hosted fallback을 유지한다.
- invalid label/environment config는 실행 전에 실패한다.
- Action outputs와 canonical result가 같은 runner fields를 보존하고 run sample도 같은 environment를 기록한다.
- released `latest`를 쓰는 `nanoom-fixtures`의 small/medium/full matrix와 aggregate status가 성공한다.

## Consequences

runner routing은 config에서 설명 가능하고 history는 pool별로 분리된다. 일치하는 self-hosted runner가 없으면 GitHub queue에서 대기하며 Nanoom은 성공이나 fallback을 가장하지 않는다. PR-controlled config는 privileged self-hosted runner routing 권한이 될 수 있으므로 untrusted PR workflow는 고정 hosted runner 또는 격리된 pool을 사용해야 한다. 실제 self-hosted/GHES runner가 없는 동안에는 hosted array contract만 검증하며 GHES 실행 검증을 주장하지 않는다.
