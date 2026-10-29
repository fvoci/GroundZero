//! 닫힌 보류 상태의 제한 조회·409·대리 저장 귀속(D-112·D-95·D-99).

use super::*;

pub(super) async fn closed_pending(f: &Fixture) {
    let mut writer = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(writer.edit_and_wait(PENDING).await);
    let mut conn = common::fixture_conn().await;
    sqlx::query("UPDATE fvoci.workspace_members SET role='readonly' WHERE workspace_id=$1::uuid AND user_id=$2")
        .bind(&f.workspace).bind(f.writer.id).execute(&mut conn).await.unwrap();
    writer.close().await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while f
            .state
            .collab
            .has_room(DocumentId(f.document.parse().unwrap()))
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("room did not close");
    sqlx::query("UPDATE fvoci.workspace_members SET role='member' WHERE workspace_id=$1::uuid AND user_id=$2")
        .bind(&f.workspace).bind(f.writer.id).execute(&mut conn).await.unwrap();
    assert!(f.pending().await);
}

pub(super) async fn stored(f: &Fixture) -> Value {
    let mut conn = common::fixture_conn().await;
    sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM fvoci.document_collab_states s WHERE document_id=$1::uuid",
    )
    .bind(&f.document)
    .fetch_one(&mut conn)
    .await
    .unwrap()
}

#[tokio::test]
async fn pending_lookup_checks_command_authority_and_returns_only_ids() {
    let f = Fixture::new().await;
    closed_pending(&f).await;
    let outsider = common::login(&f.state, "outsider").await;
    let mut denied = f.state.db.begin_as(outsider.id).await.unwrap();
    let error = sqlx::query("SELECT * FROM app_collab_command_pending('workspace_remove', $1, $2)")
        .bind(f.workspace.parse::<uuid::Uuid>().unwrap())
        .bind(f.writer.id)
        .fetch_all(denied.conn())
        .await
        .expect_err("outsider discovered pending documents");
    assert!(
        matches!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("42501" | "P0002")
        ),
        "{error:?}"
    );
    drop(denied);
    let mut tx = f.state.db.begin_as(f.owner.id).await.unwrap();
    let grants: Vec<String> = sqlx::query_scalar("SELECT r.rolname::text FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace CROSS JOIN LATERAL aclexplode(p.proacl) a JOIN pg_roles r ON r.oid=a.grantee WHERE n.nspname='fvoci' AND p.proname='app_collab_command_pending' AND a.grantee<>p.proowner AND a.privilege_type='EXECUTE' ORDER BY r.rolname")
        .fetch_all(tx.conn()).await.unwrap();
    assert_eq!(grants, vec!["fvoci_app"]);
    let public_execute: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace CROSS JOIN LATERAL aclexplode(p.proacl) a WHERE n.nspname='fvoci' AND p.proname='app_collab_command_pending' AND a.grantee=0 AND a.privilege_type='EXECUTE')")
        .fetch_one(tx.conn()).await.unwrap();
    assert!(
        !public_execute,
        "PUBLIC can inspect pending collaboration state"
    );
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM app_collab_command_pending('workspace_remove', $1, $2) p",
    )
    .bind(f.workspace.parse::<uuid::Uuid>().unwrap())
    .bind(f.writer.id)
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![json!({"document_id": f.document, "workspace_id": f.workspace})]
    );
    tx.commit().await.unwrap();
    f.stop().await;
}

#[tokio::test]
async fn unjoined_organization_admin_gets_actionable_409_and_keeps_closed_state() {
    let f = Fixture::new().await;
    closed_pending(&f).await;
    let admin = common::login(&f.state, "metadata admin").await;
    let org = common::organization_of_workspace(&f.workspace).await;
    common::join_organization(org, &admin, "admin").await;
    let before = stored(&f).await;
    let mut denied = f.state.db.begin_as(admin.id).await.unwrap();
    let load = sqlx::query("SELECT * FROM app_collab_load($1)")
        .bind(f.document.parse::<uuid::Uuid>().unwrap())
        .fetch_all(denied.conn())
        .await
        .expect_err("metadata admin could load document contents");
    assert_eq!(
        load.as_database_error().unwrap().code().as_deref(),
        Some("P0002")
    );
    drop(denied);
    let uri = format!("/api/organizations/{org}/members/{}", f.writer.id.0);
    let blocked = send(&f.app, Method::DELETE, &uri, Some(&admin), None).await;
    assert_eq!(blocked.status, StatusCode::CONFLICT, "{:?}", blocked.json);
    let message = blocked.json["message"].as_str().unwrap();
    assert!(
        message.contains(&f.document) && message.contains(&f.workspace),
        "{message}"
    );
    assert!(
        message.contains("join as admin") && message.contains("save"),
        "{message}"
    );
    assert!(
        !message.contains(PENDING) && !message.contains("sole-writer"),
        "{message}"
    );
    assert_eq!(stored(&f).await, before, "409 changed the pending state");
    // 409는 탈퇴나 초안 버리기가 아니다. 원래 편집자는 그대로 이어서 연결할 수 있다.
    let mut rejoined = Client::connect(f.addr, &f.document, &f.writer).await;
    assert_eq!(rejoined.body(), common::doc_body(PENDING));
    assert!(rejoined.edit_and_wait("409 뒤 보존한 편집 계속").await);
    rejoined.close().await;
    f.stop().await;
}

#[tokio::test]
async fn workspace_leave_checkpoints_the_departing_editors_pending_state() {
    leaving(false).await;
}

#[tokio::test]
async fn organization_leave_checkpoints_the_departing_editors_pending_state() {
    leaving(true).await;
}

async fn leaving(organization: bool) {
    let f = Fixture::new().await;
    let mut writer = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(writer.edit_and_wait(PENDING).await);
    let (method, uri) = if organization {
        (
            Method::DELETE,
            format!(
                "/api/organizations/{}/members/{}",
                common::organization_of_workspace(&f.workspace).await,
                f.writer.id.0
            ),
        )
    } else {
        (
            Method::POST,
            format!("/api/workspaces/{}/leave", f.workspace),
        )
    };
    let left = send(&f.app, method, &uri, Some(&f.writer), None).await;
    assert_eq!(left.status, StatusCode::NO_CONTENT, "{:?}", left.json);
    assert!(
        f.history()
            .await
            .iter()
            .any(|r| r["body"] == common::doc_body(PENDING))
    );
    assert!(!writer.edit_and_wait("탈퇴 뒤 옛 방 편집").await);
    let mut owner = Client::connect(f.addr, &f.document, &f.owner).await;
    assert_eq!(owner.body(), common::doc_body(PENDING));
    owner.close().await;
    f.stop().await;
}

#[tokio::test]
async fn closed_pending_checkpoint_records_the_proxy_separately_from_the_editor() {
    let f = Fixture::new().await;
    closed_pending(&f).await;
    let mut conn = common::fixture_conn().await;
    sqlx::query("UPDATE fvoci.workspace_members SET role='readonly' WHERE workspace_id=$1::uuid AND user_id=$2")
        .bind(&f.workspace).bind(f.writer.id).execute(&mut conn).await.unwrap();
    let removed = send(
        &f.app,
        Method::DELETE,
        &format!("/api/workspaces/{}/members/{}", f.workspace, f.writer.id.0),
        Some(&f.owner),
        None,
    )
    .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{:?}", removed.json);
    let history = f.history().await;
    let saved = history
        .iter()
        .find(|r| r["body"] == common::doc_body(PENDING))
        .expect("pending edit was not checkpointed");
    assert_eq!(saved["author_name"], "lifecycle owner");
    assert_eq!(saved["edited_by_names"], json!(["sole-writer"]));
    f.stop().await;
}

#[tokio::test]
async fn read_shared_organization_admin_cannot_proxy_save_closed_pending_edits() {
    let f = Fixture::new().await;
    closed_pending(&f).await;
    let email = common::unique_email("read-admin");
    let admin = common::login_with_email(&f.state, "read admin", &email, true).await;
    let org = common::organization_of_workspace(&f.workspace).await;
    common::join_organization(org, &admin, "admin").await;
    let shared = send(
        &f.app,
        Method::POST,
        &format!("/api/documents/{}/shares", f.document),
        Some(&f.owner),
        Some(json!({"email": email, "permission": "read", "organization_id": org})),
    )
    .await;
    assert_eq!(shared.status, StatusCode::CREATED, "{:?}", shared.json);
    let visible = send(
        &f.app,
        Method::GET,
        &format!("/api/documents/{}", f.document),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(visible.status, StatusCode::OK, "{:?}", visible.json);
    assert_eq!(visible.json["can_edit"], false);
    let before = stored(&f).await;
    let blocked = send(
        &f.app,
        Method::DELETE,
        &format!("/api/organizations/{org}/members/{}", f.writer.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(blocked.status, StatusCode::CONFLICT, "{:?}", blocked.json);
    assert_eq!(
        stored(&f).await,
        before,
        "read-only admin changed the closed pending state"
    );
    f.stop().await;
}
