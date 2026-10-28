#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 공간 문서에서 만든 개인 노트 사본은 출처 조직을 기억하고 그 조직의 복제·이동 경계를 계속 따른다(#175=1, D-114·D-49).
//! 결정 문구 밖의 경우 A~E는 부조종사 결정(#175 댓글 6459867577)을 따른다.

use crate::common::{self, TestUser};
use crate::sql_support::{World, attempt, code, made, workspace};

use uuid::Uuid;

const COPY_TO_NOTE: &str = "SELECT fvoci.app_copy_document($1,$2,NULL,'사본',@ids) AS id";
const COPY_TO_SPACE: &str = "SELECT fvoci.app_copy_document($1,$2,$3,'사본',@ids) AS id";
const MOVE: &str = "SELECT * FROM fvoci.app_move_document($1,$2,NULL,@ids)";
const ALLOW_COPY: &str = "SELECT fvoci.app_set_document_settings($1,'workspace',true,@ids)";
const ALLOW_NOTE_COPY: &str = "SELECT fvoci.app_set_document_settings($1,'private',true,@ids)";

/// 조직 X(`w.org`)와 그 밖의 조직 Y. `w.owner`는 둘 다의 관리자이고, X의 둘째 공간과 Y의 공간이 있다.
struct Two {
    w: World,
    other_org: Uuid,
    same_org_ws: Uuid,
    other_ws: Uuid,
}

impl Two {
    async fn new() -> Self {
        let w = World::new().await;
        let boss = common::login(&w.state, "other boss").await;
        let other_org = common::organization_for(&boss).await;
        common::join_organization(other_org, &w.owner, "admin").await;
        let other_ws = workspace(&w.state, &w.owner, other_org).await;
        let same_org_ws = workspace(&w.state, &w.owner, w.org).await;
        Self {
            w,
            other_org,
            same_org_ws,
            other_ws,
        }
    }

    /// 사용자를 Y의 관리자로 들이고 그 사람의 Y 공간을 만든다.
    async fn other_space_for(&self, user: &TestUser) -> Uuid {
        common::join_organization(self.other_org, user, "admin").await;
        workspace(&self.w.state, user, self.other_org).await
    }
}

/// 값을 돌려주지 않는 함수 호출.
async fn run(w: &World, user: &TestUser, sql: &str, args: &[Uuid]) {
    attempt(&w.state, user, sql, args).await.expect(sql);
}

async fn note_copy(w: &World, user: &TestUser, doc: Uuid) -> Uuid {
    made(&w.state, user, COPY_TO_NOTE, &[doc, Uuid::now_v7()]).await
}

/// 다른 조직 공간으로의 복제·이동이 기존 "다른 조직" 거절(55000)로 막힌다.
async fn kept_out(w: &World, user: &TestUser, note: Uuid, target: Uuid) {
    let copy = attempt(
        &w.state,
        user,
        COPY_TO_SPACE,
        &[note, Uuid::now_v7(), target],
    )
    .await;
    let copy = copy.expect_err("copying out of the source organization must be refused");
    code(&copy, "55000");
    assert!(
        copy.to_string()
            .contains("copies stay within the organization"),
        "{copy}"
    );
    let moved = attempt(&w.state, user, MOVE, &[note, target]).await;
    let moved = moved.expect_err("moving out of the source organization must be refused");
    code(&moved, "55000");
    assert!(
        moved
            .to_string()
            .contains("documents move within the organization"),
        "{moved}"
    );
}

/// SR1-1 첫째·둘째 경로와 B(복제): 공간 문서 → 개인 노트 → 다른 조직 공간.
#[tokio::test]
async fn a_note_copied_from_a_space_document_stays_inside_that_organization() {
    let t = Two::new().await;
    let note = note_copy(&t.w, &t.w.owner, t.w.doc().await).await;
    kept_out(&t.w, &t.w.owner, note, t.other_ws).await;
}

/// C: 기억한 노트를 개인 노트끼리 복제한 사본도 기억을 이어받는다.
#[tokio::test]
async fn a_copy_of_a_remembered_note_inherits_the_organization() {
    let t = Two::new().await;
    let first = note_copy(&t.w, &t.w.owner, t.w.doc().await).await;
    let second = note_copy(&t.w, &t.w.owner, first).await;
    kept_out(&t.w, &t.w.owner, second, t.other_ws).await;
}

/// SR1-1 셋째 경로(기억이 있는 경우): 공유받은 사람도 기억한 노트를 다른 조직 공간으로 복제할 수 없다.
#[tokio::test]
async fn a_shared_remembered_note_is_not_copied_out_by_the_recipient() {
    let t = Two::new().await;
    let note = note_copy(&t.w, &t.w.owner, t.w.doc().await).await;
    run(&t.w, &t.w.owner, ALLOW_NOTE_COPY, &[note]).await;
    let member = t.w.member("member").await;
    t.w.share(note, &member, "read").await;
    let theirs = t.other_space_for(&member).await;
    let copy = attempt(
        &t.w.state,
        &member,
        COPY_TO_SPACE,
        &[note, Uuid::now_v7(), theirs],
    )
    .await;
    code(
        &copy.expect_err("the recipient copied the note out"),
        "55000",
    );
}

/// E(떠난 조직): 출처 조직을 떠난 뒤에도 기억은 남아 다른 조직 공간으로 복제·이동할 수 없다. 개인 노트끼리 복제는 된다.
#[tokio::test]
async fn leaving_the_source_organization_keeps_the_boundary() {
    let t = Two::new().await;
    let doc = t.w.doc().await;
    run(&t.w, &t.w.owner, ALLOW_COPY, &[doc]).await;
    let member = t.w.member("member").await;
    let note = note_copy(&t.w, &member, doc).await;
    let theirs = t.other_space_for(&member).await;
    run(
        &t.w,
        &t.w.owner,
        "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
        &[t.w.org, member.id.0],
    )
    .await;
    kept_out(&t.w, &member, note, theirs).await;
    let again = note_copy(&t.w, &member, note).await;
    kept_out(&t.w, &member, again, theirs).await;
}

/// 지킴: 기억한 노트도 같은 조직 공간으로는 복제·이동된다.
#[tokio::test]
async fn a_remembered_note_still_copies_and_moves_within_its_organization() {
    let t = Two::new().await;
    let note = note_copy(&t.w, &t.w.owner, t.w.doc().await).await;
    made(
        &t.w.state,
        &t.w.owner,
        COPY_TO_SPACE,
        &[note, Uuid::now_v7(), t.same_org_ws],
    )
    .await;
    attempt(&t.w.state, &t.w.owner, MOVE, &[note, t.same_org_ws])
        .await
        .unwrap();
}

/// D 지킴: 처음부터 개인 노트로 만든 노트(기억 없음)는 지금대로 소유자가 속한 다른 조직 공간으로 옮기고, 공유받은
/// 사람도 다른 조직 공간으로 복제할 수 있다(D-49, #175 선택지 2는 고르지 않음).
#[tokio::test]
async fn a_note_written_from_scratch_is_not_bound_to_an_organization() {
    let t = Two::new().await;
    let shared = t.w.note().await;
    run(&t.w, &t.w.owner, ALLOW_NOTE_COPY, &[shared]).await;
    let member = t.w.member("member").await;
    t.w.share(shared, &member, "read").await;
    let theirs = t.other_space_for(&member).await;
    made(
        &t.w.state,
        &member,
        COPY_TO_SPACE,
        &[shared, Uuid::now_v7(), theirs],
    )
    .await;
    let moved = t.w.note().await;
    attempt(&t.w.state, &t.w.owner, MOVE, &[moved, t.other_ws])
        .await
        .unwrap();
}
