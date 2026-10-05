# Nanoom v0.5 CI design

> 이 문서는 작성 당시의 설계·조사·실행 기록이다. 현재 공개 계약과 작업 기준은
> [SPEC.md](SPEC.md), [AGENTS.md](AGENTS.md), [CI 철학](docs/ci-philosophy.md)을 따른다.
> 과거 branch/PR 상태와 후보 검증을 현재 구현 상태나 이번 수정의 완료 증거로 사용하지 않는다.

```text
affected ── groups(matrix) ──> install ──> run ──> status(needs only)
    │                                      │
    └─ previous successful history         └─ successful timing samples ──> history ──> status
```

- `affected`는 GitHub context를 explicit base/head로 정규화하고 assignment를 만든다.
- `install`은 static assignment workspace union을 focused install한다. continuous agent는 전체 closure를 설치한다.
- `run`은 static items를 순서대로 실행하거나 HTTP claim loop를 수행한다.
- `history`는 artifact scheduler의 기본 lifecycle이며 성공한 `run` 뒤 sample을 병합한다. 명시적 `scheduler: off`에서만 건너뛴다.
- `status`는 job ID를 가정하지 않고 `toJSON(needs)` 전체만 평가한다.

Runner bucket 수와 GitHub 동시 실행 제한은 서로 다른 정책이다. Nanoom `concurrency`를 `strategy.max-parallel`로 복사하지 않는다.
