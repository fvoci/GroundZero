#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
mod common;
mod sql_support;

use sql_support::*;
use uuid::Uuid;

// D-20·D-112·D-113: 기대 등급은 SQL 결과에서 유도하지 않는다.
#[tokio::test]
async fn roles_match_constant_grades_rls_and_real_task_writes() {
    let w = World::new().await;
    let server = common::login(&w.state, "server admin outside organization").await;
    common::make_server_admin(&server).await;
    let org_admin = common::login(&w.state, "organization admin").await;
    common::join_organization(w.org, &org_admin, "admin").await;
    let admin = w.member("admin").await;
    let member = w.member("member").await;
    let readonly = w.member("readonly").await;
    let outside = common::login(&w.state, "same organization").await;
    common::join_organization(w.org, &outside, "member").await;
    let foreign = common::login(&w.state, "outside organization").await;
    let private = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_item($1,$2,'private','',@ids) AS id",
        &[w.module, Uuid::now_v7()],
    )
    .await;
    let doc = w.doc().await;
    let secret = w.doc().await;
    fixture(
        "UPDATE fvoci.documents SET visibility='private' WHERE id=$1",
        &[secret],
    )
    .await;
    let note = w.note().await;
    let read_note = w.note().await;
    let write_note = w.note().await;
    w.share(read_note, &member, "read").await;
    w.share(write_note, &member, "write").await;
    let recipient = w.member("member").await;
    let mut attachments = std::collections::HashMap::new();
    for document in [doc, secret, note, read_note, write_note] {
        attachments.insert(document, attachment(&w, document).await);
        fixture(
            "UPDATE fvoci.documents SET allow_copy=false WHERE id=$1",
            &[document],
        )
        .await;
    }
    let task = w.task().await;
    let comment = made(&w.state, &w.owner, COMMENT, &[Uuid::now_v7(), task]).await;
    let targets = [
        ("item", "items", w.item),
        ("item", "items", private),
        ("document", "documents", doc),
        ("document", "documents", secret),
        ("document", "documents", note),
        ("document", "documents", read_note),
        ("document", "documents", write_note),
        ("task", "tasks", task),
        ("comment", "comments", comment),
    ];
    // 공개 항목, 비공개 항목, 공간 문서, 비공개 문서, 개인 노트 셋, 업무, 댓글 순.
    let matrix = [
        (&w.owner, ["owner"; 9]),
        (&server, ["none"; 9]),
        (&org_admin, ["none"; 9]),
        (
            &admin,
            [
                "owner", "owner", "owner", "owner", "none", "none", "none", "owner", "owner",
            ],
        ),
        (
            &member,
            [
                "write", "none", "write", "none", "none", "read", "write", "write", "write",
            ],
        ),
        (
            &readonly,
            [
                "read", "none", "read", "none", "none", "none", "none", "read", "read",
            ],
        ),
        (&outside, ["none"; 9]),
        (&foreign, ["none"; 9]),
    ];
    for (actor, grades) in matrix {
        for ((kind, table, target), expected) in targets.iter().zip(grades) {
            let got = one(
                &w.state,
                actor,
                &format!("SELECT fvoci.app_access('{kind}',$1) AS grade"),
                &[*target],
            )
            .await;
            assert_eq!(
                got["grade"], expected,
                "actor={:?}, target={kind}/{target}",
                actor.id
            );
            let visible = rows(
                &w.state,
                actor,
                &format!("SELECT id FROM fvoci.{table} WHERE id=$1"),
                &[*target],
            )
            .await;
            assert_eq!(
                visible.len(),
                usize::from(expected != "none"),
                "RLS actor={:?}, target={target}",
                actor.id
            );
            if *kind == "document" {
                let mut tx = w.state.db.begin_as(actor.id).await.unwrap();
                let changed = sqlx::query(
                    "UPDATE fvoci.documents SET title='matrix write',revision=revision+1,updated_by=$2 WHERE id=$1",
                )
                .bind(target)
                .bind(actor.id.0)
                .execute(tx.conn())
                .await
                .unwrap()
                .rows_affected();
                assert_eq!(
                    changed,
                    u64::from(matches!(expected, "owner" | "write")),
                    "document UPDATE actor={:?}",
                    actor.id
                );
                // 롤백하여 각 칸이 같은 fixture를 검사한다.
                drop(tx);
                let mut tx = w.state.db.begin_as(actor.id).await.unwrap();
                let comment = on(
                    tx.conn(),
                    "SELECT fvoci.app_add_comment($1,$2,NULL,NULL,'matrix comment',@ids)",
                    &[Uuid::now_v7(), *target],
                )
                .await;
                if matches!(expected, "owner" | "write") {
                    assert_eq!(comment.unwrap().len(), 1);
                } else {
                    code(
                        &comment.unwrap_err(),
                        if expected == "none" { "P0002" } else { "42501" },
                    );
                }
                drop(tx);
                assert_eq!(
                    one(
                        &w.state,
                        actor,
                        "SELECT fvoci.app_document_can_export($1) AS allowed",
                        &[*target]
                    )
                    .await["allowed"],
                    expected == "owner"
                );
                assert_eq!(
                    one(
                        &w.state,
                        actor,
                        "SELECT fvoci.app_file_can_download($1) AS allowed",
                        &[attachments[target]]
                    )
                    .await["allowed"],
                    expected == "owner"
                );
                for (sql, args) in [
                    (
                        "SELECT fvoci.app_copy_document($1,$2,NULL,'matrix copy',@ids)",
                        vec![*target, Uuid::now_v7()],
                    ),
                    (
                        "SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'read',$3,@ids)",
                        vec![*target, recipient.id.0, w.org],
                    ),
                ] {
                    let mut tx = w.state.db.begin_as(actor.id).await.unwrap();
                    let result = on(tx.conn(), sql, &args).await;
                    if expected == "owner" {
                        assert_eq!(result.unwrap().len(), 1);
                    } else {
                        code(
                            &result.unwrap_err(),
                            if expected == "none" { "P0002" } else { "42501" },
                        );
                    }
                }
            }
        }
        let grade = grades[7];
        let mut tx = w.state.db.begin_as(actor.id).await.unwrap();
        let edited = on(tx.conn(), UPDATE_TASK, &[task]).await;
        if matches!(grade, "owner" | "write") {
            assert_eq!(edited.unwrap()[0]["outcome"], "updated");
        } else {
            code(
                &edited.unwrap_err(),
                if grade == "none" { "P0002" } else { "42501" },
            );
        }
        drop(tx);
        let mut tx = w.state.db.begin_as(actor.id).await.unwrap();
        let added = on(tx.conn(), COMMENT, &[Uuid::now_v7(), task]).await;
        if matches!(grade, "owner" | "write") {
            assert_eq!(added.unwrap().len(), 1);
        } else {
            code(
                &added.unwrap_err(),
                if grade == "none" { "P0002" } else { "42501" },
            );
        }
    }
    rows(
        &w.state,
        &org_admin,
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[w.ws],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &org_admin,
            "SELECT fvoci.app_access('item',$1) AS grade",
            &[private]
        )
        .await["grade"],
        "owner"
    );
    let logs = rows(&w.state,&org_admin,"SELECT workspace_id,organization_id FROM fvoci.audit_log WHERE actor=$1 AND action='admin_join' AND (workspace_id=$2 OR organization_id=$3)",&[org_admin.id.0,w.ws,w.org]).await;
    assert_eq!(logs.len(), 2);
    assert_eq!(
        logs.iter().filter(|r| r["workspace_id"].is_null()).count(),
        1
    );
}

#[tokio::test]
async fn copy_export_download_share_and_move_enforce_separate_rules() {
    let w = World::new().await;
    let member = w.member("member").await;
    let readonly = w.member("readonly").await;
    let doc = w.doc().await;
    let file = attachment(&w, doc).await;
    fixture(
        "UPDATE fvoci.documents SET allow_copy=false WHERE id=$1",
        &[doc],
    )
    .await;
    for actor in [&w.owner, &member, &readonly] {
        let owner = actor.id == w.owner.id;
        assert_eq!(
            one(
                &w.state,
                actor,
                "SELECT fvoci.app_document_can_export($1) AS allowed",
                &[doc]
            )
            .await["allowed"],
            owner
        );
        assert_eq!(
            one(
                &w.state,
                actor,
                "SELECT fvoci.app_file_can_download($1) AS allowed",
                &[file]
            )
            .await["allowed"],
            owner
        );
        let copy = "SELECT fvoci.app_copy_document($1,$2,NULL,'copy',@ids) AS id";
        if owner {
            made(&w.state, actor, copy, &[doc, Uuid::now_v7()]).await;
        } else {
            denied(&w.state, actor, copy, &[doc, Uuid::now_v7()], "42501").await;
        }
    }
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_document_template($1,true,@ids)",
        &[doc],
    )
    .await;
    made(
        &w.state,
        &member,
        "SELECT fvoci.app_copy_document($1,$2,NULL,'template copy',@ids) AS id",
        &[doc, Uuid::now_v7()],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &member,
            "SELECT fvoci.app_document_can_export($1) AS allowed",
            &[doc]
        )
        .await["allowed"],
        false
    );
    let share = "SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'write',$3,@ids)";
    denied(
        &w.state,
        &member,
        share,
        &[doc, readonly.id.0, w.org],
        "42501",
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        share,
        &[doc, readonly.id.0, w.org],
        "22023",
    )
    .await;
    // 이메일은 바인드한 검증된 사용자 값으로 전달: RLS 때문에 보이지 않는 이메일을 NULL로 바꾸지 않는다.
    let foreign =
        common::login_with_email(&w.state, "foreign", &common::unique_email("foreign"), true).await;
    let mut conn = common::fixture_conn().await;
    let email: String = sqlx::query_scalar("SELECT email FROM fvoci.users WHERE id=$1")
        .bind(foreign.id.0)
        .fetch_one(&mut conn)
        .await
        .unwrap();
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    let error = sqlx::query("SELECT fvoci.app_share_document($1,$2,'read',$3,$4)")
        .bind(doc)
        .bind(email)
        .bind(w.org)
        .bind(ids())
        .execute(tx.conn())
        .await
        .unwrap_err();
    code(&error, "22023");
    drop(tx);
    let same = workspace(&w.state, &w.owner, w.org).await;
    rows(
        &w.state,
        &w.owner,
        "SELECT * FROM fvoci.app_move_preview($1,$2,NULL)",
        &[doc, same],
    )
    .await;
    let other_org = common::organization_for(&foreign).await;
    common::join_organization(other_org, &w.owner, "member").await;
    let other = workspace(&w.state, &w.owner, other_org).await;
    denied(
        &w.state,
        &w.owner,
        "SELECT * FROM fvoci.app_move_preview($1,$2,NULL)",
        &[doc, other],
        "55000",
    )
    .await;
}
