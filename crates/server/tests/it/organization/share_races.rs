use crate::sql_support::{World, blocked, code, on, pid};

/// D-20·D-114: 제거가 먼저 커밋되면 기다리던 공유가 공간 밖 조직원에게 write를 남기지 않는다.
#[tokio::test]
async fn member_removal_first_rejects_the_waiting_write_share() {
    let w = World::new().await;
    let member = w.member("member").await;
    let doc = w.doc().await;
    // 기존 공유가 있어 문서 잠금에서 만나는 경우도 고정한다.
    w.share(doc, &member, "read").await;
    let mut remove = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        remove.conn(),
        "SELECT fvoci.app_remove_member($1,$2,@ids)",
        &[w.ws, member.id.0],
    )
    .await
    .unwrap();
    let holder = pid(remove.conn()).await;
    let mut share = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(share.conn()).await;
    let args = [doc, member.id.0, w.org];
    let pending = tokio::spawn(async move {
        on(share.conn(), "SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'write',$3,@ids)", &args).await
    });
    blocked(holder, waiter).await;
    remove.commit().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    code(&result.unwrap_err(), "22023");
}
