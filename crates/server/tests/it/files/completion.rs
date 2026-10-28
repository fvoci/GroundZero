//! 마무리 점유를 넘겨받은 뒤 늦은 시도의 저장을 막는다.
use super::fixtures::{part, sha, start, state_with};
use crate::common::{self, TestUser, create_doc, create_workspace, login, new_request_id, send};
use axum::http::{Method, StatusCode};
use fvoci_server::http::AppState;
use serde_json::json;
/// 사용자로서 마치기를 시작하고(점유) 그 시도의 점유 시각을 돌려준다.
async fn begin_complete(
    state: &AppState,
    user: &TestUser,
    upload: uuid::Uuid,
) -> time::OffsetDateTime {
    let mut tx = state.db.begin_as(user.id).await.unwrap();
    let claim: time::OffsetDateTime =
        sqlx::query_scalar("SELECT state_changed_at FROM app_begin_complete($1)")
            .bind(upload)
            .fetch_one(tx.conn())
            .await
            .unwrap();
    tx.commit().await.unwrap();
    claim
}

/// 리뷰(감사 S11 후속): 오래 걸린 앞 마치기가 5분 뒤 다른 요청에 넘겨진 다음 늦게 실패해 점유를 풀면, 넘겨받은 요청의
/// 점유까지 풀려 세 번째 마치기가 동시에 저장소 작업을 시작할 수 있었다(한쪽이 조각을 지우고 업로드를 되돌릴 수 있음).
/// 점유는 시도마다 구별하고, 늦게 돌아온 시도는 넘겨받은 점유를 건드리지 않는다.
#[tokio::test]
async fn a_late_attempt_leaves_a_claim_taken_over_by_another_alone() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "넘겨받은 마무리").await;
    let doc = create_doc(&app, &admin, &ws, "첨부 문서").await;
    let data = vec![9_u8; 3229];
    let (status, up) = start(
        &app,
        &admin,
        json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                "name": "자료.bin", "size": data.len(), "content_type": "application/octet-stream" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{up:?}");
    let id = up["id"].as_str().unwrap().to_owned();
    assert_eq!(
        part(&app, &admin, &id, 1, &data, &sha(&data)).await,
        StatusCode::NO_CONTENT
    );
    let upload: uuid::Uuid = id.parse().unwrap();

    // 첫 시도가 점유한 뒤 오래 걸려, 5분이 지나 다른 요청이 넘겨받는다.
    let first = begin_complete(&state, &admin, upload).await;
    let mut o = common::fixture_conn().await;
    sqlx::query(
        "UPDATE fvoci.uploads SET state_changed_at = state_changed_at - interval '6 minutes' WHERE id = $1",
    )
    .bind(upload)
    .execute(&mut o)
    .await
    .unwrap();
    let second = begin_complete(&state, &admin, upload).await;
    assert_ne!(first, second);

    // 첫 시도가 늦게 실패해 점유를 풀려 해도 넘겨받은 점유는 그대로다.
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    sqlx::query("SELECT app_release_upload_claim($1, $2)")
        .bind(upload)
        .bind(first)
        .execute(tx.conn())
        .await
        .unwrap();
    let held = |claim: time::OffsetDateTime| {
        sqlx::query_scalar::<_, bool>("SELECT app_upload_claim_held($1, $2)")
            .bind(upload)
            .bind(claim)
    };
    let first_holds = held(first).fetch_one(tx.conn()).await.unwrap();
    let second_holds = held(second).fetch_one(tx.conn()).await.unwrap();
    tx.commit().await.unwrap();
    assert!(!first_holds, "the late attempt must not hold the claim");
    assert!(second_holds, "the take-over keeps its claim");
    let uri = format!("/api/uploads/{id}/complete");
    let third = send(&app, Method::POST, &uri, Some(&admin), None).await;
    assert_eq!(third.status, StatusCode::CONFLICT, "{:?}", third.json);

    // 넘겨받은 시도가 멈춰 스스로 풀면 다음 마치기가 이어서 끝낸다.
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    sqlx::query("SELECT app_release_upload_claim($1, $2)")
        .bind(upload)
        .bind(second)
        .execute(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let again = send(&app, Method::POST, &uri, Some(&admin), None).await;
    assert_eq!(again.status, StatusCode::OK, "{:?}", again.json);
    assert_eq!(again.json["state"], "done");
}

/// #65 리뷰(감사 S11 후속): 로컬 저장소는 마칠 때 조각을 임시 파일에 이은 뒤 최종 키로 이름을 바꿨다. 앞 시도가 잇는
/// 도중 오래 멈춰 5분 뒤 다른 요청이 넘겨받고, 그 요청이 조각 오류로 업로드를 되돌린 뒤 다른 바이트로 다시 받아 마치면,
/// 늦게 돌아온 앞 시도의 이름 바꾸기가 최종 키를 덮어 저장된 바이트와 기록된 SHA-256이 어긋났다. 최종 키로 게시하는 것은
/// 점유를 쥔 시도뿐이다. 앞 시도는 첫 조각 자리에 둔 FIFO에서 읽다가 멈춘다.
#[cfg(unix)]
#[tokio::test]
async fn a_late_attempt_cannot_overwrite_what_the_take_over_stored() {
    let dir = std::env::temp_dir().join(format!("fvoci-files-{}", uuid::Uuid::now_v7().simple()));
    let state = state_with(&[("FVOCI_STORAGE_DIR", dir.to_str().unwrap())]);
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "늦은 게시").await;
    let doc = create_doc(&app, &admin, &ws, "첨부 문서").await;
    let old = vec![3_u8; 3233];
    let new = vec![4_u8; 3233];
    let (status, up) = start(
        &app,
        &admin,
        json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                "name": "자료.bin", "size": old.len(), "content_type": "application/octet-stream" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{up:?}");
    let id = up["id"].as_str().unwrap().to_owned();
    assert_eq!(
        part(&app, &admin, &id, 1, &old, &sha(&old)).await,
        StatusCode::NO_CONTENT
    );
    let upload: uuid::Uuid = id.parse().unwrap();
    let mut o = common::fixture_conn().await;
    let (key, storage_upload): (String, String) =
        sqlx::query_as("SELECT storage_key, storage_upload_id FROM fvoci.uploads WHERE id = $1")
            .bind(upload)
            .fetch_one(&mut o)
            .await
            .unwrap();
    let first_part = dir.join("uploads").join(&storage_upload).join("00001");
    std::fs::remove_file(&first_part).unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(&first_part)
        .status()
        .unwrap();
    assert!(made.success());

    // 앞 시도가 점유하고 첫 조각을 읽다가 멈춘다(FIFO에 쓰는 쪽이 열리면 읽는 쪽도 열린 것이다).
    let uri = format!("/api/uploads/{id}/complete");
    let late = {
        let (app, admin, uri) = (app.clone(), admin.clone(), uri.clone());
        tokio::spawn(async move { send(&app, Method::POST, &uri, Some(&admin), None).await })
    };
    let writer = {
        let path = first_part.clone();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || std::fs::OpenOptions::new().write(true).open(path)),
        )
        .await
        .expect("the first attempt never started reading the parts")
        .unwrap()
        .unwrap()
    };

    // 5분이 지나 다음 시도가 넘겨받는다. 첫 조각이 없어 잇지 못하고 업로드를 되돌린다.
    sqlx::query(
        "UPDATE fvoci.uploads SET state_changed_at = state_changed_at - interval '6 minutes' WHERE id = $1",
    )
    .bind(upload)
    .execute(&mut o)
    .await
    .unwrap();
    std::fs::remove_file(&first_part).unwrap();
    let take_over = send(&app, Method::POST, &uri, Some(&admin), None).await;
    assert_eq!(
        take_over.status,
        StatusCode::CONFLICT,
        "{:?}",
        take_over.json
    );

    // 다른 바이트를 다시 받아 마친다.
    assert_eq!(
        part(&app, &admin, &id, 1, &new, &sha(&new)).await,
        StatusCode::NO_CONTENT
    );
    let done = send(&app, Method::POST, &uri, Some(&admin), None).await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    assert_eq!(done.json["state"], "done");
    let recorded: String =
        sqlx::query_scalar("SELECT sha256 FROM fvoci.file_versions WHERE storage_key = $1")
            .bind(&key)
            .fetch_one(&mut o)
            .await
            .unwrap();
    assert_eq!(recorded, sha(&new));

    // 앞 시도가 남은 조각을 다 읽고 늦게 돌아온다. 점유를 잃었으니 최종 키를 덮지 못한다.
    {
        use std::io::Write as _;
        let mut writer = writer;
        writer.write_all(&old).unwrap();
    }
    let late = tokio::time::timeout(std::time::Duration::from_secs(30), late)
        .await
        .expect("the first attempt never came back")
        .unwrap();
    assert!(!late.status.is_server_error(), "{:?}", late.json);
    let (stored, size) = state.storage.digest(&key).await.unwrap();
    assert_eq!(size, new.len() as u64);
    assert_eq!(
        stored, recorded,
        "the late attempt overwrote the stored content"
    );
    assert!(
        !dir.join("uploads").join(&storage_upload).exists(),
        "the received parts were left behind"
    );
    let _ignored = std::fs::remove_dir_all(&dir);
}
