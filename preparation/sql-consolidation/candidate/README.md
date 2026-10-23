# 통합 후보의 제품 기준 전환

검토 후보는 [제품 마이그레이션](../../../crates/server/migrations/README.md)으로 전환했다.
현재 정본은 `crates/server/migrations/202610230001_initial_state.sql`이며 이 디렉터리에 별도 SQL 사본을 유지하지 않는다.
새 비교는 고정 SHA의 누적 SQL과 현재 제품 경로를 사용한다.

#92에서 검증했던 후보 원문은 [고정 SHA `8ffd8c3`](https://github.com/fvoci/GroundZero/blob/8ffd8c3b940924eca8cf7ec739a85494243c0479/preparation/sql-consolidation/candidate/0001_final_state.sql)에서만 참조한다.
그 후보의 생성 기준은 `3db81d1deae24b16478581ebd6a06de2b2a2d8c9`, SHA-256은
`24a0a3dfaf7b2cf09152a11f80ff2100dd6004990b31cb661bf52a688ae9eea0`이다.
생성 기준과 `8ffd8c3b940924eca8cf7ec739a85494243c0479` 사이의 제품 SQL 차이는 없다.
이 해시는 과거 후보의 바이트를 식별하며 새 제품 SQL의 해시·실행 결과를 대신하지 않는다.

제품 전환에서는 최종 객체 정의를 테이블/행 타입 → 함수 → 제약·인덱스·트리거·외래 키 → RLS·권한 순서로 정리하고,
함수 본문의 실행 SQL은 보존하면서 이력 번호·중복 덤프 표제·대체된 설명을 정리했다.
기존 데이터 재생성은 [D-107](../../../docs/decisions.md#d-107)의 별도 대상·백업·삭제 승인을 따른다.
과거 준비 실행의 범위와 한계는 [검증 결과](../verification/results.md), [증거 위치](../evidence.md)에 남아 있다.
