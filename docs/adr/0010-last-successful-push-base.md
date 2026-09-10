# ADR-0010: 마지막 성공 push를 affected 기준으로 사용

- 상태: Accepted
- 대상 릴리스: v0.4.0

## 결정

GitHub `push` 이벤트에서 `.github/actions/affected`는 현재 workflow 파일과 branch의
가장 최근 성공한 `push` run을 Actions API로 조회하고, 그 run의 `head_sha`부터
`github.sha`까지를 비교한다. 명시적인 `base`/`head` input은 이 조회보다 우선한다.

pull request에서는 `github.event.pull_request.base.sha`를 먼저 선택해 synchronize
이벤트의 `github.event.before`가 PR base를 덮어쓰지 않게 한다. merge queue에서는
`github.event.merge_group.base_sha`를 선택한다. GitHub API,
workflow context, 권한은 Action 경계에만 두고 Rust CLI에는 full `--base`/`--head`를
전달한다.

## 실패와 복구

성공 run 조회나 응답 검증이 실패하면 CLI를 실행하지 않는다. 선택한 SHA의 history가
checkout에 없으면 CLI가 ADR-0011의 bounded blobless fetch를 수행한다. fetch 또는
base가 head의 ancestor인지 확인하는 과정이 실패하면 정상 outputs를 기록하지 않고,
축소된 event-before 비교나 자동 full run으로 폴백하지 않는다. Action은
`actions: read`와 `contents: read` 권한, 그리고 bootstrap용 명시적 `base` input을
안내한다.

## 관찰 가능성

canonical `result.revisionResolution`은 다음을 포함한다.

- `baseSource`: `explicit`, `pullRequestBase`, `mergeGroupBase`, `lastSuccessfulPush`
- `baseCommit`, `headCommit`: 실제 비교에 사용한 full SHA
- `successfulRunId`: `lastSuccessfulPush`일 때의 run ID, 그 외에는 `null`

같은 값은 always-visible log와 Step Summary에도 표시한다. lockfile 정밀 분석,
Task DAG, remote cache, Atomizer, flaky retry, coordinator/server는 이 결정의
범위가 아니다.

## 검증

- explicit override, PR, merge queue, 성공 push 조회, 현재 run 제외를 contract test로
  검증한다.
- 권한/API 오류, 빈 성공 이력, malformed 응답, fetch 실패, 비조상 SHA는 모두 실패한다.
- released fixture에서 성공 baseline → 실패 workspace 변경 → 후속 push 순서를 실행해
  마지막 성공 baseline부터 변경이 다시 선택되는지 확인한다.
