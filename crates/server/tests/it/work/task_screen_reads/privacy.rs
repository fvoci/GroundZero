//! P is paired with every R role; the complete permission product is in matrix.rs.
use super::{Result, fixture::Fixture, matrix};
use crate::{common, sql_support};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test]
async fn private_notes_time_history_and_public_links_cross_every_role() -> Result<()> {
    let mut f = Fixture::new().await?;
    sqlx::raw_sql(matrix::OUTCOME).execute(&mut f.ddl).await?;
    let doc: Uuid =
        sqlx::query_scalar("SELECT id FROM fvoci.documents WHERE item_id=$1 AND parent_id IS NULL")
            .bind(f.item)
            .fetch_one(&mut f.ddl)
            .await?;
    sqlx::query("UPDATE fvoci.documents SET visibility='private' WHERE id=$1")
        .bind(doc)
        .execute(&mut f.ddl)
        .await?;
    let note = sql_support::made(&f.state,&f.owner,"SELECT fvoci.app_create_personal_note($1,'PERSONAL_SCREEN_SENTINEL',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id",&[Uuid::now_v7()]).await;
    let link = sql_support::made(
        &f.state,
        &f.owner,
        "SELECT fvoci.app_create_public_link($1,decode(repeat('a',64),'hex'),NULL,@ids) AS id",
        &[doc],
    )
    .await;
    let entry = Uuid::now_v7();
    for (id, user, memo) in [
        (entry, f.actor.id.0, "my private memo"),
        (Uuid::now_v7(), f.owner.id.0, "OTHER_TIME_SCREEN_SENTINEL"),
    ] {
        sqlx::query("INSERT INTO fvoci.time_entries(id,user_id,started_at,ended_at,memo,target_kind,target_id,workspace_id,request_id) VALUES($1,$2,'2026-01-01T00:00Z','2026-01-01T00:01Z',$3,'task',$4,$5,$6)")
            .bind(id).bind(user).bind(memo).bind(f.tasks[0]).bind(f.ws).bind(Uuid::now_v7()).execute(&mut f.ddl).await?;
        sqlx::query("INSERT INTO fvoci.time_entry_revisions(id,entry_id,user_id,started_at,ended_at,paused_seconds,kind,memo,target_kind,target_id) VALUES($1,$2,$3,'2026-01-01T00:00Z','2026-01-01T00:01Z',0,'work',$4,'task',$5)")
            .bind(Uuid::now_v7()).bind(id).bind(user).bind(memo).bind(f.tasks[0]).execute(&mut f.ddl).await?;
    }
    let app = common::app(&f.state);
    for path in [
        format!("/api/items/{}/task-context?token=public-link", f.item),
        format!("/api/items/{}/task-board", f.item),
        format!("/api/tasks/{}/detail-read", f.tasks[0]),
    ] {
        assert_eq!(
            common::send(&app, Method::GET, &path, None, None)
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
    }
    let mut cases = 0;
    for organization in ["creator", "admin", "member", "none", "outside"] {
        for workspace in ["owner", "admin", "member", "readonly", "none"] {
            for server in [false, true] {
                matrix::set_role(&mut f, organization, workspace, server).await?;
                for (p, sharing) in ["private", "read", "write", "revoked", "expired", "disabled"]
                    .into_iter()
                    .enumerate()
                {
                    let own_note = p == 0;
                    sqlx::query("UPDATE fvoci.documents SET owner_user_id=$2,source_organization_id=$3 WHERE id=$1")
                        .bind(note)
                        .bind(if own_note { f.actor.id.0 } else { f.owner.id.0 })
                        .bind(if matches!(p,1|2) {Some(f.org)} else if matches!(p,3|4) {Some(f.other_org)} else {None})
                        .execute(&mut f.ddl)
                        .await?;
                    sqlx::query("DELETE FROM fvoci.document_shares WHERE document_id=ANY($1)")
                        .bind(vec![doc, note])
                        .execute(&mut f.ddl)
                        .await?;
                    if matches!(sharing, "read" | "write") {
                        for target in [doc, note] {
                            sqlx::query("INSERT INTO fvoci.document_shares(id,document_id,organization_id,grantee_user_id,permission,granted_by) VALUES($1,$2,$3,$4,$5,$6)")
                                .bind(Uuid::now_v7()).bind(target).bind(f.org).bind(f.actor.id.0).bind(sharing).bind(f.owner.id.0).execute(&mut f.ddl).await?;
                        }
                    }
                    sqlx::query("UPDATE fvoci.public_links SET revoked_at=CASE WHEN $2 THEN now() END,expires_at=CASE WHEN $3 THEN now()-interval '1 second' END WHERE id=$1").bind(link).bind(p==3).bind(p==4).execute(&mut f.ddl).await?;
                    sqlx::query(
                        "UPDATE fvoci.organizations SET public_links_enabled=$2 WHERE id=$1",
                    )
                    .bind(f.org)
                    .bind(p != 5)
                    .execute(&mut f.ddl)
                    .await?;
                    sqlx::query("UPDATE fvoci.time_entries SET ended_at=CASE WHEN $2 THEN NULL ELSE '2026-01-01T00:01Z'::timestamptz END,deleted_at=CASE WHEN $3 THEN now() END,target_id=$4,paused_seconds=$5 WHERE id=$1")
                        .bind(entry).bind(p==1).bind(p==2).bind(if p==3 {f.tasks[1]} else {f.tasks[0]}).bind(if p==5 {60} else {0}).execute(&mut f.ddl).await?;
                    let member = !matches!(organization, "none" | "outside");
                    let item_visible = member && workspace != "none";
                    let shared = member && matches!(sharing, "read" | "write");
                    let doc_visible = (member && matches!(workspace, "owner" | "admin")) || shared;
                    let mut tx = f.state.db.begin_as(f.actor.id).await?;
                    let (note_seen,doc_seen,other_times,other_history,public,context,actual):(bool,bool,i64,i64,i64,Value,Value)=sqlx::query_as(
                        "SELECT EXISTS(SELECT 1 FROM fvoci.documents WHERE id=$1),EXISTS(SELECT 1 FROM fvoci.documents WHERE id=$2),
                         (SELECT count(*) FROM fvoci.time_entries WHERE user_id<>fvoci.app_user_id()),
                         (SELECT count(*) FROM fvoci.time_entry_revisions WHERE user_id<>fvoci.app_user_id()),
                         (SELECT count(*) FROM fvoci.app_public_document(decode(repeat('a',64),'hex'))),
                         fvoci.screen_outcome('context',$3),fvoci.screen_outcome('time',$4)")
                        .bind(note).bind(doc).bind(f.item).bind(f.tasks[0]).fetch_one(tx.conn()).await?;
                    let case = format!("{organization}/{workspace}/{server}/{sharing}");
                    assert_eq!(note_seen, own_note || shared, "note: {case}");
                    assert_eq!(doc_seen, doc_visible, "private document: {case}");
                    assert_eq!((other_times, other_history), (0, 0), "private time: {case}");
                    assert_eq!(public, i64::from(p < 3), "link: {case}");
                    if item_visible {
                        assert_eq!(
                            context["documents"].as_array().ok_or("documents")?.len(),
                            1 + usize::from(doc_visible),
                            "context documents: {case}"
                        );
                        let children: Vec<_> = context["documents"]
                            .as_array()
                            .ok_or("documents")?
                            .iter()
                            .filter(|row| row["id"] != json!(doc))
                            .collect();
                        assert_eq!(children.len(), 1, "one visible child: {case}");
                        assert_eq!(
                            children[0]["parent_id"],
                            if doc_visible { json!(doc) } else { Value::Null },
                            "hidden parent: {case}"
                        );
                        assert!(
                            !context.to_string().contains("PERSONAL_SCREEN_SENTINEL"),
                            "{case}"
                        );
                        assert_eq!(
                            actual,
                            json!(if matches!(p, 0 | 4) { 60 } else { 0 }),
                            "my seconds: {case}"
                        );
                    } else {
                        assert_eq!(context["code"], json!("P0002"), "{case}");
                        assert_eq!(actual["code"], json!("P0002"), "{case}");
                    }
                    tx.commit().await?;
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 300, "each personal/share value meets every R role");
    f.finish().await
}
