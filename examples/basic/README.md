# Basic example

가장 흔한 모노레포 형태입니다. pnpm 워크스페이스에 라이브러리(`packages/ui`) 하나,
앱(`apps/web`, `apps/api`) 두 개. `web`은 `ui`에 `workspace:*`로 의존합니다.

```
basic/
├── pnpm-workspace.yaml
├── nanoom.config.json      # ci 그룹: test + build
├── packages/ui/            # 공유 라이브러리
└── apps/
    ├── web/                # ui에 의존하는 프론트엔드
    └── api/                # 독립적인 백엔드
```

## 직접 실행해보기

```bash
cd examples/basic
git init -b main && git add . && git commit -m init   # 베이스 커밋 필요

# 전체가 변경됨 (아무 파일이나 수정 후 커밋했다면 해당 프로젝트만 감지)
nanoom affected --format text
nanoom run ci test
```

## 기대 동작

- `packages/ui/src`를 수정하면 → **ui와 web** 둘 다 감지 (`workspace:*` 의존성 전파)
- `apps/api`를 수정하면 → **api만** 감지
- `pnpm-lock.yaml` 같은 락파일을 고정 예제에는 넣지 않았으므로,
  실제 사용 시 `globalDependencies`에 락파일을 추가해 전체 실행을 트리거하세요.

## 참고

설정은 단 한 개의 그룹뿐입니다. 규칙(rules), 샤딩, 격리가 필요하면
[advanced 예제](../advanced/README.md)를 보세요.
