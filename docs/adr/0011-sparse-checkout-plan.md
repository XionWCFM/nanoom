# ADR-0011: affected manifest checkout과 run checkout plan

## 상태

Accepted

## 배경

대규모 모노레포에서 affected 계산이 source blob과 과거 tree를 내려받거나, 각 run이
전체 repository를 checkout하면 Git 전송량과 self-hosted runner의 디스크 사용량이
matrix 크기에 비례해 커진다. 동시에 sparse checkout이 manifest를 빠뜨리거나 삭제된
workspace를 현재 graph에서 놓치면 빠른 대신 잘못된 affected 결과가 된다.

## 결정

1. affected graph checkout은 workflow가 소유한다. assignment checkout은 `prepare` composite Action이 공식 `actions/checkout`을 호출하고 Git을 직접 구현하지 않는다.
2. affected job은 non-cone pattern으로 root `package.json`, `nanoom.config.json`,
   `workspace.include` 범위의 모든 workspace `package.json`만 checkout한다.
3. workspace discovery는 Nx, Turbo, package-manager 설정을 읽지 않고 Nanoom config와
   checkout된 manifest만 사용한다. Git index에 존재하는 필수 manifest가 worktree에
   없으면 실패한다.
4. 조상 검증과 merge-base에 필요한 shallow history는 `tree:0`으로 tree, blob, tag 없이
   commit만 32, 128, 512, 설정된 최대 depth 순으로 가져오며 기본 최대값은 2048이다.
5. matrix entry는 affected workspace, 내부 dependency closure, `checkout.always`의
   합집합을 cone-mode checkout plan으로 제공한다. Git cone mode가 root 파일을
   포함하므로 run job의 lockfile과 root 설정은 별도 pattern으로 나열하지 않는다.
6. self-hosted runner의 checkout `path`는 run ID, attempt, job, matrix index로 동적 격리한다.
   run Action의 opt-in `cleanupCheckout`은 마지막 `if: always()` 단계에서 `.nanoom/`
   하위 경로인지 검증한 후 삭제한다.
7. configured workspace manifest가 삭제되거나 rename되면 이전 graph를 추가로 복원하지
   않고 현재 graph에 남은 모든 workspace를 affected 처리한다.
8. `nanoom-fixtures`는 branch ref나 local binary를 소비하지 않는다. release 완료 때
   이동하는 `latest` tag의 Action을 사용하고, setup은 GitHub 최신 Release의 실제 versioned
   asset을 내려받는다.
9. Plan v1 상세는 30일 file artifact로 두고 stdout에는 digest/provenance reference와 bounded assignment matrix만 전달한다. `affected` producer는 Plan file CLI로 생성한다. 재실행은 같은 run에서 `producerAttempt <= current.attempt`인 이전 계획만 허용하며 consumer가 current identity를 채운다. group당 256행 또는 UTF-16 output 1 MiB 초과는 실패하고 no-change의 0-assignment Plan은 유효하다. `affected --json` full report는 Plan output과 함께 요청하지 않는다.
10. Planned install은 assignment workspace union을 non-empty JSON string array로 `nanoom install --filter-file FILE`에 전달한다. malformed 또는 빈 범위가 전체 root install로 조용히 확대되는 일은 없으며, 기존 standalone no-filter install은 root install을 유지한다.
11. `prepare`는 original Plan reference와 artifact reference를 비교하고, digest/schema/provenance/assignment/head를 검증한 뒤 exact head를 run/attempt/job/matrix index별 경로에 checkout한다. root-only non-cone shallow checkout 뒤 assignment `paths.txt`를 cone mode로 적용한다. install/run은 trusted Plan reference와 assignment-file을 모두 받아 Plan digest와 실제 Git HEAD를 다시 검증한다. static inline-matrix 소비는 제거하고 `scheduler=http`의 기존 continuous-agent matrix 입력만 남긴다.
12. GitHub.com은 upload-artifact v4.6.2/download-artifact v4.3.0, GHES wrappers는 upload-artifact v3.2.2/download-artifact v3.1.0을 사용한다. `affected-ghes`와 `prepare-ghes`는 shared shell validator/checkout 로직을 재사용한다. same-run Plan download는 official download Action을 사용한다.

## 검토한 대안

- 정확한 base/head snapshot만 비교해 history fetch를 없애는 방식은 non-ancestor base를
  실패시키는 기존 정책을 검증할 수 없어 채택하지 않았다.
- base와 head의 manifest graph를 모두 복원하는 방식은 드문 삭제/rename을 위해 추가
  tree와 parser 복잡도를 요구하므로, 보수적인 full affected보다 비용이 크다.
- 모든 run이 같은 checkout을 공유하는 방식은 격리 실패와 동시 실행 충돌을 만들므로
  기본값으로 채택하지 않았다.

## 수용 기준

- manifest-only shallow clone에서 affected와 dependency propagation이 계산된다.
- 필수 workspace manifest 하나를 빼면 명시적으로 실패한다.
- affected 출력만으로 새 cone checkout을 만들고 focused install과 실제 task를 실행한다.
- Plan v1 file reference의 digest/provenance가 맞지 않거나 assignment가 없으면 선택 전에 실패하고, 같은 run의 유효한 이전 producer attempt는 재사용된다.
- unrelated workspace와 서비스는 run worktree에 나타나지 않는다.
- workspace manifest 삭제와 rename은 남은 workspace 전체를 선택하고 이유를 설명한다.
- `cleanupCheckout: true`인 실패하거나 취소된 self-hosted job도 격리 checkout을 정리한다.
- hosted fixture와 released Action 검증 전에는 릴리스 완료로 보지 않는다.
- Nanoom과 nanoom-fixtures를 각각 main에 병합하고, 정식 release tag를 소비하는 fixture의
  post-merge hosted run과 aggregate status가 성공해야 작업 완료로 본다.
- fixture completion gate는 build, test, typecheck의 세 run job과 aggregate status 성공을 검증한다.

## 결과

정상 run은 필요한 dependency closure만 받으며 affected history는 commit object로
제한된다. workspace 구조 변경은 평소보다 많은 job을 실행할 수 있지만 누락하지 않는다.
run마다 checkout 요청은 남지만 ADR-0012부터 runtime makespan이 같은 후보에서는
assignment 전체의 중복 closure path 수가 적은 배치를 선택한다. byte 전송량과 실제 GHES
부하는 GHES 환경이 제공될 때 별도로 검증한다.
