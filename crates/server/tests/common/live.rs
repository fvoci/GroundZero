//! 다른 영역 시험의 본문 fixture도 실제 WebSocket·CRDT·리비전 경로를 쓴다(D-27).

use super::{PUBLIC_URL, Reply, TestUser, send};
use axum::{
    Router,
    http::{Method, StatusCode},
};
use futures_util::{SinkExt, StreamExt};
use fvoci_server::collab::convert::{FRAGMENT, write_doc};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message as Frame, client::IntoClientRequest};
use yrs::sync::{Message, MessageReader, SyncMessage};
use yrs::types::xml::XmlFragment;
use yrs::updates::{
    decoder::{Decode, DecoderV1},
    encoder::Encode,
};
use yrs::{Doc, ReadTxn, Transact, Update};

/// 유효한 본문 fixture를 실시간으로 바꾸고 마지막 연결 종료의 리비전을 기다린다.
/// 권한 거절은 실제 제목 PATCH 응답으로 확인한다. 잘못된 CRDT 입력의 닫기 코드는 collab 시험이 확인한다.
pub async fn save(app: &Router, user: &TestUser, uri: &str, input: &Value) -> Reply {
    let mut current = send(app, Method::GET, uri, Some(user), None).await;
    if !current.status.is_success() {
        return current;
    }
    if current.json["can_edit"] != true || current.json["title"] != input["title"] {
        current = send(app, Method::PATCH, uri, Some(user), Some(json!({
            "request_id": input["request_id"], "base_revision": current.json["revision"], "title": input["title"]
        }))).await;
        if !current.status.is_success() {
            return current;
        }
    }
    if current.json["body"] == input["body"] {
        return current;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = app.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut request = format!(
        "ws://{addr}{uri}/collab?write=1&expected_user={}",
        user.id.0
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("cookie", user.cookie.parse().unwrap());
    request
        .headers_mut()
        .insert("origin", PUBLIC_URL.parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let doc = Doc::new();
    let sv = doc.transact().state_vector();
    ws.send(Frame::Binary(
        Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1().into(),
    ))
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let frame = ws.next().await.unwrap().unwrap();
            let Frame::Binary(bytes) = frame else {
                return Err(format!("sync refused: {frame:?}"));
            };
            let mut decoder = DecoderV1::from(bytes.as_ref());
            let messages = MessageReader::new(&mut decoder)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            for message in messages {
                match message {
                    Message::Sync(SyncMessage::SyncStep1(sv)) => {
                        let diff = doc.transact().encode_state_as_update_v1(&sv);
                        ws.send(Frame::Binary(
                            Message::Sync(SyncMessage::SyncStep2(diff))
                                .encode_v1()
                                .into(),
                        ))
                        .await
                        .unwrap();
                    }
                    Message::Sync(SyncMessage::SyncStep2(update)) => {
                        doc.transact_mut()
                            .apply_update(Update::decode_v1(&update).unwrap())
                            .unwrap();
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("live fixture sync deadline")
    .expect("live fixture sync must succeed");
    let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
    let before = doc.transact().state_vector();
    {
        let mut tx = doc.transact_mut();
        let len = fragment.len(&tx);
        fragment.remove_range(&mut tx, 0, len);
        write_doc(&mut tx, &fragment, &input["body"]);
    }
    let update = doc.transact().encode_state_as_update_v1(&before);
    ws.send(Frame::Binary(
        Message::Sync(SyncMessage::Update(update))
            .encode_v1()
            .into(),
    ))
    .await
    .unwrap();
    // Ping 응답은 서버가 앞선 편집 메시지를 방에 보낸 뒤에 온다. 마지막 연결을 닫으면 방이 리비전을 남긴다.
    ws.send(Frame::Ping(vec![1].into())).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(frame) = ws.next().await {
            match frame.unwrap() {
                Frame::Pong(_) => return Ok(()),
                Frame::Close(reason) => return Err(format!("live fixture rejected: {reason:?}")),
                _ => {}
            }
        }
        Err("live fixture closed before ping reply".to_owned())
    })
    .await
    .expect("live fixture ping deadline")
    .expect("live fixture edit must be accepted");
    ws.close(None).await.unwrap();
    drop(ws);
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let reply = send(app, Method::GET, uri, Some(user), None).await;
            if reply.json["revision"].as_i64() > current.json["revision"].as_i64() {
                assert_eq!(reply.status, StatusCode::OK);
                return reply;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("live fixture revision deadline");
    server.abort();
    result
}
