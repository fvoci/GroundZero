# 새 설치 SQL 기준

제품 스키마의 정본은 이 디렉터리다. [`202610230001_initial_state.sql`](202610230001_initial_state.sql)을
[`db::admin::migrate`](../src/db/admin.rs)의 SQLx migrator가 적용한다.
설치 순서는 `db bootstrap` → `db migrate` → 앱 시작이며 [설치 안내](../../../docs/self-hosting.md#db-setup)를 따른다.

## 기준과 적용 범위

[D-107](../../../docs/decisions.md#d-107)에 따라 기존 누적 SQL 97개(`0001`–`0098`, `0020` 없음)를
새 설치 마이그레이션 1개로 통합했다. [비교 기준 `8ffd8c3b940924eca8cf7ec739a85494243c0479`](https://github.com/fvoci/GroundZero/tree/8ffd8c3b940924eca8cf7ec739a85494243c0479/crates/server/migrations)의
최종 객체 정의를 보존하며, 그 SQL은 #92 후보 생성 기준 `3db81d1deae24b16478581ebd6a06de2b2a2d8c9`와 같다.
과거 SQL 전체와 별도 후보 사본은 현재 작업 트리에 두지 않는다.

새 DB는 버전 `202610230001`의 실제 SQLx 이력으로 시작한다. 기존 이력 행·체크섬을 복사하거나 고쳐서
업그레이드된 것으로 처리하지 않는다. 기존 개발·시험 DB를 자동 이전하는 경로는 없으며,
잘못 연결하면 재생성이 필요하다고 안내하고 중단한다. **대상·백업·데이터 소실 범위를 확인하고 승인받은 뒤에만 재생성한다.**
Supabase의 관리 스키마·기본 역할, 다른 앱 객체, Keycloak 계정, 첨부 저장소와 다른 체크아웃 자원은 자동 삭제 대상이 아니다.

통합 뒤 적용된 SQL은 주석도 체크섬의 일부다. 이후 변경은 기존 파일을 고치지 않고 새 번호로 추가한다.
최종 SQL 파일의 SHA-256·SQLx SHA-384와 검증 소스 SHA는 해당 구현 PR의 결과·증거 링크에서 확인한다.

## 정의 순서와 유지하는 계약

한 SQLx 트랜잭션 안에서 다음 순서로 설치한다.

1. 스키마 소유자와 설치 세션 설정, 테이블 CHECK가 쓰는 순수 도우미.
2. 최종 컬럼·기본값·CHECK를 가진 테이블과 행 타입.
3. 각 함수의 최종 서명·본문·소유자.
4. 기본키·고유 제약·인덱스·트리거·외래 키.
5. RLS·정책·개별 실행/테이블/컬럼 권한.

상호 참조 함수의 본문 검사는 `SET LOCAL check_function_bodies = false`로 설치 트랜잭션 안에서만 늦춘다.
각 SECURITY DEFINER의 `search_path = fvoci, pg_temp`, PUBLIC 실행 회수와 앱 역할의 기존 권한은 유지한다.
`app_local_instant`·`app_normalize_key`의 PUBLIC 실행, FORCE RLS를 쓰지 않는 소유자 함수 경로도 기존 계약이다.
기본 권한은 bootstrap과 PostgreSQL 기본값을 유지하며 설치 뒤 실제 ACL을 비교한다.

끝에서 `search_path = fvoci`로 돌아가 SQLx가 같은 트랜잭션에서 원장을 기록하게 한다.
원장 테이블의 `fvoci baseline 202610230001` 주석은 앱이 원장 SELECT 권한 없이 세대를 확인하기 위한 메타데이터다.
실제 적용 버전·체크섬은 SQLx가 검증한다. 이 주석만 바꿔서 이전 DB를 호환 DB로 만들 수 없다.

[D-100](../../../docs/decisions.md#d-100)의 PG17 최소 지원·검증한 PG18과 환경 공통 SQL,
[D-88](../../../docs/decisions.md#d-88)의 앱 공급 UUIDv7,
[D-08](../../../docs/decisions.md#d-08)의 RLS·앱 권한을 유지한다.
ID 개수·순서·FV001/002 재시도 계약은 [SQL ID 계약](../../../docs/sql-id-contract.md)을 따른다.
새 설치의 필수 초기 데이터는 없고 개발 예제는 기존 seed가 별도로 만든다.

<a id="comparison"></a>

## 읽기와 검증

함수는 이름과 전체 인자로 찾는다. 함수의 이유·예외는 가까운 주석과 안정적인 결정 ID·문서 앵커에 둔다.
옛 구현 번호·개발 절 번호·진행 일지는 새 기준 SQL에 남기지 않는다.

```sh
rg -n '^CREATE FUNCTION fvoci\.app_record_document_references\(' crates/server/migrations
```

기준 뒤에 새 번호로 더한 파일이 같은 함수를 다시 만들 수 있다(예: `202610250001`의 `app_set_display_settings`, D-109).
그때는 번호가 가장 큰 파일의 정의가 지금 정의다. 새 설치 시험은 기준이 먼저이고 디렉터리의 모든 마이그레이션이 적용되는지 본다.

제품 검사는 `cargo xtask check`·`cargo xtask test`를 쓴다. 이번 통합의 전용 비교 도구와 준비 자료는
현재 트리에서 제거했다. 고정 SHA의 누적 SQL과 새 기준을 각각 격리된 빈 DB에 적용해
객체·권한·RLS·초기 상태와 동작을 비교하며, 의도한 SQLx 이력·원장 주석 차이를 구분했다.
빈 DB 동등성은 구버전 백업 복구나 기존 데이터 보존을 증명하지 않는다.

### 고정 검증 소스의 재현과 실행 범위

다음 명령은 [검증 소스 `b7d34936d68b6cc2901c020e5cc26a6098314a54`](https://github.com/fvoci/GroundZero/tree/b7d34936d68b6cc2901c020e5cc26a6098314a54)를
별도로 받은 checkout에서만 실행한다. 도구는 Git에서 과거 SQL을 읽고 실행 SHA를 기록하므로 Git 이력이 없는
압축 해제본으로 실행하지 않는다. 기존 개발 checkout이나 DB를 재생성하지 않는다.

```sh
git clone --no-checkout https://github.com/fvoci/GroundZero.git target/sql-consolidation-reproduce
git -C target/sql-consolidation-reproduce fetch origin b7d34936d68b6cc2901c020e5cc26a6098314a54
git -C target/sql-consolidation-reproduce checkout --detach b7d34936d68b6cc2901c020e5cc26a6098314a54
git -C target/sql-consolidation-reproduce cat-file -e '8ffd8c3b940924eca8cf7ec739a85494243c0479^{commit}'
cd target/sql-consolidation-reproduce
cargo xtask sql-compare --control --output target/sql-compare-aa
cargo xtask sql-compare --postgres-major 17 --output target/sql-compare-pg17 --behavior --oidc
cargo xtask sql-compare --postgres-major 18 --output target/sql-compare-pg18 --behavior --oidc
cargo xtask sql-compare --supabase --output target/sql-compare-supabase --behavior --oidc
```

출력 경로는 실행마다 새로 지정한다. A는 `8ffd8c3`의 실제 SQLx 이력, B는 검증 소스의 제품 `db migrate`를 쓴다.
양쪽은 별도 클러스터·Compose 프로젝트에 설치하고 증거를 저장한 뒤 해당 프로젝트만 `stop`한다.
Supabase 직접 연결과 session pooler를 각각 실행하지만 별도 빈 DB가 필요한 백업·검색 시험은 직접 연결을 쓴다.
A의 일반 API·백업·검색은 과거 이력, 시작·마이그레이션 거부·종료 회귀시험은 별도 새 기준 DB를 사용한다.
위 A/A control은 객체 비교만 실행하며 행동시험은 NOT RUN이다.

카탈로그와 스키마 덤프에서 정의·소유자·ACL·RLS·초기 상태를 비교한다. 함수 본문은 문자열·인용 식별자·
달러 문자열·블록 주석 밖의 전체 줄 `--` 주석만 정규화하며 실행 바이트·인라인 주석·공백은 보존한다.
SQLx 원장 구조는 비교하고 버전·체크섬·세대 COMMENT의 의도한 차이를 별도 확인한다.
OID·덤프 표기 정규화, 변조 탐지·기존 DB 무변경 거부 검사와 추가 제한 관리자 명령은
[검증 당시 안내](https://github.com/fvoci/GroundZero/blob/b7d34936d68b6cc2901c020e5cc26a6098314a54/crates/server/migrations/README.md#comparison)에 있다.
추가 명령의 존재를 최종 SHA에서 실행한 결과로 보지 않는다. 실행 SHA·범위와 PASS/FAIL/NOT RUN은 해당 PR의 증거를 확인한다.

### 과거 자료와 증거

준비 문서·후보·조사 목록은 현재 트리에 유지하지 않는다. [통합 전 main 기록](https://github.com/fvoci/GroundZero/tree/8ffd8c3b940924eca8cf7ec739a85494243c0479/preparation/sql-consolidation)과
[준비 디렉터리 제거 직전 기록](https://github.com/fvoci/GroundZero/tree/6837acc6283efb1c06fd061c6fe2884a95826e9f/preparation/sql-consolidation)은 해당 SHA의 역사다.
기존 원시 증거 270개는 [보존 커밋 `0dfa2b9`](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation)에 있으며,
[SHA-256 목록](https://github.com/fvoci/GroundZero/blob/8ffd8c3b940924eca8cf7ec739a85494243c0479/preparation/sql-consolidation/evidence.sha256)으로 확인한다.
이번 통합의 [최종 검증 증거 `e304e6b`](https://github.com/fvoci/GroundZero/tree/e304e6b24fbd766e443c0eb764c8c519342daf60/evidence/sql-consolidation)는
[SHA-256 목록](https://github.com/fvoci/GroundZero/blob/e304e6b24fbd766e443c0eb764c8c519342daf60/evidence/sql-consolidation.sha256)으로 확인한다.
새 원시 출력은 `target/`에 두고 원격 보존·재현 명령·검증 SHA·체크섬·최종 결과는 해당 PR에 남긴다.
