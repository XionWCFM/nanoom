# ADR-0011: affected manifest checkout과 run checkout plan

## 상태

Accepted

## 배경

대규모 모노레포에서 affected 계산이 source blob과 과거 tree를 내려받거나, 각 run이
전체 repository를 checkout하면 Git 전송량과 self-hosted runner의 디스크 사용량이
matrix 크기에 비례해 커진다. 동시에 sparse checkout이 manifest를 빠뜨리거나 삭제된
workspace를 현재 graph에서 놓치면 빠른 대신 잘못된 affected 결과가 된다.

## 결정

1. 사용자가 `actions/checkout`을 소유한다. Nanoom 전용 checkout Action은 만들지 않는다.
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
- unrelated workspace와 서비스는 run worktree에 나타나지 않는다.
- workspace manifest 삭제와 rename은 남은 workspace 전체를 선택하고 이유를 설명한다.
- `cleanupCheckout: true`인 실패하거나 취소된 self-hosted job도 격리 checkout을 정리한다.
- hosted fixture와 released Action 검증 전에는 릴리스 완료로 보지 않는다.
- Nanoom과 nanoom-fixtures를 각각 main에 병합하고, 정식 release tag를 소비하는 fixture의
  post-merge hosted run과 aggregate status가 성공해야 작업 완료로 본다.

## 결과

정상 run은 필요한 dependency closure만 받으며 affected history는 commit object로
제한된다. workspace 구조 변경은 평소보다 많은 job을 실행할 수 있지만 누락하지 않는다.
run마다 checkout 요청은 남고, matrix output 1 MiB 한도와 task별 추가 경로는 실제
fixture에서 한도나 누락이 확인될 때 별도 결정한다.
