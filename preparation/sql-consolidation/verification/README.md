# SQL 통합 준비 검증

실제 `crates/server/migrations/`와 제품 설치 경로는 바꾸지 않는다. 이 도구는 새 설치 후보가 누적 마이그레이션의 최종 상태와 같은지 비교하는 준비용 명령이다.

```sh
# 비교기 대조군: 독립된 빈 클러스터 둘에 현재 누적 마이그레이션 적용
cargo xtask sql-compare --output target/sql-compare-aa

# PostgreSQL 18: 누적 마이그레이션 A / 후보 B + 기존 앱 역할 통합 테스트
cargo xtask sql-compare \
  --candidate preparation/sql-consolidation/candidate \
  --output target/sql-compare-postgres --behavior

# 고정된 셀프호스트 Supabase PostgreSQL 17: 같은 비교 + 직접 연결 및 session pooler
cargo xtask sql-compare \
  --candidate preparation/sql-consolidation/candidate \
  --output target/sql-compare-supabase --supabase --behavior
```

출력 디렉터리는 새 경로여야 한다. 후보는 SQLx 규칙의 `번호_설명.sql` 파일로 구성한다. 양쪽 모두 기존 `db bootstrap`을 실행한다. A에는 기존 `db migrate`, B에는 후보 디렉터리를 읽은 실제 SQLx `Migrator`를 적용한다. B에 누적 이력을 복사하거나 과거 SQL을 재실행하지 않는다. 후보는 SQLx가 같은 트랜잭션에서 `_sqlx_migrations`를 기록할 수 있게 마지막에 `SET LOCAL search_path = fvoci`를 둔다.

Compose 파일·이미지·테스트 자격 증명은 기존 설정을 재사용한다. 프로젝트 이름은 체크아웃 해시와 실행 UUID를 포함하며 A/B/검색·S3가 서로 다르다. 호스트 포트는 동적 루프백 포트다. 종료 시 생성한 프로젝트만 `stop`한다. 컨테이너·이름 붙은 볼륨은 보존하고 `down`·`prune`을 호출하지 않는다. 기존 Compose DB는 tmpfs이므로 **컨테이너를 정지하면 DB의 tmpfs 데이터 자체는 사라진다**. 그 전에 카탈로그·덤프·로그를 출력 경로에 저장한다. 개인 개발 DB 또는 Cloud DB 주소는 받지 않는다.

## 객체 비교

행동 테스트가 fixture를 만들기 전에 양쪽 카탈로그·초기 행·덤프를 읽는다. `catalog.sql`이 다음을 이름 기반 키로 비교한다.

- 앱 함수 전체 정의, 언어, 소유자, 실제 ACL, SECURITY DEFINER, 설정(search_path 포함), volatility/strict/parallel/leakproof/cost/rows.
- 테이블·인덱스·시퀀스·타입 소유자와 ACL, RLS/FORCE RLS, 컬럼 타입·기본값·generated/identity·collation·column ACL, 제약·인덱스 식과 상태.
- RLS USING/WITH CHECK·대상 역할·명령·permissive/restrictive, 일반 트리거와 내부 FK 트리거의 enabled/deferrable/deferred 상태.
- 역할 속성(비밀번호 제외), 멤버십 grantor/admin/inherit/set, 기본 ACL, DB 소유자·ACL·tablespace 이름과 설정, 확장 버전·스키마·소유자.
- 관리 스키마를 포함한 namespace·relation 소유자/ACL/속성, function 정의/소유자/ACL과 event trigger. 관리 스키마의 relation 컬럼·제약·정책 등 전체 DDL을 비교한다는 뜻은 아니다. `uuid_extract_version` 제공자 함수도 포함한다.
- 모든 앱 테이블의 초기 행(빈 테이블 포함), 시퀀스 현재 값/is_called. 초기 행의 시각·ID를 임의로 지우지 않는다.

`pg_dump --schema-only --schema=fvoci --no-comments`를 같은 서버 이미지의 도구로 만들고 객체 블록별로 비교한다. 함수 본문이나 권한 텍스트를 지우지 않는다. SQLx 이력 테이블 **구조도 비교한다**. 그 테이블 행만 별도 `migration-ledger.json`에 기록한다. 후보의 진짜 적용 버전/체크섬과 누적 이력은 의도적으로 다르며 동등하다고 주장하지 않는다. 이력의 실행 시각·소요 시간만 별도 기록에서 제외한다.

정규화 범위는 명시적이다. OID 참조를 이름으로 바꾸며 DB 이름은 `current` 키로 비교한다. 시스템 transaction horizon과 물리 tablespace OID는 비교하지 않고 후자는 이름으로 대신한다. 덤프의 무작위 `\restrict` 토큰·도구 버전 헤더·객체 순서를 제외한다. 객체 COMMENT는 동작 비교 대상이 아니다. 정지 전에 뽑은 원본 덤프도 남긴다.

한 가지 PostgreSQL 재파싱 차이는 `files_name_check`에서 관찰됐다.

```sql
-- 누적 DDL의 덤프
CHECK (((A AND B) AND C))
-- 그 덤프를 다시 적용한 뒤의 덤프
CHECK ((A AND B AND C))
```

여기서 A/B/C는 각각 파일명 길이 하한·상한·금지 문자 검사다. `pg_get_constraintdef(oid, true)`의 결과는 같다. 덤프의 정확한 TABLE 블록 안에서 정확한 이름으로 찾은 단일 행 CHECK만 그 카탈로그 표현으로 대체한다. 다른 괄호·함수 본문·ACL은 정규화하지 않는다. 제약의 의미를 바꾸면 카탈로그와 이 정규 덤프가 달라진다.

각 실행은 B에 정책 추가, 세션 테이블 DELETE 권한 부여, `app_user_id()` 본문 대체, 파일명 CHECK를 `true`로 완화하는 네 변조를 **하나씩** 트랜잭션 안에서 넣는다. 해당 객체 종류에서 차이가 검출돼야 하며, ROLLBACK 뒤 전체 스냅숏이 원래와 같아야 한다. 결과는 `self-test.json`에 남긴다.

## 행동 비교와 한계

`--behavior`는 기존 workspace Rust 시험을 실제 `fvoci_app` 접속으로 양쪽에서 실행한다. 후보 쪽의 `fresh_database` 공통 헬퍼도 같은 후보 SQLx 디렉터리로 DB를 준비한다. 별도 API/권한 구현이나 축약한 모의 SQL 시험으로 대체하지 않는다. CRUD·테넌트 격리·개인/공유 문서·철회·경합/409·반복·시간 기록·검색·백업 등의 기존 검증을 포함한다.

행동 시험의 일반 DB·검색·S3 환경 변수도 격리된 서비스에 고정한다. 기본 파일 저장소는 local이며 S3 시험만 기존 헬퍼로 시험 SeaweedFS를 선택한다. 외부 환경에서 설정한 저장소·검색 URL이 시험에 새어 들어가지 않는다. Zotero 기본 URL은 닫힌 루프백 포트이고 해당 시험이 자체 HTTP fixture로 덮어쓴다.

Supabase에서는 각 DB의 직접 연결과 session pooler를 실행한다. 일반 앱 연결과 `db_context`는 실제 pooler를 사용한다. **새 DB를 만드는 백업·검색 시험은 기존 규칙대로 직접 연결을 사용한다**. 따라서 session 단계 통과를 모든 시험의 pooler 통과로 해석하지 않는다. OIDC Keycloak 시험은 이 명령에서 NOT RUN이며, PostgreSQL 실행의 Supabase 전용 시험도 NOT RUN이다. S3는 기존 SeaweedFS 시험 서비스를 사용한다.

`inputs.json`은 소스 SHA·누적/후보 SQL SHA-256·비교기와 카탈로그 해시·Supabase 고정 revision을 기록한다. `result.json`은 설치·스냅숏·객체 비교·변조 시험과 각 행동 경로의 PASS/FAIL/NOT RUN을 분리한다. `a/`, `b/`의 원본 덤프·카탈로그·이력·행동 로그가 근거다. 실패한 실행도 별도 출력 경로로 보존한다.

새 설치 동등성은 기존 DB의 이력 교체, 기존 백업의 새 버전 복구, 데이터 업그레이드, Supabase Cloud 실행을 증명하지 않는다. 특히 후보 v1과 누적 이력의 마지막 버전이 다르므로 기존 백업 호환 여부는 별도 설계·승인이 필요하다.
