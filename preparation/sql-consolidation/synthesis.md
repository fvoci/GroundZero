# SQL 통합 범위와 준비 기록

제품 정본은 `crates/server/migrations/`다. #92의 최종 상태 후보를 실제 초기 마이그레이션으로 전환한다.
비교 기준은 main `8ffd8c3b940924eca8cf7ec739a85494243c0479`의 누적 SQL이다.
후보 생성 기준 `3db81d1deae24b16478581ebd6a06de2b2a2d8c9` 이후 실제 SQL 변경은 없었다.
97개 누적 파일(최대 버전 98)을 `202610230001_initial_state.sql` 하나로 정리한다.
준비 디렉터리에 별도 SQL 사본을 유지하지 않는다.

새 기준·적용 순서는 [제품 마이그레이션 안내](../../crates/server/migrations/README.md)를,
승인 정책은 [결정 기록](../../docs/decisions.md)을 따른다.
지원 환경과 설치 조건은 [설치 안내](../../docs/self-hosting.md)와
[환경 공통 계약](environment/common-postgres.md)을 함께 본다.

## 실제 전환 순서

1. 최종 적용 대상의 main·관련 PR SHA와 후보 생성 기준 사이의 변경을 확인한다.
   SQL·호출자·seed·설치·권한 계약에 영향이 있으면 후보와 대응 시험을 갱신한다.
2. 대상 SHA에 맞는 객체 비교와 앱 역할 동작 시험의 결과를 각각 확인한다.
   한쪽의 성공으로 다른 쪽을 대신하지 않는다. 명령과 범위는
   [검증 안내](verification/README.md)를 따른다.
3. [D-107](../../docs/decisions.md#d-107)의 확정 방향은 로컬 개발·시험 및 Cloud 테스트 DB 재생성이다.
   자동 이전이나 SQLx 이력·체크섬 덮어쓰기는 제공하지 않는다. 기존 DB를 열면 재생성 필요를 알리고 멈춘다.
4. 실제 삭제는 연결·소유·공유 관계, 정확한 대상·소실 데이터, 백업 여부·위치·확인 방법·복구 한계를
   제시하고 최종 승인을 받은 다음 수행한다. 범위를 확정하지 못한 자원은 제외한다.
   Cloud 관리 스키마·기본 역할·프로젝트, 다른 앱과 checkout, Keycloak 계정·첨부를 자동 포함하지 않는다.
   검색 색인 정리는 승인 범위에 따로 넣는다. [재생성 안내](../../docs/self-hosting.md#sql-baseline-reset)를 따른다.
5. 별도 Draft PR에서 실제 마이그레이션 교체와 검증 결과를 보고한다. main 병합·태그·배포는 포함하지 않는다.

빈 DB 객체 동등성·앱 역할·OIDC 검증과 실제 Cloud 재생성·검증은 따로 보고한다.
개발 seed는 새 예제 데이터다. 구버전 백업 복구를 시험하지 않았다면 NOT RUN으로 남긴다.

이 순서는 기존 전환 조건을 연결한 안내다. 설치 역할 보완 제안(E1–E3)을 새 차단 조건으로
삼거나, 로컬 제한 권한 결과를 실제 Cloud의 설치·복구 결과로 확대하지 않는다.

## 기록된 검증과 과거 종합

[DB 실행 결과](verification/results.md)와 [품질 검사](quality-check.md)는
기록된 SHA의 결과다. 현재 후보나 제품의 검증 완료를 자동으로 뜻하지 않는다.
기존 데이터·백업·SQLx 이력 전환과 실제 Cloud의 미검증 범위도 각 기록을 따른다.

과거 행렬·작업자 교차 검토·시험 대응·당시 PR 상태는
[보존된 종합 기록](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/synthesis.md)에 있다.
필요한 결함이나 SHA를 조사할 때만 열며, 보존 위치와 무결성은 [증거 안내](evidence.md)를 따른다.
