//! 개발 실행. Compose를 띄우고 역할·마이그레이션을 준비한 뒤 웹(Vite)과 서버를 실행한다.
//!
//! - `--oidc`: 개발용 Keycloak을 함께 띄우고 서버에 `FVOCI_OIDC_*`를 넣는다. 테스트 realm의 가짜 사용자로 바로
//!   로그인할 수 있다: `test-user`는 초기 관리자(D-51)로 '개발' 워크스페이스의 소유자, `test-user-2`는 그 워크스페이스의
//!   작성자. 둘이 함께 쓰는 실시간 공동 편집 문서도 한 번만 만든다(`deploy/postgres/dev-seed.sql`). 이어서 둘이
//!   함께 쓰는 별도 합성 워크스페이스에 다섯 모듈의 예제를 한 번만 만든다(`deploy/postgres/dev-module-seed.sql`).
//!   마지막으로 Storybook과 같은 합성 데이터(`deploy/postgres/dev-ui-data.json`)로 "UI 상태 예제" 워크스페이스를
//!   한 번만 만든다(`deploy/postgres/dev-ui-seed.sql`). `cargo xtask test`가 같은 경로를 새 DB에 두 번 돌려
//!   본다([`check_seeds`]).
//! - `--no-web`: 웹 개발 서버를 띄우지 않는다(API만). 기본은 `pnpm --filter @fvoci/web dev`를 함께 띄운다.
//! - 공개 URL 기본값은 Vite 원점 `http://127.0.0.1:5173`이며 `FVOCI_PUBLIC_URL`로 바꾼다. 쿠키·리다이렉트가 이
//!   원점에 맞춰진다.

use std::fmt::Write as _;
use std::path::Path;
use std::process::{Child, Command};

use anyhow::{Context, Result, bail};
use xshell::{Shell, cmd};

use crate::compose::{self, Compose, DbUrls, Env};

pub fn run(root: &Path, oidc: bool, web: bool) -> Result<()> {
    let mut compose = Compose::new(root, Env::Dev);
    if oidc {
        compose = compose.with_profile("oidc");
    }
    let sh = compose.shell()?;
    println!("==> compose up ({})", compose.project());
    compose.up(&sh)?;

    let pg_port = compose.host_port(&sh, "postgres", 5432)?;
    let meili_port = compose.host_port(&sh, "meilisearch", 7700)?;
    let urls = DbUrls::local(pg_port);
    compose::prepare_database(&sh, &urls)?;

    let bind = std::env::var("FVOCI_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let public =
        std::env::var("FVOCI_PUBLIC_URL").unwrap_or_else(|_| "http://127.0.0.1:5173".to_owned());

    println!("postgres     127.0.0.1:{pg_port}");
    println!("meilisearch  127.0.0.1:{meili_port}");
    println!("server       {bind}");
    println!("open         {public}");
    println!(
        "stop         Ctrl-C (data stays; `cargo xtask down` removes this checkout's containers and volumes; attachments in target/dev-files stay)"
    );

    let _db = sh.push_env("DATABASE_URL", &urls.app);
    let _bind = sh.push_env("FVOCI_BIND", &bind);
    let _public = sh.push_env("FVOCI_PUBLIC_URL", &public);
    let _search = sh.push_env("FVOCI_SEARCH_URL", format!("http://127.0.0.1:{meili_port}"));
    let _search_key = sh.push_env("FVOCI_SEARCH_KEY", compose::SEARCH_TEST_KEY);
    // 개발 중에는 검색 색인·반복 생성이 빨리 보이도록 짧게.
    let interval = std::env::var("FVOCI_WORKER_INTERVAL_SECS").unwrap_or_else(|_| "5".to_owned());
    let _interval = sh.push_env("FVOCI_WORKER_INTERVAL_SECS", interval);
    // 개발 파일은 빌드 산출물 옆에 둔다(저장소에 섞이지 않게).
    let files =
        std::env::var("FVOCI_STORAGE_DIR").unwrap_or_else(|_| "target/dev-files".to_owned());
    let _files = sh.push_env("FVOCI_STORAGE_DIR", files);
    let _oidc = if oidc {
        let kc = compose.host_port(&sh, "keycloak", 8080)?;
        let issuer = issuer(kc);
        check_realm_users(root, &compose, &sh)?;
        seed(root, &compose, &sh, &issuer, DEV_DATABASE)?;
        println!("keycloak     {issuer}");
        // 비밀번호는 로그·코드에 두지 않는다. 값은 deploy/keycloak/realm-test.json의 테스트 계정에 있다.
        println!(
            "login        test-user (admin), test-user-2 — passwords in deploy/keycloak/realm-test.json"
        );
        let admin = std::env::var("FVOCI_INITIAL_ADMIN_SUBJECT")
            .unwrap_or_else(|_| compose::DEV_ADMIN_SUBJECT.to_owned());
        Some((
            sh.push_env("FVOCI_OIDC_ISSUER", issuer),
            sh.push_env("FVOCI_OIDC_CLIENT_ID", compose::OIDC_CLIENT_ID),
            sh.push_env("FVOCI_OIDC_CLIENT_SECRET", compose::OIDC_CLIENT_SECRET),
            sh.push_env("FVOCI_INITIAL_ADMIN_SUBJECT", admin),
        ))
    } else {
        None
    };
    let _web = if web {
        Some(WebServer::start(root, &bind)?)
    } else {
        println!(
            "web          (not started) VITE_API_URL=http://{bind} pnpm --filter @fvoci/web dev"
        );
        None
    };
    cmd!(sh, "cargo run --package fvoci-server --locked").run()?;
    Ok(())
}

/// 개발 Keycloak 볼륨에 예전 테스트 realm이 남아(Keycloak은 realm이 이미 있으면 파일을 다시 들이지 않는다) 테스트
/// 사용자 id가 realm 파일의 고정 id와 다르면, 로그인한 사람이 준비한 계정과 다른 사용자가 되어 준비한 워크스페이스에
/// 들어가지 못한다. 그때는 알리고 멈춘다(볼륨을 저절로 지우지 않는다). 확인 자체를 못 하면 알리고 계속한다.
fn check_realm_users(root: &Path, compose: &Compose, sh: &Shell) -> Result<()> {
    let path = root.join("deploy/keycloak/realm-test.json");
    let realm: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
    )?;
    // 관리자 비밀번호는 컨테이너의 환경 변수로만 쓴다(코드·로그에 두지 않는다).
    let script = format!(
        "/opt/keycloak/bin/kcadm.sh config credentials --config /tmp/fvoci-kcadm.config \
         --server http://localhost:8080 --realm master --user \"$KC_BOOTSTRAP_ADMIN_USERNAME\" \
         --password \"$KC_BOOTSTRAP_ADMIN_PASSWORD\" >/dev/null 2>&1 && \
         /opt/keycloak/bin/kcadm.sh get users --config /tmp/fvoci-kcadm.config -r {} --fields id,username",
        compose::OIDC_REALM
    );
    let command = ["sh".to_owned(), "-c".to_owned(), script];
    let users: Vec<serde_json::Value> = match compose
        .exec_read(sh, "keycloak", &command)
        .and_then(|out| serde_json::from_str(&out).context("read the Keycloak user list"))
    {
        Ok(users) => users,
        Err(err) => {
            println!("warning: could not check the dev Keycloak test users ({err:#}); continuing");
            return Ok(());
        }
    };
    for user in realm["users"].as_array().into_iter().flatten() {
        let (Some(name), Some(want)) = (user["username"].as_str(), user["id"].as_str()) else {
            continue;
        };
        let found = users
            .iter()
            .find(|u| u["username"] == name)
            .and_then(|u| u["id"].as_str());
        if found != Some(want) {
            let found = found.map_or_else(|| "missing".to_owned(), |id| format!("id {id}"));
            bail!(
                "the dev Keycloak keeps an older test realm: {name} should be id {want} but is {found}. \
                 Preserve the Keycloak accounts and database, then inspect the original realm and issuer configuration. \
                 Any reset requires confirmed targets, backups and approval; see docs/self-hosting.md#sql-baseline-reset"
            );
        }
    }
    Ok(())
}

/// 개발 Compose의 DB 이름(`compose.dev.yaml`의 `POSTGRES_DB`).
const DEV_DATABASE: &str = "fvoci";

/// 개발 Keycloak의 테스트 realm 발급자. 사용자 식별은 발급자 + subject라 포트도 그 일부다.
fn issuer(keycloak_port: u16) -> String {
    format!(
        "http://127.0.0.1:{keycloak_port}/realms/{}",
        compose::OIDC_REALM
    )
}

/// 개발 전용 사용자·공동 작업 공간·공동 편집 문서를 한 번만 만든다(이미 있으면 아무것도 바꾸지 않음).
fn seed(root: &Path, compose: &Compose, sh: &Shell, issuer: &str, database: &str) -> Result<()> {
    println!("==> dev seed (shared workspace and document, once)");
    seed_file(root, compose, sh, issuer, database, "dev-seed.sql", None)?;
    // 모듈 예제는 자기 표시(dev_module_seed_v1)로 따로 한 번만 만든다. 이미 dev_seed가 있는 개발 DB에도 더해진다.
    println!("==> dev module examples (separate synthetic workspace, once)");
    seed_file(
        root,
        compose,
        sh,
        issuer,
        database,
        "dev-module-seed.sql",
        None,
    )?;
    // Storybook story와 같은 내용(apps/web/stories/fixtures/synthetic.ts). 묶음마다 자기 표시(dev_ui_seed_v1·v2)로 한 번만.
    println!("==> dev UI examples (same synthetic data as Storybook, once)");
    let path = root.join("deploy/postgres/dev-ui-data.json");
    let data =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    // 형식이 틀리면 DB에 보내기 전에 멈춘다. 세션 설정으로 넘겨 dev-ui-seed.sql이 읽는다.
    let data = serde_json::from_str::<serde_json::Value>(&data)
        .with_context(|| format!("parse {}", path.display()))?
        .to_string();
    seed_file(
        root,
        compose,
        sh,
        issuer,
        database,
        "dev-ui-seed.sql",
        Some(("fvoci.dev_ui_data", &data)),
    )
}

/// `compose.dev.yaml`의 `FVOCI_DEV_KEYCLOAK_PORT` 기본값. 시험은 Keycloak 없이 이 발급자로 준비 SQL을 돌린다(준비 SQL은
/// 발급자를 문자열로 비교·저장만 하고 사용자 id는 `deploy/keycloak/realm-test.json`의 고정 subject를 쓴다).
const DEV_KEYCLOAK_PORT: u16 = 8180;

/// 개발 준비 SQL 셋이 기준 SQL을 막 설치한 새 DB에 끝까지 적용되고, 두 번째 적용은 실패하지 않고 아무것도 바꾸지
/// 않는지 본다(`cargo xtask test`, #168). 개발 실행과 같은 [`seed`]를 거친다(파일 목록·순서, `dev-ids.sql` 앞붙임,
/// 세션 설정, psql 옵션). `compose`는 PostgreSQL 서비스가 뜬 시험 프로젝트이고, DB는 그 서버에 따로 만들어 통합
/// 시험의 DB를 건드리지 않는다(준비 SQL이 초기 관리자를 정한다). 시험 DB는 tmpfs라 프로젝트를 내릴 때 함께 사라진다.
/// 하위 단계(새 DB, 기준 SQL, 지문 조회, 첫·둘째 적용) 사이마다 중단 신호를 확인해 남은 단계를 건너뛴다.
pub fn check_seeds(root: &Path, compose: &Compose, sh: &Shell, pg_port: u16) -> Result<()> {
    check_seeds_with(root, compose, sh, |database| {
        compose::prepare_database(
            sh,
            &compose::DbUrls::at_database(&format!("127.0.0.1:{pg_port}"), database),
        )
    })
}

/// 같은 준비 SQL 검사를 실행하되 새 DB의 역할·기준 SQL 준비는 호출자가 맡는다.
/// 컨테이너 시험 실행기는 게시 포트 없이 Compose 네트워크 안에서 DB를 준비한다.
pub fn check_seeds_with(
    root: &Path,
    compose: &Compose,
    sh: &Shell,
    prepare_database: impl FnOnce(&str) -> Result<()>,
) -> Result<()> {
    let database = format!("fvoci_dev_seed_{}", uuid::Uuid::now_v7().simple());
    println!("==> dev seed check on a new database {database}");
    compose.exec(
        sh,
        "postgres",
        &psql(
            DEV_DATABASE,
            &["-c", &format!("CREATE DATABASE {database}")],
        ),
        "",
    )?;
    compose::check_interrupt()?;
    prepare_database(&database)?;
    compose::check_interrupt()?;
    let issuer = issuer(DEV_KEYCLOAK_PORT);
    let empty = table_digests(compose, sh, &database)?;
    compose::check_interrupt()?;
    seed(root, compose, sh, &issuer, &database)?;
    compose::check_interrupt()?;
    let first = table_digests(compose, sh, &database)?;
    if first == empty {
        bail!("dev seed check: the dev seeds changed nothing on a new database");
    }
    compose::check_interrupt()?;
    println!("==> dev seeds again (must change nothing)");
    seed(root, compose, sh, &issuer, &database)?;
    compose::check_interrupt()?;
    let second = table_digests(compose, sh, &database)?;
    if second != first {
        let changed: Vec<&str> = second
            .lines()
            .filter(|line| !first.lines().any(|l| l == *line))
            .collect();
        bail!(
            "dev seed check: the second apply changed tables (table rows md5): {}",
            changed.join("; ")
        );
    }
    println!("==> dev seed check passed (second apply changed nothing)");
    Ok(())
}

/// 사용자 스키마의 표마다 `이름 행수 md5`(행 글자열을 정렬해 이은 것). 행의 물리 순서와 상관없이 내용이 같으면 같다.
/// 관리자(superuser)로 읽어 RLS와 상관없이 모든 행을 센다.
fn table_digests(compose: &Compose, sh: &Shell, database: &str) -> Result<String> {
    const QUERY: &str = "SELECT format('%I.%I', n.nspname, c.relname) || ' ' \
        || (xpath('/row/d/text()', query_to_xml(format( \
             'SELECT count(*) || '' '' || md5(coalesce(string_agg(t::text, chr(10) ORDER BY t::text), '''')) AS d FROM %I.%I t', \
             n.nspname, c.relname), false, true, '')))[1]::text \
        FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
        WHERE c.relkind = 'r' AND n.nspname NOT LIKE 'pg\\_%' AND n.nspname <> 'information_schema' \
        ORDER BY 1";
    compose.exec_read(sh, "postgres", &psql(database, &["-At", "-c", QUERY]))
}

/// PostgreSQL 컨테이너 안에서 관리자(`fvoci`)로 도는 psql 명령. 개발 실행과 그 시험이 같은 옵션을 쓴다.
fn psql(database: &str, args: &[&str]) -> Vec<String> {
    [
        "psql",
        "--quiet",
        "--no-psqlrc",
        "-U",
        "fvoci",
        "-d",
        database,
        "-v",
        "ON_ERROR_STOP=1",
    ]
    .iter()
    .chain(args)
    .map(|s| (*s).to_owned())
    .collect()
}

/// 준비 SQL에 넘기는 값의 달러 따옴표. 값에 이 글자열이 있으면 넘기지 않는다.
const SEED_QUOTE: &str = "$fvoci_seed$";

/// 준비 SQL 하나가 쓸 수 있는 새 행 id 수(넉넉히). 모자라면 준비 SQL이 멈춘다.
const SEED_IDS: usize = 1000;

/// `setting`은 준비 SQL 앞에서 정할 세션 설정 하나(이름, 값). 명령줄이 아니라 표준 입력으로 넘겨 실행 기록에 값이
/// 찍히지 않게 한다.
fn seed_file(
    root: &Path,
    compose: &Compose,
    sh: &Shell,
    issuer: &str,
    database: &str,
    name: &str,
    setting: Option<(&str, &str)>,
) -> Result<()> {
    let dir = root.join("deploy/postgres");
    let read = |file: &str| {
        let path = dir.join(file);
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
    };
    // 새 행 id는 DB가 아니라 여기서 만든다(D-88): UUIDv7 목록을 세션 설정으로 넘기고, dev-ids.sql이 꺼내 쓰게 한다.
    let ids = (0..SEED_IDS)
        .map(|_| uuid::Uuid::now_v7().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut sql = format!("SELECT set_config('fvoci.dev_ids', '{{{ids}}}', false);\n");
    if let Some((key, value)) = setting {
        if value.contains(SEED_QUOTE) {
            bail!("{name}: the value for {key} contains {SEED_QUOTE}");
        }
        // SET은 결과 행을 찍지 않는다(SELECT set_config는 값 전체를 출력한다).
        let _ignored = writeln!(sql, "SET {key} = {SEED_QUOTE}{value}{SEED_QUOTE};");
    }
    let _ignored = write!(sql, "{}\n{}", read("dev-ids.sql")?, read(name)?);
    let command = psql(database, &["-v", &format!("issuer={issuer}"), "-f", "-"]);
    compose.exec(sh, "postgres", &command, &sql)
}

/// 웹 개발 서버. 같은 프로세스 그룹이라 Ctrl-C가 함께 멈춘다. 서버가 먼저 끝나면 여기서 멈춘다.
struct WebServer(Child);

impl WebServer {
    fn start(root: &Path, bind: &str) -> Result<Self> {
        let child = Command::new("pnpm")
            .args(["--filter", "@fvoci/web", "dev"])
            .env("VITE_API_URL", format!("http://{bind}"))
            .current_dir(root)
            .spawn()
            .context("start the web dev server (pnpm --filter @fvoci/web dev)")?;
        Ok(Self(child))
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        // pnpm이 띄운 vite까지 멈춘다.
        // 이미 끝났으면(Ctrl-C) 실패해도 괜찮으므로 알리기만 한다.
        let pid = self.0.id().to_string();
        if let Err(e) = Command::new("pkill").args(["-TERM", "-P", &pid]).status() {
            eprintln!("web dev server: pkill: {e}");
        }
        if let Err(e) = self.0.kill() {
            eprintln!("web dev server: kill: {e}");
        }
        if let Err(e) = self.0.wait() {
            eprintln!("web dev server: wait: {e}");
        }
    }
}
