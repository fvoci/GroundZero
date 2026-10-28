use crate::sql_support::{World, code, made, on, one, rows, workspace};

/// SR-201-6: 불가능한 마지막 관리자 강등은 공간·문서의 작업을 기다리지 않는다.
#[tokio::test]
async fn last_admin_self_demotion_does_not_wait_for_workspace_or_document_locks() {
    let w = World::new().await;
    let creator = w.member("member").await;
    let ws = workspace(&w.state, &creator, w.org).await;
    let doc = made(
        &w.state,
        &creator,
        "SELECT fvoci.app_create_wiki_document($1,$2,'last admin lock',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id",
        &[ws, uuid::Uuid::now_v7()],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[ws],
    )
    .await;
    for (resource, lock, target) in [
        (
            "workspace",
            "SELECT id FROM fvoci.workspaces WHERE id=$1 FOR NO KEY UPDATE",
            ws,
        ),
        (
            "document",
            "SELECT id FROM fvoci.documents WHERE id=$1 FOR UPDATE",
            doc,
        ),
    ] {
        let mut holder = w.state.db.begin_as(creator.id).await.unwrap();
        on(holder.conn(), lock, &[target]).await.unwrap();
        let mut call = w.state.db.begin_as(w.owner.id).await.unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let count: i32 =
                sqlx::query_scalar("SELECT fvoci.app_set_organization_member_ids($1,$2,'member')")
                    .bind(w.org)
                    .bind(w.owner.id.0)
                    .fetch_one(call.conn())
                    .await?;
            sqlx::query("SELECT fvoci.app_set_organization_member($1,$2,'member',$3)")
                .bind(w.org)
                .bind(w.owner.id.0)
                .bind(fvoci_db::new_ids(usize::try_from(count).unwrap()))
                .execute(call.conn())
                .await
        })
        .await;
        assert!(
            result.is_ok(),
            "SR-201-6 last admin demotion waited for the locked {resource}"
        );
        let error = result.unwrap().unwrap_err();
        code(&error, "55000");
        assert_eq!(
            error.as_database_error().unwrap().message(),
            "the organization needs at least one admin"
        );
        holder.commit().await.unwrap();
    }
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2",
            &[w.org, w.owner.id.0],
        )
        .await["role"],
        "admin"
    );
}
