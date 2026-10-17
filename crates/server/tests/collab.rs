#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 실시간 공동 편집(D-27, D-70): 실제 웹소켓으로 y-sync를 주고받는다. 권한·원점·켜짐 확인, 두 작성자 수렴,
//! 읽기 연결의 쓰기 거부, 내용 검사 실패 거부, 마지막 사람이 나가면 저장 API와 같은 길로 리비전, 실시간 중 저장
//! API는 제목만, 방을 다시 열어도 내용 유지, 복원하면 방 재시작, 권한·세션이 끝나면 연결 닫힘, 끄면 저장 후 닫힘.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, config_with, create_workspace, login, new_request_id, send};
use futures_util::{SinkExt, StreamExt};
use fvoci_server::collab::convert::{FRAGMENT, read_doc};
use serde_json::json;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use yrs::sync::{Message, MessageReader, SyncMessage};
use yrs::types::xml::{Xml, XmlElementPrelim, XmlFragment, XmlTextPrelim};
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, Transact, Update};

async fn serve(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

struct Client {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    doc: Doc,
}

/// 연결. 실패하면 HTTP 상태.
async fn connect(
    addr: SocketAddr,
    user: Option<&TestUser>,
    doc: &str,
    origin: bool,
) -> Result<Client, StatusCode> {
    connect_with(addr, user, doc, origin, Doc::new()).await
}

/// 이미 가진 Yjs 문서로 다시 연결한다(끊겼다 다시 붙는 클라이언트).
async fn connect_with(
    addr: SocketAddr,
    user: Option<&TestUser>,
    doc: &str,
    origin: bool,
    local: Doc,
) -> Result<Client, StatusCode> {
    let mut request = format!("ws://{addr}/api/documents/{doc}/collab")
        .into_client_request()
        .unwrap();
    if let Some(user) = user {
        request
            .headers_mut()
            .insert("cookie", HeaderValue::from_str(&user.cookie).unwrap());
    }
    if origin {
        request
            .headers_mut()
            .insert("origin", HeaderValue::from_static(common::PUBLIC_URL));
    }
    match tokio_tungstenite::connect_async(request).await {
        Ok((ws, _)) => {
            let mut client = Client { ws, doc: local };
            client.sync().await;
            Ok(client)
        }
        Err(WsError::Http(response)) => {
            Err(StatusCode::from_u16(response.status().as_u16()).unwrap())
        }
        Err(err) => panic!("websocket error {err}"),
    }
}

enum Event {
    Data,
    Closed(u16),
    Timeout,
}

impl Client {
    async fn send(&mut self, message: Message) {
        self.ws
            .send(WsMessage::Binary(message.encode_v1().into()))
            .await
            .unwrap();
    }

    /// 서버의 첫 메시지에 답하고 내 상태 차이를 받을 때까지 기다린다.
    async fn sync(&mut self) {
        let sv = self.doc.transact().state_vector();
        self.send(Message::Sync(SyncMessage::SyncStep1(sv))).await;
        for _ in 0..20 {
            if let Event::Data = self.next(Duration::from_secs(5)).await {
                if !self.text().is_empty() && self.text() != r#"{"type":"doc"}"# {
                    return;
                }
            } else {
                return;
            }
        }
    }

    async fn next(&mut self, wait: Duration) -> Event {
        let Ok(next) = tokio::time::timeout(wait, self.ws.next()).await else {
            return Event::Timeout;
        };
        match next {
            Some(Ok(WsMessage::Binary(bytes))) => {
                let mut decoder = DecoderV1::from(bytes.as_ref());
                let messages: Vec<Message> = MessageReader::new(&mut decoder)
                    .collect::<Result<_, _>>()
                    .unwrap();
                for message in messages {
                    match message {
                        Message::Sync(SyncMessage::SyncStep1(sv)) => {
                            let diff = self.doc.transact().encode_state_as_update_v1(&sv);
                            self.send(Message::Sync(SyncMessage::SyncStep2(diff))).await;
                        }
                        Message::Sync(SyncMessage::SyncStep2(u) | SyncMessage::Update(u)) => {
                            let update = Update::decode_v1(&u).unwrap();
                            self.doc.transact_mut().apply_update(update).unwrap();
                        }
                        _ => {}
                    }
                }
                Event::Data
            }
            Some(Ok(WsMessage::Close(frame))) => {
                Event::Closed(frame.map_or(0, |f| u16::from(f.code)))
            }
            Some(Ok(_)) => Event::Data,
            Some(Err(_)) | None => Event::Closed(0),
        }
    }

    /// 조건이 맞을 때까지 받는다.
    async fn until(&mut self, mut done: impl FnMut(&Self) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !done(self) {
            if tokio::time::Instant::now() > deadline {
                return false;
            }
            if let Event::Closed(_) = self.next(Duration::from_millis(500)).await {
                return done(self);
            }
        }
        true
    }

    /// 닫힐 때까지 받아 닫기 코드.
    async fn closed(&mut self) -> Option<u16> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while tokio::time::Instant::now() < deadline {
            if let Event::Closed(code) = self.next(Duration::from_millis(500)).await {
                return Some(code);
            }
        }
        None
    }

    fn text(&self) -> String {
        let fragment = self.doc.get_or_insert_xml_fragment(FRAGMENT);
        read_doc(&self.doc.transact(), &fragment).to_string()
    }

    /// 내 문서를 바꾸고 그 변경만 보낸다.
    async fn edit(&mut self, change: impl FnOnce(&mut yrs::TransactionMut, &yrs::XmlFragmentRef)) {
        let before = self.doc.transact().state_vector();
        let fragment = self.doc.get_or_insert_xml_fragment(FRAGMENT);
        change(&mut self.doc.transact_mut(), &fragment);
        let diff = self.doc.transact().encode_state_as_update_v1(&before);
        self.send(Message::Sync(SyncMessage::Update(diff))).await;
    }

    async fn paragraph(&mut self, text: &'static str) {
        self.edit(|txn, fragment| {
            let p = fragment.push_back(txn, XmlElementPrelim::empty("paragraph"));
            p.push_back(txn, XmlTextPrelim::new(text));
        })
        .await;
    }

    async fn close(mut self) {
        let _ignored = self.ws.close(None).await;
    }
}

#[tokio::test]
async fn live_editing_end_to_end() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "1")])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "실시간").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (writer2, _) = add_member(&app, &state, &admin, &ws, "writer2", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let outsider = login(&state, "outsider").await;
    let doc = common::create_doc(&app, &writer, &ws, "함께 쓰는 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let doc_uri = format!("/api/documents/{id}");
    let saved = send(
        &app,
        Method::PUT,
        &doc_uri,
        Some(&writer),
        Some(json!({
        "request_id": new_request_id(), "base_revision": 1, "title": "함께 쓰는 문서",
        "body": common::doc_body("시작") })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);

    // 꺼져 있으면 409. 켜고 끄기는 관리자만.
    assert_eq!(
        connect(addr, Some(&writer), &id, true).await.err(),
        Some(StatusCode::CONFLICT)
    );
    let realtime = format!("/api/workspaces/{ws}/realtime");
    let set = |user: &TestUser, on: bool| {
        let (app, user, uri) = (app.clone(), user.clone(), realtime.clone());
        async move {
            send(
                &app,
                Method::PUT,
                &uri,
                Some(&user),
                Some(json!({ "enabled": on })),
            )
            .await
            .status
        }
    };
    assert_eq!(set(&writer, true).await, StatusCode::FORBIDDEN);
    assert_eq!(set(&admin, true).await, StatusCode::NO_CONTENT);
    let got = send(&app, Method::GET, &doc_uri, Some(&writer), None).await;
    assert_eq!(got.json["realtime"], true);

    // 연결 전 확인: 밖의 사람 404, 원점 없음 403, 세션 없음 401.
    assert_eq!(
        connect(addr, Some(&outsider), &id, true).await.err(),
        Some(StatusCode::NOT_FOUND)
    );
    assert_eq!(
        connect(addr, Some(&writer), &id, false).await.err(),
        Some(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        connect(addr, None, &id, true).await.err(),
        Some(StatusCode::UNAUTHORIZED)
    );

    // 두 작성자는 같은 내용을 받고 서로의 변경으로 수렴한다.
    let mut a = connect(addr, Some(&writer), &id, true).await.unwrap();
    let mut b = connect(addr, Some(&writer2), &id, true).await.unwrap();
    assert!(a.text().contains("시작"), "{}", a.text());
    assert!(b.text().contains("시작"));
    a.paragraph("A가 씀").await;
    assert!(
        b.until(|c| c.text().contains("A가 씀")).await,
        "{}",
        b.text()
    );
    b.paragraph("B가 씀").await;
    assert!(a.until(|c| c.text().contains("B가 씀")).await);
    assert_eq!(a.text(), b.text());

    // 읽기 연결은 내용을 받지만 쓰면 닫힌다.
    let mut r = connect(addr, Some(&reader), &id, true).await.unwrap();
    assert!(r.text().contains("B가 씀"));
    r.paragraph("독자가 씀").await;
    assert_eq!(r.closed().await, Some(4400));

    // 내용 검사를 통과하지 못하는 변경(바깥 주소 그림)은 버리고 그 연결을 닫는다.
    let mut bad = connect(addr, Some(&writer2), &id, true).await.unwrap();
    bad.edit(|txn, fragment| {
        let img = fragment.push_back(txn, XmlElementPrelim::empty("image"));
        img.insert_attribute(txn, "src", "https://tracker.example/pixel.png");
    })
    .await;
    assert_eq!(bad.closed().await, Some(4400));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = a.next(Duration::from_millis(300)).await;
    assert!(
        !a.text().contains("독자가 씀") && !a.text().contains("tracker"),
        "{}",
        a.text()
    );

    // 실시간 중 저장 API는 본문을 바꾸지 못하고 제목만 바꾼다.
    let current = send(&app, Method::GET, &doc_uri, Some(&writer), None)
        .await
        .json;
    let body_change = send(&app, Method::PUT, &doc_uri, Some(&writer), Some(json!({
        "request_id": new_request_id(), "base_revision": current["revision"], "title": "함께 쓰는 문서",
        "body": common::doc_body("저장 API") }))).await;
    assert_eq!(
        body_change.status,
        StatusCode::CONFLICT,
        "{:?}",
        body_change.json
    );
    let title_only = send(
        &app,
        Method::PUT,
        &doc_uri,
        Some(&writer),
        Some(json!({
        "request_id": new_request_id(), "base_revision": current["revision"], "title": "새 제목",
        "body": current["body"] })),
    )
    .await;
    assert_eq!(title_only.status, StatusCode::OK, "{:?}", title_only.json);

    // 마지막 사람이 나가면 저장 API와 같은 길로 리비전이 생긴다(마지막으로 고친 사람이 작성자).
    a.close().await;
    b.close().await;
    let mut snapshot = None;
    for _ in 0..40 {
        let d = send(&app, Method::GET, &doc_uri, Some(&writer), None)
            .await
            .json;
        if d["body"].to_string().contains("B가 씀") {
            snapshot = Some(d);
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let snapshot = snapshot.expect("live document saved as a revision");
    assert_eq!(snapshot["title"], "새 제목");
    assert!(snapshot["body"].to_string().contains("A가 씀"));
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let author: uuid::Uuid = sqlx::query_scalar(
        "SELECT author FROM fvoci.document_revisions WHERE document_id = $1::uuid ORDER BY revision DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(author, writer2.id.0);

    // 방을 다시 열어도 내용이 그대로다.
    let mut c = connect(addr, Some(&writer), &id, true).await.unwrap();
    assert!(
        c.text().contains("A가 씀") && c.text().contains("B가 씀"),
        "{}",
        c.text()
    );

    // 복원하면 열린 방은 저장 없이 닫히고, 다시 열면 복원된 본문이다.
    let latest = send(&app, Method::GET, &doc_uri, Some(&writer), None)
        .await
        .json;
    let restored = send(
        &app,
        Method::POST,
        &format!("{doc_uri}/revisions/2/restore"),
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "base_revision": latest["revision"] })),
    )
    .await;
    assert_eq!(restored.status, StatusCode::OK, "{:?}", restored.json);
    assert_eq!(c.closed().await, Some(4409));
    let mut d = connect(addr, Some(&writer2), &id, true).await.unwrap();
    assert!(
        d.text().contains("시작") && !d.text().contains("A가 씀"),
        "{}",
        d.text()
    );
    // 복원 전 상태를 들고 다시 붙은 클라이언트는 옛 내용을 섞지 못하고 다시 열라는 닫기를 받는다.
    let mut stale = connect_with(addr, Some(&writer), &id, true, c.doc)
        .await
        .unwrap();
    assert_eq!(stale.closed().await, Some(4409));
    let _ = d.next(Duration::from_millis(500)).await;
    assert!(!d.text().contains("A가 씀"), "{}", d.text());

    // 워크스페이스에서 빠지면(권한 끝) 연결이 닫힌다.
    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", writer2.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(removed.status.is_success(), "{:?}", removed.json);
    assert_eq!(d.closed().await, Some(4403));

    // 로그아웃한 세션의 연결도 닫힌다.
    let mut e = connect(addr, Some(&writer), &id, true).await.unwrap();
    let logout = send(&app, Method::POST, "/auth/logout", Some(&writer), None).await;
    assert!(logout.status.is_success(), "{:?}", logout.json);
    assert_eq!(e.closed().await, Some(4403));

    // 끄면 열린 방은 리비전을 남기고 닫히며, 그 뒤로는 연결할 수 없다.
    let (writer3, _) = add_member(&app, &state, &admin, &ws, "writer3", "writer").await;
    let mut f = connect(addr, Some(&writer3), &id, true).await.unwrap();
    f.paragraph("끄기 전").await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(set(&admin, false).await, StatusCode::NO_CONTENT);
    assert_eq!(f.closed().await, Some(4410));
    let after = send(&app, Method::GET, &doc_uri, Some(&writer3), None)
        .await
        .json;
    assert!(after["body"].to_string().contains("끄기 전"), "{after:?}");
    assert_eq!(after["realtime"], false);
    assert_eq!(
        connect(addr, Some(&writer3), &id, true).await.err(),
        Some(StatusCode::CONFLICT)
    );
    owner.close().await;
}
