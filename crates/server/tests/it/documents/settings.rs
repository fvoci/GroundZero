use super::command_support::{assert_stale, call, snapshot};
use crate::sql_support::{self, World};
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn document_settings_same_field_conflict_changes_nothing() {
    let w = World::new().await;
    let doc = w.doc().await;
    let uri = format!("/api/documents/{doc}/settings");
    let first = call(
        &w,
        &w.owner,
        Method::PATCH,
        &uri,
        &[],
        &json!({
            "visibility":"private", "previous_visibility":"workspace"
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let before = snapshot(&w).await;
    // Even if the intended visibility already matches, its previous value is stale.
    // The second field must not be partly saved or audited.
    let stale = call(
        &w,
        &w.owner,
        Method::PATCH,
        &uri,
        &[],
        &json!({
            "visibility":"private", "previous_visibility":"workspace",
            "allow_copy":true, "previous_allow_copy":false
        }),
    )
    .await;
    assert_stale(&stale, json!(["allow_copy"]));
    assert_eq!(stale.body["current"]["visibility"], "private");
    assert_eq!(stale.body["current"]["allow_copy"], false);
    assert_eq!(before, snapshot(&w).await);

    let first = call(
        &w,
        &w.owner,
        Method::PATCH,
        &uri,
        &[],
        &json!({
            "allow_copy":true, "previous_allow_copy":false
        }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let before = snapshot(&w).await;
    let stale = call(
        &w,
        &w.owner,
        Method::PATCH,
        &uri,
        &[],
        &json!({
            "allow_copy":false, "previous_allow_copy":false
        }),
    )
    .await;
    assert_stale(&stale, json!(["allow_copy"]));
    assert_eq!(stale.body["current"]["allow_copy"], true);
    assert_eq!(before, snapshot(&w).await);
}

#[tokio::test]
async fn document_settings_require_previous_values_for_supplied_fields() {
    let w = World::new().await;
    let doc = w.doc().await;
    let uri = format!("/api/documents/{doc}/settings");
    let before = snapshot(&w).await;
    for body in [
        json!({"visibility":"private"}),
        json!({"allow_copy":true}),
        json!({"allow_copy":false,"previous_allow_copy":null}),
        json!({"previous_visibility":"workspace"}),
        json!({"previous_allow_copy":false}),
        json!({"visibility":"private","previous_visibility":"invalid"}),
    ] {
        let reply = call(&w, &w.owner, Method::PATCH, &uri, &[], &body).await;
        assert_eq!(
            reply.status,
            StatusCode::BAD_REQUEST,
            "{body}: {}",
            reply.body
        );
        assert_eq!(before, snapshot(&w).await);
    }
}

#[tokio::test]
async fn document_settings_compare_after_waiting_for_the_row_lock() {
    let w = World::new().await;
    for (second, expected) in [
        ("'workspace',NULL,'workspace',NULL", false),
        ("NULL,true,NULL,false", true),
    ] {
        let doc = w.doc().await;
        let mut first = w.state.db.begin_as(w.owner.id).await.unwrap();
        sql_support::on(first.conn(),
            "SELECT fvoci.app_set_document_settings($1,'private',NULL,'workspace',NULL,@ids) AS saved", &[doc])
            .await.unwrap();
        let holder = sql_support::pid(first.conn()).await;
        let mut next = w.state.db.begin_as(w.owner.id).await.unwrap();
        let waiter = sql_support::pid(next.conn()).await;
        let sql = format!("SELECT fvoci.app_set_document_settings($1,{second},@ids) AS saved");
        let pending = tokio::spawn(async move {
            let result = sql_support::on(next.conn(), &sql, &[doc]).await.unwrap();
            next.commit().await.unwrap();
            result
        });
        sql_support::blocked(holder, waiter).await;
        first.commit().await.unwrap();
        let reply = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reply[0]["saved"], expected);
        let now = sql_support::one(
            &w.state,
            &w.owner,
            "SELECT visibility,allow_copy,revision FROM fvoci.documents WHERE id=$1",
            &[doc],
        )
        .await;
        assert_eq!(now["visibility"], "private");
        assert_eq!(now["allow_copy"], expected);
        assert_eq!(now["revision"], 1);
    }
}
