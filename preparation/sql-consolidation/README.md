# SQL 통합 기록과 비교

제품 SQL의 정본은 [`crates/server/migrations/`](../../crates/server/migrations/README.md)다.
#92의 최종 상태 후보를 실제 초기 마이그레이션으로 전환했으며 준비 디렉터리에 SQL 사본을 유지하지 않는다.
기존 개발·시험 DB는 [D-107](../../docs/decisions.md#d-107)에 따라 대상·백업·데이터 소실 확인과 최종 승인 후 재생성한다.
통합 구현이나 빈 DB 검증은 기존 DB 삭제 승인을 대신하지 않는다.

## 현재 비교 기준

- 이전 누적 SQL: main [`8ffd8c3b940924eca8cf7ec739a85494243c0479`](https://github.com/fvoci/GroundZero/tree/8ffd8c3b940924eca8cf7ec739a85494243c0479/crates/server/migrations), 97개 파일·마지막 버전 98.
- #92 후보 생성 기준: `3db81d1deae24b16478581ebd6a06de2b2a2d8c9`. 위 main까지 SQL 내용 변경은 없고 마이그레이션 README만 바뀌었다.
- 새 기준: `202610230001_initial_state.sql`, 최종 객체·의존 순서와 현행 주석으로 정리한 제품 SQL.
- [실행 명령·정규화 범위](verification/README.md), [전환 범위](synthesis.md), [재생성 안내](../../docs/self-hosting.md#sql-baseline-reset).

`cargo xtask sql-compare`는 고정 SHA의 이전 SQL을 실행별 `target/` 출력 아래에만 꺼내 두 빈 DB를 비교한다.
제품 SQLx 이력은 새 버전·실제 체크섬으로 생성한다. 이전 이력 복사·체크섬 우회·기존 DB 자동 이전은 없다.
원본 로그·카탈로그·스크린숏은 `target/`에 두고 장기 보존 시 [기존 증거 방식](evidence.md)을 따른다.

## 기준과 증거

이 디렉터리의 조사·실행 요약은 **역사 기록이며 현재 상태를 나타내지 않음**이다.
관련 결함이나 SHA를 조사할 때만 원시 증거를 명시적으로 연다. 현재 사용자 요구사항,
승인 결정, 코드·테스트를 기준으로 삼고 서로 충돌하면 보고한다.

- 분리 대상 체크아웃: `abd545a250ff1fd9cc166b3c078c4dff76846724` (#92).
- 후보 생성 기준: `3db81d1deae24b16478581ebd6a06de2b2a2d8c9` (#97의 `b01d74a` 병합).
- 최종 기록된 A/B 실행 소스: `f5b5a9d4a42dc6c87deddddcda0d0ea41f7092f9`.
- 최초 조사 기준: `cb65cc31156c0fd8d28cf856bc132d4a506c8885`. 이 기준의 실행은 정책 반영 후 결과로 대체되었다.
- [보존 위치·SHA-256 목록·분리 검증](evidence.md), [SHA별 최종 결과와 NOT RUN 범위](verification/results.md).

증거 분리 시 #97 최신 `0a26654`와 이미 포함된 `b01d74a` 사이의 추가 변경은 웹 컴포넌트·시험·문서이며
마이그레이션 입력은 바뀌지 않았다. 당시 증거 분리 작업은 SQL이나 동작 변경 및 A/B 재실행을 포함하지 않았다.
#106은 검토 완료·조치 항목 없음인 보존용 PR이며 다시 검토하거나 병합할 대상이 아니다.
#97 → #92는 main에 병합됐다. 그 검증이 이번 제품 통합의 검증을 대신하지 않는다.

## 유지하는 자료

- [최종 객체·제거/유지 근거](analysis-a.md), [목록 설명과 재생성 명령](inventory/README.md), [테이블 요약](inventory/tables.md)
- [준비 당시 후보 보존 위치](candidate/README.md)
- [비교 명령·정규화·검증 한계](verification/README.md), [사전 검사 재현 fixture](verification/fixtures/README.md)
- [기록된 품질 검사 결과](quality-check.md)
- [설치·역할·환경](environment.md), [세션 상태](environment/session-state.md), [Cloud 후속 계획](environment/cloud-plan.md)
- [범위·후속 순서와 과거 종합 기록](synthesis.md)

준비 당시 기존 데이터·백업·SQLx 이력 전환, 실제 Cloud 및 transaction pooler는 NOT RUN이었다.
빈 DB 동등성 PASS는 기존 DB 전환 완료를 뜻하지 않는다. 이번 실행의 상태는 별도 통합 PR에서 보고한다.
과거 작업 분담·진행 경과는 [원래 안내](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/README.md)에 보존한다.
