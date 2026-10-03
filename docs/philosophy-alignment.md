# Nanoom 철학과 공개 동작 정합성 검토

기준: 루트 AGENTS.md, ADR-0015, docs/ci-philosophy.md.
시작 소스: 44cece86c3fc90bff05061af9f8941b9d53be9df, 공개 제품 v0.7.7.
현재 상태: 조사와 수정 진행 중. 공개 릴리즈 소비 검증 전에는 완료가 아니다.

## 계획과 수용 기준

1. 공개 CLI, Action, 설정, npm 배포, 기본/고급 예제, 운영 CI와 fixture를
   실제 호출 경로로 대조한다. 과거 설계 문서는 현재 원칙을 바꾸는 근거로 쓰지 않는다.
2. 위반을 재현하고 공통 원인에서 수정한다. 각 공개 동작 변경에는 회귀 검증과
   문서를 함께 남긴다. 소비자 보조 스크립트나 필수 도구 override로 우회하지 않는다.
3. 기본 네 단계에서 정확한 SHA와 파일 집합, 루트 도구와 dependency closure,
   계획한 모든 item의 실행을 확인한다. 무관한 workspace 소스는 제외한다.
4. 양성 변경은 실제 assignment를 실행한다. 변경 없음은 정상 생략한다.
   실행 실패·취소·필수 실행 생략은 aggregate 실패이며, 무관한 그룹 생략은 허용한다.
5. 이력은 기본 경로에서 유효한 측정값을 만들고 다음 계획에서 재사용한다.
   이력 오류는 task 정확도와 독립적인 cold fallback이어야 한다.
6. workflow 수정이 필요하면 완성된 제안 diff와 trigger·권한·필수 check 영향을
   먼저 제시하고 승인받는다. 승인 전에는 workflow 디렉터리에 적용하지 않는다.
7. 변경 검토와 로컬 gates, producer CI, 새 릴리즈, 최신 공개 제품을 사용하는
   두 저장소 CI, 양성 fixture와 aggregate status를 각각 검증한다.

## 발견 사항

| 항목 | 재현되는 문제 | 검증/수정 상태 |
| --- | --- | --- |
| 그룹별 status | positive 전체 계획에서 명시한 requiredJobs 외의 정상 skipped 잡까지 필수로 추가함 | 공통 status 수정 및 성공/필수 skipped/무관한 failure 회귀 통과 |
| 루트 도구 checkout | focused install은 루트를 설치하지만 root manifest의 내부 도구 dependency closure를 checkout하지 않음 | Plan closure 수정; 실제 sparse checkout 회귀 통과 |
| Yarn 실행 자산 | root .yarnrc.yml의 yarnPath/plugin 파일이 manifest-only 계획에서 assignment checkout에 자동 포함되지 않음 | manifest-only 선언 계산; 안전 경로/정규화 회귀 추가 |
| 기본 준비 시간 | 대표 네 단계에서 preparedAtMs가 전달되지 않아 preparation observations가 비어 있음 | 현재 attempt jobs API 자동 시각; 정상/중복/오류/명시적 시각 회귀 통과 |
| 공개 예제 | basic의 제거된 --format, 불완전 workflow, advanced의 @main·필수 override·Node 설정 누락 | 네 단계 템플릿, manifest/lockfile 수정; clean copy install/affected/run 통과 |
| 계약 문서 | SPEC.md가 v0.6.0/미출시/last-successful-push를 현재 계약처럼 설명함 | SPEC 갱신, 과거 계획에 역사적 기록 표시 |
| 릴리즈 toolchain | moving stable에 target을 설치하면서 빌드는 저장소의 고정 toolchain을 사용함 | 완성 diff 준비; 사용자 승인 요청 중, workflow 미적용 |
| 기본 전역 입력 | root lockfile/실행 설정 변경에도 모든 task가 생략됨 | 기본 global 입력과 root tooling 전파 수정; 실제 Git 회귀 통과 |
| focused install | pnpm의 root filter만으로 내부 도구의 dependency가 설치되지 않음; production 환경의 dev tooling 누락; npm 기본 Plan 실행 미지원 | 양쪽 closure와 dev 설치 수정; native pnpm 12.8.2/npm 11.16.0/Yarn 4.11.0 clean install 통과 |
| 이력 재계획 | 중단된 warm CLI가 cold Plan 파일을 덮어써 반환 digest와 실제 파일이 어긋남 | 후보 파일 격리; 중단 회귀 통과 |
| 도구 판별 | 잘못된 packageManager/여러 lockfile을 다른 도구로 조용히 실행함 | 선언/모호성 오류 및 명시적 override 회귀 추가 |
| 로컬 run | affected 선택이 base를 요구하지만 run에는 base 입력이 없음 | --base/--head 추가; 실제 Git 선택 실행/잘못된 조합 회귀 통과 |
| GHES 측정 수집 | v3 download의 미지원 pattern/merge 입력으로 다른 artifact까지 읽음 | bounded API 선택; 현재 attempt/만료/빈 목록/크기 제한 회귀 통과 |

| 사용자 지정 설정 | affected.config로 계획해도 install/run은 기본 설정을 읽음 | Plan configPath와 sparse 설정 복원/검증; 실제 CLI·Git·Action 경로 회귀 추가 |
| 메타데이터 명령 | status/cache-key가 불필요한 설정 파싱에 실패하고 cache-key가 선택한 설정·읽기 오류를 무시함 | 독립 실행과 실제 CLI 회귀; 해시 입력/framing/error 보강 |
| npm 활성화/예측 | 선언한 npm 버전은 활성화하지 않고 preparation은 shrinkwrap을 무시함 | native Corepack npm 실행 및 양쪽 lockfile 우선순위 회귀 |
| 측정 이름 | 긴 job/assignment 이름 truncation으로 artifact 충돌 가능 | 전체 identity digest와 두 긴 잡 측정 보존 회귀 |

## 검증 기록

위의 통과는 개발 중인 소스의 로컬 증거다. 예제와 native install은 실제
패키지 매니저·filesystem·Git을 사용했다. GHES 및 준비 시각 오류 경계는 로컬
API 응답 회귀이며, live GHES 검증을 주장하지 않는다. 전체 local gates가 통과했으며 line coverage는 96.48%다.
native install 회귀는 Action 계약 gate에도 연결했다.
producer CI [37115923209](https://github.com/XionWCFM/nanoom/actions/runs/37115923209)는 실패했다.
Linux는 새 Git 회귀가 전역 author 설정에 의존했고 Windows는 pnpm --prod=false
기대값이 누락됐다. 로컬 author 없는 Git 경로와 Windows 기대값을 수정했다.
후속 변경의 `scripts/verify-completion.sh --local`이 통과했다. line coverage는
96.29%이며 실제 CLI, native focused install과 Action 계약을 포함한다.
producer 재실행은 진행 중이다.
새 릴리즈 및 두 저장소의 최신 공개 제품 소비 검증은 아직 수행하지 않았다.
