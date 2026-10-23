# SQL 통합 준비

누적 마이그레이션 교체 전의 검토 후보와 재현 절차를 보존한다. 제품 정본은
`crates/server/migrations/`이며 D-101~D-106 구현, 후보 SQL, 비교 도구와 fixture는 증거 분리에서 바꾸지 않는다.

## 기준과 증거

이 디렉터리의 조사·실행 요약은 **역사 기록이며 현재 상태를 나타내지 않음**이다.
관련 결함이나 SHA를 조사할 때만 원시 증거를 명시적으로 연다. 현재 사용자 요구사항,
승인 결정, 코드·테스트를 기준으로 삼고 서로 충돌하면 보고한다.

- 분리 대상 체크아웃: `abd545a250ff1fd9cc166b3c078c4dff76846724` (#92).
- 후보 생성 기준: `3db81d1deae24b16478581ebd6a06de2b2a2d8c9` (#97의 `b01d74a` 병합).
- 최종 기록된 A/B 실행 소스: `f5b5a9d4a42dc6c87deddddcda0d0ea41f7092f9`.
- 최초 조사 기준: `cb65cc31156c0fd8d28cf856bc132d4a506c8885`. 이 기준의 실행은 정책 반영 후 결과로 대체되었다.
- [보존 위치·SHA-256 목록·분리 검증](evidence.md), [SHA별 최종 결과와 NOT RUN 범위](verification/results.md).

분리 시 #97 최신 `0a26654`와 이미 포함된 `b01d74a` 사이의 추가 변경은 웹 컴포넌트·시험·문서이며
마이그레이션 입력은 바뀌지 않았다. 이번 작업은 SQL이나 동작 변경 및 A/B 재실행을 포함하지 않는다.
#106은 검토 완료·조치 항목 없음인 보존용 PR이며 다시 검토하거나 병합할 대상이 아니다.
#97 → #92의 병합 및 최종 main 상태 판단은 별도 조정 작업이다.

## 유지하는 자료

- [최종 객체·제거/유지 근거](analysis-a.md), [목록 설명과 재생성 명령](inventory/README.md), [테이블 요약](inventory/tables.md)
- [실제 후보 SQL과 적용 조건](candidate/README.md)
- [비교 명령·정규화·검증 한계](verification/README.md), [사전 검사 재현 fixture](verification/fixtures/README.md)
- [기록된 품질 검사 결과](quality-check.md)
- [설치·역할·환경](environment.md), [세션 상태](environment/session-state.md), [Cloud 후속 계획](environment/cloud-plan.md)
- [종합 판단·시험 대응·전환 조건](synthesis.md)

기존 데이터·백업·SQLx 이력 전환, 실제 Cloud 및 transaction pooler는 NOT RUN이다.
빈 DB 동등성 PASS는 기존 DB 전환 완료를 뜻하지 않는다. 실제 마이그레이션 교체는 별도 구현 범위다.
과거 작업 분담·진행 경과는 [원래 안내](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/README.md)에 보존한다.
