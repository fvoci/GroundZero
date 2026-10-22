# SQL 통합 준비

이 디렉터리는 누적 마이그레이션을 대체하기 전 검토용 후보와 비교 근거를 담는다.
제품의 정본은 계속 `crates/server/migrations/`다. 이 작업은 기존 SQL의 교체·삭제·번호 변경,
기존 DB의 이력 수정·초기화, main 병합·태그·배포를 수행하지 않는다.

## 조사 기준과 경계

- 공통 조사 SHA: `cb65cc31156c0fd8d28cf856bc132d4a506c8885` (2026-10-22 원격 main).
- [#91](https://github.com/fvoci/GroundZero/pull/91)은 이 SHA로 병합됐다.
- 조사 시작 시 [#69](https://github.com/fvoci/GroundZero/pull/69)는 열린 Draft,
  head `e4cae26dbf3104ba40d6ef39fda50b8d8b2035cb`였다. 미병합 기록을 확정 결정으로 삼지 않는다.
- #69와 후속 수정이 정리되기 전에는 **최종 통합 기준 미확정**이다. 그 뒤에는 공통 조사 SHA와
  최종 main 사이의 변경만 추가 검토하고, 영향을 받은 후보·시험을 갱신한다.
- 진행 중 #69가 `4fa6f1589984141fc4f892e522ca9182f178738d`로 바뀌었다. 직전 head와의 델타는
  결정·개발 문서 두 파일이며 D-99와 `fix/audit-reconnect-discard`(W21) 후속을 기록한다.
  SQL·서버·시험 변경은 없지만 이 후속도 기준 확정의 대기 대상이다. 해당 편집기 작업은 수정하지 않는다.
- 이후 #69의 `44aa798106a89f754c67dd1cf88ca61bf79e2bd5`까지 확인했다. 직전 head와의 차이는
  개발 문서의 리뷰 처리 기록과 NOT RUN 목록 정정뿐이다. 원격 main은 공통 조사 SHA와 같으며 최종 기준은 여전히 미확정이다.
- D-08·D-17·D-42·D-88·D-92 및 기존 서버 권한·업무 규칙을 보존한다.
  UUIDv7은 앱이 공급하며 DB 생성 fallback을 추가하지 않는다.
- PostgreSQL과 셀프호스트 Supabase 어댑터는 지원 환경 계약이다. 과거 개발 버전 호환 코드와
  구분한다. Supabase Cloud 변경 시험은 계획만 작성한다.
- 추가 사용자 지시에 따라 Supabase Cloud의 권한 제약에서도 동작하는 표준 PostgreSQL 공통 구현을
  설계 우선순위로 삼는다. 제품 SQL은 하나로 유지하고 환경 차이는 불가피한 최소 초기 설정·접속 설정에만 둔다.
  Supabase 전용 서비스 의존성은 추가하지 않는다. 로컬 제한 권한 시험과 실제 Cloud 검증은 구분하며,
  이 설계 지시를 원격 DB 변경 승인으로 해석하지 않는다.
- 미승인 P-24·P-25·P-26·P-28·P-29·P-30·P-31·P-35는 후보에 구현하지 않는다.
  현재 동작과의 동등성을 먼저 검증하고 정책 판단은 별도로 남긴다.

## 작업 분리

세 작업자는 위 SHA에서 각각 독립 worktree로 시작했다. 결과를 종합 worktree에 모아 하나의
독립 Draft PR로 검토한다. 기존 편집기 수정·감사 기록·병합 worktree는 수정하지 않는다.

| 담당                | worktree             | 소유 파일                                                                 |
| ------------------- | -------------------- | ------------------------------------------------------------------------- |
| A: 최종 객체·후보   | `sql-prep-a`         | `preparation/sql-consolidation/candidate/`, `inventory/`, `analysis-a.md` |
| B: 동등성 도구·시험 | `sql-prep-b`         | xtask 비교 구현, 필요한 최소 시험 연결, `verification/`                   |
| C: 설치·역할·환경   | `sql-prep-c`         | `environment.md`, `environment/`                                          |
| 종합                | `sql-prep-synthesis` | 본 안내, 기준·판단·검증 결과 종합, Draft PR                               |

작업자 모델은 `gpt-6-astra`, 추론 강도는 별도 `xhigh`로 지정했다. 시작 시 로컬 Codex의
실행 속도 설정은 `service_tier = "ultrafast"`였다. `/batch`와 새 전면 감사는 사용하지 않는다.

DB·Docker 실행은 B가 전담한다. 이 작업용으로 새로 만든 프로젝트·DB와 동적 포트만 쓰며
공유 개발·시험 DB 및 기존 볼륨을 보존한다. `cargo xtask down`과 광범위한 정리는 하지 않는다.
빌드 출력은 각 worktree에서 분리한다.

실행 기록의 예외: 초기 A/A 도구는 수정 전 종료 경로에서 이 작업 전용 B 프로젝트에 한 번
`docker compose down --volumes --remove-orphans`를 실행했다. 삭제 대상은 해당 임시 PostgreSQL
컨테이너와 네트워크였고, DB는 tmpfs여서 해당 프로젝트의 이름 붙은 볼륨은 없었다.
공유 DB·기존 볼륨을 대상으로 하지 않았다. 이후 도구는 `stop`만 사용하며,
이 예외를 숨기거나 전체 실행이 처음부터 stop-only였다고 기록하지 않는다. 상세 로그는 검증 실행 기록에 남긴다.

## 전환 전 관문

1. #69와 후속 변경 종료를 확인하고 최종 main SHA를 기록한다.
2. 기준과의 델타에서 SQL·호출자·seed·권한·설치 계약 변화만 추적해 후보에 반영한다.
3. 기존 누적 SQL과 후보를 별도 빈 DB에 적용하고 객체 비교와 앱 역할 동작 비교를 각각 통과한다.
   정책·권한·함수 차이를 넣은 검사 자체 검증도 통과해야 한다.
4. PostgreSQL, 셀프호스트 Supabase 직접 연결·session pooler에서 해당 결과를 확인한다.
   Cloud의 읽기 전용 연결 성공을 이 결과로 대체하지 않는다.
5. 남은 정책 및 기존 데이터 보존·이전 방법을 결정한다. 새 설치의 동등성만으로 기존 DB의
   checksum·백업 복원 호환성을 보장하지 않는다.
6. 별도 구현에서만 실제 마이그레이션 교체를 검토한다. 본 PR은 그 전 단계다.

객체 차이와 동작 시험 결과는 서로 대체하지 않는다. 실행하지 않은 환경·경로는 `NOT RUN`,
비교 실패는 `FAIL`로 기록한다.

## 검토 자료

- [최종 객체·제거/유지 근거](analysis-a.md), [객체와 호출 목록](inventory/README.md)
- [후보 SQL과 적용 조건](candidate/README.md)
- [비교 명령·정규화·검증 한계](verification/README.md)
- [SHA별 DB 실행 결과와 증거](verification/results.md), [종합 품질 검사](quality-check.md)
- [설치·역할·환경](environment.md), [session 상태](environment/session-state.md), [Cloud 후속 계획](environment/cloud-plan.md)
- [종합 판단·실제 시험 대응·전환 순서](synthesis.md)
- 검토용 Draft PR: [#92](https://github.com/fvoci/GroundZero/pull/92)
