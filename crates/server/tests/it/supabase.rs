#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 셀프호스트 Supabase에서만 도는 확인(D-88, `cargo xtask test --supabase`). 이름이 `supabase_`로 시작하며 다른 구성에서는
//! xtask가 건너뛰고 NOT RUN으로 알린다.
//!
//! GroundZero는 Supabase의 PostgreSQL만 쓴다. Data API(PostgREST)·Auth의 역할(`anon`·`authenticated`·`service_role`·
//! `authenticator`)은 fvoci 스키마에 닿을 수 없고, RLS는 Supabase `auth.uid()`가 아니라 `app.user_id`를 본다.

use sqlx::Connection;

const SUPABASE_ROLES: [&str; 4] = ["anon", "authenticated", "service_role", "authenticator"];

async fn admin() -> sqlx::PgConnection {
    let url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test --supabase`");
    sqlx::PgConnection::connect(&url).await.unwrap()
}

/// 시험한 관리자 연결이 정말 superuser가 아닌 Supabase `postgres`인지(그 경로로 bootstrap·복구를 시험했다는 근거).
#[tokio::test]
async fn supabase_admin_is_the_non_superuser_postgres_role() {
    let mut conn = admin().await;
    let (name, superuser, create_role): (String, bool, bool) = sqlx::query_as(
        "SELECT rolname::text, rolsuper, rolcreaterole FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&mut conn)
    .await
    .unwrap();
    assert_eq!(name, "postgres");
    assert!(!superuser);
    assert!(create_role);
    let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert!(
        (170_000..180_000).contains(&version),
        "the official image runs PostgreSQL 17, got {version}"
    );
}

/// CI의 직접 연결·session pooler 단계에서 실제 관리자 자격으로 같은 잠금 도우미를 부른다.
/// pooler 단계는 기존 앱 주소의 host/port/tenant와 기존 관리자 자격만 조합한다(새 자격 증명 없음).
#[tokio::test]
async fn supabase_migrator_lock_preserves_the_backend_session() {
    use fvoci_core::secret::Secret;
    use fvoci_db::admin::{MIGRATOR_LOCK, lock_migrator, unlock_migrator};

    let admin_url = url::Url::parse(&std::env::var("FVOCI_TEST_ADMIN_URL").unwrap()).unwrap();
    let mut endpoint = url::Url::parse(&std::env::var("DATABASE_URL").unwrap()).unwrap();
    if std::env::var_os("FVOCI_TEST_DIRECT_APP_URL").is_some() {
        let (_, tenant) = endpoint
            .username()
            .split_once('.')
            .expect("pooler tenant suffix");
        let username = format!("{}.{tenant}", admin_url.username());
        endpoint.set_username(&username).unwrap();
        endpoint.set_password(admin_url.password()).unwrap();
    } else {
        endpoint = admin_url;
    }
    // 대상 경로가 무엇이든 잠금 연결은 postgres로 고정해야 한다.
    endpoint.set_path("/ignored_restore_target");
    let secret = Secret::new(endpoint.to_string());
    let mut first = lock_migrator(&secret).await.unwrap();
    let (pid, database): (i32, String) =
        sqlx::query_as("SELECT pg_backend_pid(), current_database()::text")
            .fetch_one(&mut first)
            .await
            .unwrap();
    assert_eq!(database, "postgres");
    let mut inspector = admin().await;
    for _ in 0..3 {
        let mut tx = first.begin().await.unwrap();
        let current: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            current, pid,
            "the pooler changed the locked backend session"
        );
        tx.commit().await.unwrap();
        let held: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE pid = $1 AND locktype = 'advisory'
               AND granted AND ((classid::bigint << 32) | objid::bigint) = $2)",
        )
        .bind(pid)
        .bind(MIGRATOR_LOCK)
        .fetch_one(&mut inspector)
        .await
        .unwrap();
        assert!(held, "commit released the session advisory lock");
    }

    let release =
        async {
            loop {
                let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype = 'advisory' AND NOT granted
                   AND ((classid::bigint << 32) | objid::bigint) = $1
                   AND $2 = ANY(pg_blocking_pids(pid)))",
            ).bind(MIGRATOR_LOCK).bind(pid).fetch_one(&mut inspector).await.unwrap();
                if blocked {
                    break;
                }
                tokio::task::yield_now().await;
            }
            unlock_migrator(first).await.unwrap();
        };
    let (second, ()) = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        tokio::join!(lock_migrator(&secret), release)
    })
    .await
    .expect("closing the pooler client did not hand off the advisory lock");
    unlock_migrator(second.unwrap()).await.unwrap();
    // pooler의 비동기 세션 정리까지 관찰하고, 시험 연결이 얻은 잠금도 바로 반납한다.
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
                .bind(MIGRATOR_LOCK)
                .fetch_one(&mut inspector)
                .await
                .unwrap();
            if got {
                sqlx::query("SELECT pg_advisory_unlock($1)")
                    .bind(MIGRATOR_LOCK)
                    .execute(&mut inspector)
                    .await
                    .unwrap();
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("pooler sessions left the migrator lock behind");
}

/// Supabase 역할은 fvoci 스키마·표·함수에 권한이 없고, GroundZero 역할은 Supabase 역할의 멤버가 아니다.
#[tokio::test]
async fn supabase_roles_cannot_reach_the_fvoci_schema() {
    let mut conn = admin().await;
    for role in SUPABASE_ROLES {
        let usage: bool = sqlx::query_scalar("SELECT has_schema_privilege($1, 'fvoci', 'USAGE')")
            .bind(role)
            .fetch_one(&mut conn)
            .await
            .unwrap();
        assert!(!usage, "{role} has USAGE on fvoci");
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT c.relname::text FROM pg_class c
             WHERE c.relnamespace = 'fvoci'::regnamespace AND c.relkind IN ('r', 'v', 'm', 'p')
               AND has_table_privilege($1, c.oid, 'SELECT, INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER')",
        )
        .bind(role)
        .fetch_all(&mut conn)
        .await
        .unwrap();
        assert!(tables.is_empty(), "{role} has privileges on {tables:?}");
        // 유효 권한(PUBLIC·상속·기본 권한 포함). 정의자 함수는 하나도 실행할 수 없어야 한다. PUBLIC에 열린 순수
        // 도우미(정의자 아님)가 있어도 스키마 USAGE가 없어 부를 수 없다(아래에서 실제로 불러 본다).
        let definers: Vec<String> = sqlx::query_scalar(
            "SELECT p.oid::regprocedure::text FROM pg_proc p
             WHERE p.pronamespace = 'fvoci'::regnamespace AND p.prosecdef
               AND has_function_privilege($1, p.oid, 'EXECUTE')",
        )
        .bind(role)
        .fetch_all(&mut conn)
        .await
        .unwrap();
        assert!(definers.is_empty(), "{role} may execute {definers:?}");
        let can_set: bool = sqlx::query_scalar("SELECT pg_has_role(current_user, $1, 'SET')")
            .bind(role)
            .fetch_one(&mut conn)
            .await
            .unwrap();
        if can_set {
            for probe in [
                "SELECT fvoci.app_normalize_key('x')",
                "SELECT count(*) FROM fvoci.documents",
            ] {
                let mut tx = conn.begin().await.unwrap();
                sqlx::raw_sql(&format!("SET LOCAL ROLE {role}"))
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                let err = sqlx::query(probe).execute(&mut *tx).await.unwrap_err();
                let code = err
                    .as_database_error()
                    .and_then(|e| e.code())
                    .map(|c| c.into_owned());
                assert_eq!(code.as_deref(), Some("42501"), "{role}: {probe}: {err}");
                tx.rollback().await.unwrap();
            }
        }
    }
    let memberships: Vec<String> = sqlx::query_scalar(
        "SELECT r.rolname || ' in ' || g.rolname FROM pg_auth_members m
         JOIN pg_roles r ON r.oid = m.member JOIN pg_roles g ON g.oid = m.roleid
         WHERE r.rolname IN ('fvoci_app', 'fvoci_migrator')",
    )
    .fetch_all(&mut conn)
    .await
    .unwrap();
    assert!(
        memberships.is_empty(),
        "GroundZero roles inherit from {memberships:?}"
    );
    let auth_policies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_policies WHERE schemaname = 'fvoci'
           AND (coalesce(qual, '') ILIKE '%auth.%' OR coalesce(with_check, '') ILIKE '%auth.%')",
    )
    .fetch_one(&mut conn)
    .await
    .unwrap();
    assert_eq!(auth_policies, 0, "RLS must not use Supabase auth.*");
}

/// Data API(PostgREST)는 fvoci 스키마를 내놓지 않는다(노출 스키마에 넣지 않았다).
#[tokio::test]
async fn supabase_data_api_does_not_expose_fvoci() {
    let rest = std::env::var("FVOCI_TEST_SUPABASE_REST_URL")
        .expect("run via `cargo xtask test --supabase`");
    let client = reqwest::Client::new();
    let profiled = client
        .get(format!("{rest}/workspaces"))
        .header("Accept-Profile", "fvoci")
        .send()
        .await
        .unwrap();
    let status = profiled.status();
    let body: serde_json::Value = profiled.json().await.unwrap();
    assert_eq!(status.as_u16(), 406, "{body}");
    assert_eq!(body["code"], "PGRST106", "{body}");
    let plain = client
        .get(format!("{rest}/workspaces"))
        .send()
        .await
        .unwrap();
    assert_eq!(plain.status().as_u16(), 404);
}
