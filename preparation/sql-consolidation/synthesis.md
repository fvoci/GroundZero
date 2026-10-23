# SQL 통합 준비의 범위와 후속

제품 정본은 `crates/server/migrations/`다. 이 디렉터리의 후보는 빈 DB의
새 설치 동등성을 검토하기 위한 것이며, 실제 마이그레이션 교체나 기존 DB 전환을 수행하지 않는다.

후보의 생성 기준·체크섬·적용 순서는 [후보 안내](candidate/README.md)를,
승인 정책은 [결정 기록](../../docs/decisions.md)을 따른다.
지원 환경과 설치 조건은 [설치 안내](../../docs/self-hosting.md)와
[환경 공통 계약](environment/common-postgres.md)을 함께 본다.

## 실제 전환 순서

1. 최종 적용 대상의 main·관련 PR SHA와 후보 생성 기준 사이의 변경을 확인한다.
   SQL·호출자·seed·설치·권한 계약에 영향이 있으면 후보와 대응 시험을 갱신한다.
2. 대상 SHA에 맞는 객체 비교와 앱 역할 동작 시험의 결과를 각각 확인한다.
   한쪽의 성공으로 다른 쪽을 대신하지 않는다. 명령과 범위는
   [검증 안내](verification/README.md)를 따른다.
3. 새 설치 검증과 별개로 기존 데이터 보존·이관 및 백업·SQLx 이력 전환 방식을 결정한다.
   기존 DB를 자동 초기화하지 않는다.
4. 별도 구현 PR에서 실제 마이그레이션 경로 교체와 설치 문서 전환을 검토한다.

이 순서는 기존 전환 조건을 연결한 안내다. 설치 역할 보완 제안(E1–E3)을 새 차단 조건으로
삼거나, 로컬 제한 권한 결과를 실제 Cloud의 설치·복구 결과로 확대하지 않는다.

## 기록된 검증과 과거 종합

[DB 실행 결과](verification/results.md)와 [품질 검사](quality-check.md)는
기록된 SHA의 결과다. 현재 후보나 제품의 검증 완료를 자동으로 뜻하지 않는다.
기존 데이터·백업·SQLx 이력 전환과 실제 Cloud의 미검증 범위도 각 기록을 따른다.

과거 행렬·작업자 교차 검토·시험 대응·당시 PR 상태는
[보존된 종합 기록](https://github.com/fvoci/GroundZero/blob/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/synthesis.md)에 있다.
필요한 결함이나 SHA를 조사할 때만 열며, 보존 위치와 무결성은 [증거 안내](evidence.md)를 따른다.
