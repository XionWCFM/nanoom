# Nanoom v0.3 CI design

```text
affected ── groups(matrix) ──> install ──> run ──> status(needs only)
    │                                      │
    └─ best-effort previous history        └─ successful timing samples ──> history
```

- `affected`는 GitHub context를 explicit base/head로 정규화하고 assignment를 만든다.
- `install`은 static assignment workspace union을 focused install한다. continuous agent는 전체 closure를 설치한다.
- `run`은 static items를 순서대로 실행하거나 HTTP claim loop를 수행한다.
- `history`는 correctness 경로와 분리된 telemetry side branch다.
- `status`는 job ID를 가정하지 않고 `toJSON(needs)` 전체만 평가한다.

Runner bucket 수와 GitHub 동시 실행 제한은 서로 다른 정책이다. Nanoom `concurrency`를 `strategy.max-parallel`로 복사하지 않는다.
