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

/// 자동 정리(`app_system_purge_old_versions`)는 DB 전체를 본다. 그것을 부르는 시험끼리는 차례로 돌아 서로의 정리 대상을
/// 지우거나 세지 않게 한다.
static GLOBAL_PURGE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn serve(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

struct Client {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    doc: Doc,
    /// 받은 참여자 표시(client id → JSON 글).
    presence: std::collections::HashMap<u64, String>,
    /// 받은 저장 확인(서버가 DB에 저장한 상태의 스냅숏, v1).
    persisted: Vec<Vec<u8>>,
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
            let mut client = Client {
                ws,
                doc: local,
                presence: std::collections::HashMap::new(),
                persisted: Vec::new(),
            };
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
                // 브라우저(y-websocket)는 한 프레임에서 첫 메시지만 읽는다. 서버는 메시지마다 프레임을 따로 보내야 한다.
                assert_eq!(messages.len(), 1, "one y-sync message per frame");
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
                        Message::Custom(tag, data)
                            if tag == fvoci_server::collab::room::MSG_PERSISTED =>
                        {
                            self.persisted.push(data);
                        }
                        Message::Awareness(update) => {
                            for (id, entry) in update.clients {
                                self.presence.insert(id.get(), entry.json.to_string());
                            }
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

    // 읽기 연결은 내용을 받지만 그 변경은 받지 않는다(편집기가 스스로 맞춘 것일 수 있어 연결은 그대로).
    let mut r = connect(addr, Some(&reader), &id, true).await.unwrap();
    assert!(r.text().contains("B가 씀"));
    r.paragraph("독자가 씀").await;
    assert!(matches!(
        r.next(Duration::from_millis(700)).await,
        Event::Timeout | Event::Data
    ));
    r.close().await;

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

impl Client {
    async fn send_raw_frame(&mut self, bytes: Vec<u8>) {
        self.ws.send(WsMessage::Binary(bytes.into())).await.unwrap();
    }

    async fn announce(&mut self, client: u64, clock: u32, json: &str) {
        let mut clients = std::collections::HashMap::new();
        clients.insert(
            yrs::block::ClientID::new(client),
            yrs::sync::awareness::AwarenessUpdateEntry {
                clock,
                json: std::sync::Arc::from(json),
            },
        );
        self.send(Message::Awareness(yrs::sync::AwarenessUpdate { clients }))
            .await;
    }
}

/// 리뷰 반영(D-70): 편집기 스키마 밖의 변경(해시 붙은 표시 이름, 모르는 최상위·노드), 한 프레임의 메시지 수,
/// 앞선 변경이 빠진 변경, 참여자 표시 가로채기를 막고, 마지막으로 고친 사람이 쓸 수 없게 되면 다른 작성자로
/// 리비전을 남긴다.
#[tokio::test]
async fn live_editing_guards() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "1")])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "실시간 경계").await;
    let (w1, _) = add_member(&app, &state, &admin, &ws, "w1", "writer").await;
    let (w2, _) = add_member(&app, &state, &admin, &ws, "w2", "writer").await;
    let doc = common::create_doc(&app, &w1, &ws, "경계 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{id}"),
        Some(&w1),
        Some(json!({
        "request_id": new_request_id(), "base_revision": 1, "title": "경계 문서",
        "body": common::doc_body("처음") })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK);
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let mut watcher = connect(addr, Some(&w2), &id, true).await.unwrap();

    // 해시 붙은 표시 이름(브라우저가 `link`로 읽음)으로 링크 검사를 피하려 하면 닫는다.
    let mut bad = connect(addr, Some(&w1), &id, true).await.unwrap();
    bad.edit(|txn, fragment| {
        let p = fragment.push_back(txn, XmlElementPrelim::empty("paragraph"));
        let t = p.push_back(txn, XmlTextPrelim::new(""));
        let mut attrs = std::collections::HashMap::new();
        let mut href = std::collections::HashMap::new();
        href.insert("href".to_owned(), yrs::Any::from("//evil.example/login"));
        attrs.insert(std::sync::Arc::from("link--AAAAAAAA"), yrs::Any::from(href));
        yrs::Text::insert_with_attributes(&t, txn, 0, "가짜 링크", attrs);
    })
    .await;
    assert_eq!(bad.closed().await, Some(4400));

    // 모르는 최상위 이름, 모르는 노드 이름도 닫는다.
    let mut bad = connect(addr, Some(&w1), &id, true).await.unwrap();
    {
        let before = bad.doc.transact().state_vector();
        let junk = bad.doc.get_or_insert_map("junk");
        yrs::Map::insert(&junk, &mut bad.doc.transact_mut(), "big", "x".repeat(1000));
        let diff = bad.doc.transact().encode_state_as_update_v1(&before);
        bad.send(Message::Sync(SyncMessage::Update(diff))).await;
    }
    assert_eq!(bad.closed().await, Some(4400));
    let mut bad = connect(addr, Some(&w1), &id, true).await.unwrap();
    bad.edit(|txn, fragment| {
        fragment.push_back(txn, XmlElementPrelim::empty("script"));
    })
    .await;
    assert_eq!(bad.closed().await, Some(4400));

    // 한 프레임에 메시지를 너무 많이 담으면 닫는다.
    let mut bad = connect(addr, Some(&w1), &id, true).await.unwrap();
    let mut frame = Vec::new();
    for _ in 0..20 {
        frame.extend(Message::AwarenessQuery.encode_v1());
    }
    bad.send_raw_frame(frame).await;
    assert_eq!(bad.closed().await, Some(4400));

    // 앞선 변경이 빠진 변경(서버와 어긋난 클라이언트)은 새로 열게 한다.
    let mut gap = connect(addr, Some(&w1), &id, true).await.unwrap();
    {
        let fragment = gap.doc.get_or_insert_xml_fragment(FRAGMENT);
        let p = fragment.push_back(
            &mut gap.doc.transact_mut(),
            XmlElementPrelim::empty("paragraph"),
        );
        let before = gap.doc.transact().state_vector();
        p.push_back(
            &mut gap.doc.transact_mut(),
            XmlTextPrelim::new("앞이 빠진 글"),
        );
        let diff = gap.doc.transact().encode_state_as_update_v1(&before);
        gap.send(Message::Sync(SyncMessage::Update(diff))).await;
    }
    assert_eq!(gap.closed().await, Some(4409));
    let _ = watcher.next(Duration::from_millis(300)).await;
    let seen = watcher.text();
    assert!(
        !seen.contains("가짜") && !seen.contains("script") && !seen.contains("앞이 빠진"),
        "{seen}"
    );

    // 참여자 표시: 다른 연결이 알린 id는 가로챌 수 없다.
    let mut a = connect(addr, Some(&w1), &id, true).await.unwrap();
    a.announce(777, 1, r#"{"user":{"name":"w1"}}"#).await;
    let _ = watcher.until(|c| c.presence.contains_key(&777)).await;
    let mut spoof = connect(addr, Some(&w2), &id, true).await.unwrap();
    spoof.announce(777, 9, r#"{"user":{"name":"가짜"}}"#).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut fresh = connect(addr, Some(&w2), &id, true).await.unwrap();
    let _ = fresh.until(|c| c.presence.contains_key(&777)).await;
    assert_eq!(
        fresh.presence.get(&777).map(String::as_str),
        Some(r#"{"user":{"name":"w1"}}"#)
    );
    spoof.close().await;
    fresh.close().await;

    // 두 작성자가 고친 뒤 마지막으로 고친 사람(w1)이 독자로 바뀌면, 리비전은 다른 작성자(w2)로 남는다.
    watcher.paragraph("w2가 씀").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    a.paragraph("w1이 마지막에 씀").await;
    assert!(
        watcher
            .until(|c| c.text().contains("w1이 마지막에 씀"))
            .await
    );
    let demoted = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/members/{}", w1.id.0),
        Some(&admin),
        Some(json!({ "role": "reader" })),
    )
    .await;
    assert!(demoted.status.is_success(), "{:?}", demoted.json);
    // 쓰기 권한을 잃은 연결은 다음 확인에서 닫힌다(열어 두면 그 사람의 입력이 조용히 버려진다, 감사 반영).
    assert_eq!(a.closed().await, Some(4403));
    // 끊긴 사이 독자가 된 사람이 편집기로 다시 붙으면(write=1) 붙지 않고 4403(화면이 지금 권한으로 다시 열림).
    // 읽기로 붙는 것은 된다.
    assert_eq!(
        refused_code_with(addr, &w1, &id, "?write=1").await,
        Some(4403)
    );
    connect(addr, Some(&w1), &id, true)
        .await
        .unwrap()
        .close()
        .await;
    watcher.close().await;
    let mut body = String::new();
    for _ in 0..40 {
        let d = send(
            &app,
            Method::GET,
            &format!("/api/documents/{id}"),
            Some(&admin),
            None,
        )
        .await
        .json;
        body = d["body"].to_string();
        if body.contains("w1이 마지막에 씀") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(
        body.contains("w1이 마지막에 씀") && body.contains("w2가 씀"),
        "{body}"
    );
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let author: uuid::Uuid = sqlx::query_scalar(
        "SELECT author FROM fvoci.document_revisions WHERE document_id = $1::uuid ORDER BY revision DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(author, w2.id.0);
    owner.close().await;
}

/// 연결이 곧바로 닫히는지 보고 닫기 코드를 돌려준다(방에 들어가지 못한 경우).
async fn refused_code(addr: SocketAddr, user: &TestUser, doc: &str) -> Option<u16> {
    refused_code_with(addr, user, doc, "").await
}

/// 질의 문자열(`?write=1` 등)을 붙여 연결한다.
async fn refused_code_with(
    addr: SocketAddr,
    user: &TestUser,
    doc: &str,
    query: &str,
) -> Option<u16> {
    let mut request = format!("ws://{addr}/api/documents/{doc}/collab{query}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("cookie", HeaderValue::from_str(&user.cookie).unwrap());
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static(common::PUBLIC_URL));
    let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let mut client = Client {
        ws,
        doc: Doc::new(),
        presence: std::collections::HashMap::new(),
        persisted: Vec::new(),
    };
    client.closed().await
}

/// 실시간 끄기 회귀(사용자 지시 2026-10-17, 리뷰 반영): 끄는 동안 새 방이 열리지 않고(4503), 열린 방과 방 없이 남은
/// 실시간 상태의 편집이 모두 리비전으로 남은 뒤에만 꺼진다. 편집한 사람이 더는 쓸 수 없으면 끄는 관리자가 작성자가
/// 된다. 같은 워크스페이스를 동시에 끄는 요청이 겹쳐도 편집이 남는다. 끈 뒤 저장 API는 그 리비전 위에서만 성공한다.
#[tokio::test]
async fn turning_realtime_off_keeps_live_edits() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "1")])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "끄기 보존").await;
    let ws_id = uuid::Uuid::parse_str(&ws).unwrap();
    let (w1, _) = add_member(&app, &state, &admin, &ws, "w1", "writer").await;
    let (r1, _) = add_member(&app, &state, &admin, &ws, "r1", "reader").await;
    let set = |enabled: bool| {
        let app = app.clone();
        let admin = admin.clone();
        let ws = ws.clone();
        async move {
            send(
                &app,
                Method::PUT,
                &format!("/api/workspaces/{ws}/realtime"),
                Some(&admin),
                Some(json!({ "enabled": enabled })),
            )
            .await
            .status
        }
    };
    let role = |r: &'static str| {
        let app = app.clone();
        let admin = admin.clone();
        let ws = ws.clone();
        let user = w1.id.0;
        async move {
            let res = send(
                &app,
                Method::PUT,
                &format!("/api/workspaces/{ws}/members/{user}"),
                Some(&admin),
                Some(json!({ "role": r })),
            )
            .await;
            assert!(res.status.is_success(), "{:?}", res.json);
        }
    };
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let last_author = |id: String| {
        let owner = owner.clone();
        async move {
            let author: uuid::Uuid = sqlx::query_scalar(
                "SELECT author FROM fvoci.document_revisions WHERE document_id = $1::uuid ORDER BY revision DESC LIMIT 1",
            )
            .bind(id)
            .fetch_one(&owner)
            .await
            .unwrap();
            author
        }
    };
    let a_doc = common::create_doc(&app, &w1, &ws, "열린 방 문서").await;
    let a_id = a_doc["id"].as_str().unwrap().to_owned();
    let b_doc = common::create_doc(&app, &w1, &ws, "닫힌 방 문서").await;
    let b_id = b_doc["id"].as_str().unwrap().to_owned();
    assert_eq!(set(true).await, StatusCode::NO_CONTENT);

    // 끄는 중(막음이 걸린 동안)에는 방에 새로 들어가지 못하고 다시 붙으라는 4503을 받는다.
    let mut a = connect(addr, Some(&w1), &a_id, true).await.unwrap();
    {
        let _closing = state.collab.begin_closing(ws_id);
        assert_eq!(refused_code(addr, &w1, &a_id).await, Some(4503));
    }

    // 문서 B: w1이 고친 뒤 독자가 되어 나가면 방은 리비전을 남기지 못하고(쓸 수 있는 사람이 없음) 실시간 상태만 남는다.
    let mut b = connect(addr, Some(&w1), &b_id, true).await.unwrap();
    b.paragraph("닫힌 방에 남은 글").await;
    // 문서 A: w1이 고친 뒤 독자가 되면 그 연결은 닫히고(4403), 독자 연결이 방을 열어 둔다(쓸 수 있는 사람이 없는 열린 방).
    let mut watcher = connect(addr, Some(&r1), &a_id, true).await.unwrap();
    a.paragraph("열린 방의 글").await;
    assert!(watcher.until(|c| c.text().contains("열린 방의 글")).await);
    role("reader").await;
    assert_eq!(a.closed().await, Some(4403));
    assert_eq!(b.closed().await, Some(4403));
    tokio::time::sleep(Duration::from_millis(800)).await;
    let b_before = send(
        &app,
        Method::GET,
        &format!("/api/documents/{b_id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert!(
        !b_before["body"].to_string().contains("닫힌 방에 남은 글"),
        "{b_before:?}"
    );

    // 같은 워크스페이스를 끄는 요청 둘이 겹쳐도 둘 다 성공하고 두 문서의 편집이 리비전으로 남는다(관리자가 작성자).
    let (first, second) = tokio::join!(set(false), set(false));
    assert_eq!(first, StatusCode::NO_CONTENT);
    assert_eq!(second, StatusCode::NO_CONTENT);
    assert_eq!(watcher.closed().await, Some(4410));
    for (id, text) in [(&a_id, "열린 방의 글"), (&b_id, "닫힌 방에 남은 글")] {
        let doc = send(
            &app,
            Method::GET,
            &format!("/api/documents/{id}"),
            Some(&admin),
            None,
        )
        .await
        .json;
        assert_eq!(doc["realtime"], false);
        assert!(doc["body"].to_string().contains(text), "{doc:?}");
        assert_eq!(last_author(id.clone()).await, admin.id.0);
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.document_collab_states WHERE pending AND document_id IN ($1::uuid, $2::uuid)",
    )
    .bind(&a_id)
    .bind(&b_id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(pending, 0);

    // 끈 뒤 저장 API는 그 리비전을 바탕으로만 저장된다(예전 리비전을 바탕으로 하면 충돌, 실시간 편집을 덮지 않음).
    role("writer").await;
    let doc_uri = format!("/api/documents/{a_id}");
    let after = send(&app, Method::GET, &doc_uri, Some(&w1), None)
        .await
        .json;
    let stale = send(
        &app,
        Method::PUT,
        &doc_uri,
        Some(&w1),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": 1, "title": "열린 방 문서",
            "body": common::doc_body("옛 바탕") }),
        ),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{:?}", stale.json);
    let fresh = send(
        &app,
        Method::PUT,
        &doc_uri,
        Some(&w1),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": after["revision"],
            "title": "열린 방 문서", "body": common::doc_body("끈 뒤 저장") }),
        ),
    )
    .await;
    assert_eq!(fresh.status, StatusCode::OK, "{:?}", fresh.json);
    owner.close().await;
}

/// 감사 반영: 다시 붙은 연결의 동기화 답(이미 반영된 지움 기록)은 편집이 아니다. 편집으로 치면 다시 붙을 때마다
/// 조용한 시간이 처음부터 시작돼 리비전이 남지 않는다. 혼자 있는 연결도 자기 참여자 표시를 돌려받아(연결 유지)
/// 30초마다 다시 붙지 않는다.
#[tokio::test]
async fn reconnecting_without_changes_is_not_an_edit() {
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "다시 붙기").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let doc = common::create_doc(&app, &writer, &ws, "지운 흔적").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let doc_uri = format!("/api/documents/{id}");
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    // 쓰고 지운 뒤 나가면 리비전이 남는다(지움 기록이 생긴다).
    let mut a = connect(addr, Some(&writer), &id, true).await.unwrap();
    a.paragraph("남길 글").await;
    a.paragraph("지울 글").await;
    a.edit(|txn, fragment| {
        let last = fragment.len(txn) - 1;
        fragment.remove_range(txn, last, 1);
    })
    .await;
    let local = a.doc.clone();
    a.close().await;
    let mut saved = None;
    for _ in 0..40 {
        let d = send(&app, Method::GET, &doc_uri, Some(&writer), None)
            .await
            .json;
        if d["body"].to_string().contains("남길 글") {
            saved = Some(d);
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let saved = saved.expect("live document saved as a revision");
    assert!(!saved["body"].to_string().contains("지울 글"), "{saved:?}");

    // 같은 내용으로 다시 붙어 아무것도 고치지 않는다. 저장 주기(3초)가 두 번 지나도 남길 편집이 없다.
    let mut again = connect_with(addr, Some(&writer), &id, true, local)
        .await
        .unwrap();
    again
        .announce(4242, 1, r#"{"user":{"name":"writer"}}"#)
        .await;
    assert!(
        again.until(|c| c.presence.contains_key(&4242)).await,
        "the sender gets its own presence back"
    );
    tokio::time::sleep(Duration::from_secs(7)).await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let pending: Option<bool> = sqlx::query_scalar(
        "SELECT pending FROM fvoci.document_collab_states WHERE document_id = $1::uuid",
    )
    .bind(&id)
    .fetch_optional(&owner)
    .await
    .unwrap();
    assert_ne!(pending, Some(true), "a reconnect counted as an edit");
    again.close().await;
    let after = send(&app, Method::GET, &doc_uri, Some(&writer), None)
        .await
        .json;
    assert_eq!(after["revision"], saved["revision"]);
    owner.close().await;
}

/// 작은 파일을 한 번에 올린다(새 파일 또는 `file_id`의 새 버전). 돌려준 완료 응답.
async fn upload_small(
    app: &Router,
    user: &TestUser,
    mut body: serde_json::Value,
    data: &[u8],
) -> serde_json::Value {
    use sha2::Digest;
    body["request_id"] = json!(new_request_id());
    body["size"] = json!(data.len());
    body["content_type"] = json!("text/plain");
    let started = send(app, Method::POST, "/api/uploads", Some(user), Some(body)).await;
    assert_eq!(started.status, StatusCode::CREATED, "{:?}", started.json);
    let id = started.json["id"].as_str().unwrap().to_owned();
    let hash = hex::encode(sha2::Sha256::digest(data));
    let (status, _, _) = common::send_raw(
        app,
        Method::PUT,
        &format!("/api/uploads/{id}/parts/1"),
        Some(user),
        &[
            ("x-content-sha256", &hash),
            ("content-type", "application/octet-stream"),
        ],
        data.to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let done = send(
        app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    done.json
}

/// 제품 로직 점검(80절): 아직 리비전이 되지 않은 실시간 본문이 가리키는 파일 버전은 지울 수 없다. 전에는 리비전·기록의
/// 참조만 보아 지울 수 있었고, 그 본문이 리비전이 되면 그 버전 주소가 404였다.
#[tokio::test]
async fn live_body_pins_the_file_versions_it_points_to() {
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "실시간 첨부").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let doc = common::create_doc(&app, &writer, &ws, "그림 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let target = json!({ "target": { "kind": "document", "id": id }, "name": "그림.txt" });
    let first = upload_small(&app, &writer, target, b"v1").await;
    let file_id = first["file_id"].as_str().unwrap().to_owned();
    upload_small(&app, &writer, json!({ "file_id": file_id }), b"v2").await;
    // 나중에 1판으로 되돌릴 수 있게 리비전 2를 만든다.
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{id}"),
        Some(&writer),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": 1, "title": "그림 문서",
                     "body": common::doc_body("둘째 판") }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    // 실시간 본문에만 1판 주소를 넣고(리비전은 아직 없음), 상태 저장(3초)을 기다린다.
    let mut a = connect(addr, Some(&writer), &id, true).await.unwrap();
    let url = format!("/api/files/{file_id}/content?version=1&inline=true");
    a.edit(|txn, fragment| {
        let p = fragment.push_back(txn, XmlElementPrelim::empty("paragraph"));
        p.push_back(txn, XmlTextPrelim::new(url.as_str()));
    })
    .await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    let revisions = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&writer),
        None,
    )
    .await
    .json["revision"]
        .clone();
    assert_eq!(revisions, 2, "no live revision yet");

    let delete_v1 = || {
        let app = app.clone();
        let writer = writer.clone();
        let file_id = file_id.clone();
        async move {
            send(
                &app,
                Method::DELETE,
                &format!("/api/files/{file_id}/versions/1"),
                Some(&writer),
                None,
            )
            .await
        }
    };
    let delete = delete_v1().await;
    assert_eq!(delete.status, StatusCode::CONFLICT, "{:?}", delete.json);

    // 리비전 1로 되돌리기 전에 방의 실시간 본문이 리비전 3으로 남는다(검토 6번). 기준 2 바로 다음이 그것뿐이라 복원은
    // 되고, 리비전 3이 1판을 가리키므로 되돌린 뒤에도 1판은 이력이 붙잡는다(리비전 참조 규칙, 0046).
    let restore = |base: i64| {
        let app = app.clone();
        let writer = writer.clone();
        let uri = format!("/api/documents/{id}/revisions/1/restore");
        async move {
            send(
                &app,
                Method::POST,
                &uri,
                Some(&writer),
                Some(json!({ "request_id": new_request_id(), "base_revision": base })),
            )
            .await
        }
    };
    let restored = restore(2).await;
    assert!(restored.status.is_success(), "{:?}", restored.json);
    let _ = a.closed().await;
    let delete = delete_v1().await;
    assert_eq!(delete.status, StatusCode::CONFLICT, "{:?}", delete.json);
}

/// 제품 로직 점검(80절, 0061): 휴지통에 넣은 문서의 아직 리비전이 되지 않은 실시간 본문도 파일 버전을 붙잡는다.
/// 휴지통에 넣은 직후 방이 닫히며 하는 마지막 저장이 거절될 때 그 참조를 지우지 않는다(되살리면 이미지가 보여야 한다).
#[tokio::test]
async fn trashed_document_keeps_live_file_pins() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "1")])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "휴지통 실시간 첨부").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let doc = common::create_doc(&app, &writer, &ws, "버릴 그림 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let target = json!({ "target": { "kind": "document", "id": id }, "name": "그림.txt" });
    let first = upload_small(&app, &writer, target, b"v1").await;
    let file_id = first["file_id"].as_str().unwrap().to_owned();
    upload_small(&app, &writer, json!({ "file_id": file_id }), b"v2").await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    let mut a = connect(addr, Some(&writer), &id, true).await.unwrap();
    let url = format!("/api/files/{file_id}/content?version=1&inline=true");
    a.edit(|txn, fragment| {
        let p = fragment.push_back(txn, XmlElementPrelim::empty("paragraph"));
        p.push_back(txn, XmlTextPrelim::new(url.as_str()));
    })
    .await;
    tokio::time::sleep(Duration::from_millis(3500)).await;
    // 한 번 더 고친 직후 휴지통에 넣는다. 방은 다음 확인에서 닫히며 마지막 상태를 저장하려다 거절된다.
    a.paragraph("버리기 직전").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{id}"),
        Some(&writer),
        None,
    )
    .await;
    assert!(trashed.status.is_success(), "{:?}", trashed.json);
    let _ = a.closed().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 휴지통 문서의 첨부는 API로 지울 수 없다(404). 남는 길은 주기 작업(이전 버전 정리·휴지통 비우기)이며, 둘 다 이
    // 참조 판단을 쓴다.
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let pinned: bool = sqlx::query_scalar(
        "SELECT fvoci.app_version_referenced(v.file_id, v.version, v.id)
         FROM fvoci.file_versions v WHERE v.file_id = $1::uuid AND v.version = 1",
    )
    .bind(&file_id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert!(
        pinned,
        "the trashed document's live body still points at version 1"
    );
    owner.close().await;
}

/// 사용자 지시(80절 마무리): 실시간 본문에 이미지 주소를 넣은 직후, 주기적 상태 저장(3초) 전에 그 버전을 지우는
/// 경합. 4초를 기다린 뒤 지우는 시험만으로는 이 구간을 보지 못한다.
#[tokio::test]
async fn deleting_a_version_right_after_a_live_reference_is_refused() {
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "실시간 첨부 경합").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let doc = common::create_doc(&app, &writer, &ws, "경합 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let target = json!({ "target": { "kind": "document", "id": id }, "name": "그림.txt" });
    let first = upload_small(&app, &writer, target, b"v1").await;
    let file_id = first["file_id"].as_str().unwrap().to_owned();
    upload_small(&app, &writer, json!({ "file_id": file_id }), b"v2").await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    let mut a = connect(addr, Some(&writer), &id, true).await.unwrap();
    let url = format!("/api/files/{file_id}/content?version=1&inline=true");
    a.edit(|txn, fragment| {
        let p = fragment.push_back(txn, XmlElementPrelim::empty("paragraph"));
        p.push_back(txn, XmlTextPrelim::new(url.as_str()));
    })
    .await;
    // 방이 변경을 받을 만큼만(모아 넣기 0.1초) 기다린다. 상태 저장(3초)은 아직이다.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let delete = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{file_id}/versions/1"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(delete.status, StatusCode::CONFLICT, "{:?}", delete.json);
    a.close().await;
}

/// 마무리 ①: 방은 상태를 DB에 저장한 뒤에만 저장 확인(스냅숏)을 보내고, 그 스냅숏은 저장된 편집을 담는다. 이미 저장된
/// 방에 새로 붙으면 처음 메시지에 지난 저장 확인이 온다. 클라이언트는 이것으로만 복구용 초안을 지운다.
#[tokio::test]
async fn rooms_acknowledge_persisted_state_with_a_snapshot() {
    use yrs::Snapshot;
    use yrs::updates::decoder::Decode as _;

    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "저장 확인").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let doc = common::create_doc(&app, &writer, &ws, "확인 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    let mut a = connect(addr, Some(&writer), &id, true).await.unwrap();
    let before = a.persisted.len();
    a.paragraph("저장될 편집").await;
    let mine = a.doc.client_id();
    let covered = |c: &Client| {
        c.persisted[before..].iter().any(|bytes| {
            let snapshot = Snapshot::decode_v1(bytes).unwrap();
            snapshot.state_map.get(&mine) >= c.doc.transact().state_vector().get(&mine)
        })
    };
    // 상태 저장(3초)이 지나면 내 편집을 담은 저장 확인이 온다. 그 전에는 오지 않는다.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = a.next(Duration::from_millis(100)).await;
    assert!(!covered(&a), "acknowledged before the state was stored");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    while !covered(&a) && tokio::time::Instant::now() < deadline {
        let _ = a.next(Duration::from_millis(200)).await;
    }
    assert!(covered(&a), "no acknowledgement covering the stored edit");
    let stored: bool = {
        let owner_url =
            std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
        let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
        let exists = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM fvoci.document_collab_states WHERE document_id = $1::uuid)",
        )
        .bind(&id)
        .fetch_one(&owner)
        .await
        .unwrap();
        owner.close().await;
        exists
    };
    assert!(stored, "acknowledged state must be in the database");

    // 같은 방에 새로 붙으면 처음 메시지에 지난 저장 확인이 온다.
    let b = connect(addr, Some(&writer), &id, true).await.unwrap();
    assert!(
        !b.persisted.is_empty(),
        "joining a stored room carries its acknowledgement"
    );
    b.close().await;
    a.close().await;
}

/// 마무리 ③: 실시간 상태의 파일 참조 저장도 리비전 저장처럼 그 버전 행을 공유 잠금한다. 저장 트랜잭션이 열려 있는
/// 동안 자동 정리(이전 버전 정리)는 그 버전을 건너뛰고, 저장이 끝난 뒤에는 참조를 보고 남긴다. 두 트랜잭션으로 재현.
#[tokio::test]
async fn live_ref_store_and_old_version_cleanup_do_not_race() {
    let _serial = GLOBAL_PURGE.lock().await;
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "정리 경합").await;
    let doc = common::create_doc(&app, &admin, &ws, "정리 경합 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let target = json!({ "target": { "kind": "document", "id": id }, "name": "그림.txt" });
    let first = upload_small(&app, &admin, target, b"v1").await;
    let file_id = first["file_id"].as_str().unwrap().to_owned();
    upload_small(&app, &admin, json!({ "file_id": file_id }), b"v2").await;

    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    // 1판을 정리 대상으로 만든다: 보존 1일, 다음 판이 이틀 전에 올라온 것으로.
    sqlx::query("UPDATE fvoci.workspaces SET version_retention_days = 1 WHERE id = $1::uuid")
        .bind(&ws)
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE fvoci.file_versions SET created_at = now() - interval '2 days' WHERE file_id = $1::uuid AND version = 2",
    )
    .bind(&file_id)
    .execute(&owner)
    .await
    .unwrap();
    let v1_exists = || {
        let owner = owner.clone();
        let file_id = file_id.clone();
        async move {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM fvoci.file_versions WHERE file_id = $1::uuid AND version = 1)",
            )
            .bind(file_id)
            .fetch_one(&owner)
            .await
            .unwrap()
        }
    };

    // T1: 실시간 상태를 저장하며 1판을 가리킨다(아직 끝나지 않음).
    let body = json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text",
        "text": format!("/api/files/{file_id}/content?version=1&inline=true") }] }] });
    let mut t1 = owner.begin().await.unwrap();
    let stored: bool = sqlx::query_scalar(
        "SELECT fvoci.app_system_collab_store($1::uuid, 1, '\\x00'::bytea, true, $2)",
    )
    .bind(&id)
    .bind(&body)
    .fetch_one(&mut *t1)
    .await
    .unwrap();
    assert!(stored);
    // T2: 그동안 자동 정리가 돈다.
    let purged: i32 = sqlx::query_scalar("SELECT fvoci.app_system_purge_old_versions()")
        .fetch_one(&owner)
        .await
        .unwrap();
    t1.commit().await.unwrap();
    assert_eq!(
        purged, 0,
        "cleanup removed a version a live state was pinning"
    );
    assert!(v1_exists().await);
    // 저장이 끝난 뒤의 정리는 참조를 보고 남긴다.
    let purged: i32 = sqlx::query_scalar("SELECT fvoci.app_system_purge_old_versions()")
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_eq!(purged, 0);
    assert!(v1_exists().await);
    owner.close().await;
}

/// 감사 2번: 계속 편집하는 문서는 쉬는 시간(60초)이 없어 리비전이 남지 않는다. 워크스페이스 반출은 그동안의 실시간
/// 편집을 담아야 한다(반출 직전에 열린 방을 리비전으로 남긴다).
#[tokio::test]
async fn workspace_export_includes_live_edits() {
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "실시간 반출").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let doc = common::create_doc(&app, &writer, &ws, "반출 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    let mut a = connect(addr, Some(&writer), &id, true).await.unwrap();
    a.paragraph("방금 쓴 실시간 문단").await;
    // 방이 변경을 받을 만큼만(모아 넣기 0.1초) 기다린다. 쉬는 시간 리비전(60초)은 아직이다.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (status, _, bytes) = common::send_raw(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/export"),
        Some(&admin),
        &[],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut json = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("export.json").unwrap(), &mut json).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json).unwrap();
    let body = json["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id.as_str())
        .unwrap()["body"]
        .to_string();
    assert!(body.contains("방금 쓴 실시간 문단"), "{body}");
    a.close().await;
}

/// 검토 6번: 실시간 편집 중 옛 리비전으로 복원해도 아직 리비전으로 남지 않은 실시간 편집이 이력에서 사라지면 안 된다
/// (D-27 내용 손실 방지). 복원 전에 열린 방을 리비전으로 남긴다. 그 사이 기준이 지났으면 복원은 충돌로 알린다.
#[tokio::test]
async fn restoring_during_live_editing_keeps_the_live_edits_in_history() {
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "실시간 복원").await;
    let doc = common::create_doc(&app, &admin, &ws, "복원 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    let mut a = connect(addr, Some(&admin), &id, true).await.unwrap();
    a.paragraph("복원 전에 쓴 실시간 문단").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let current = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let restore = |base: serde_json::Value| {
        let app = app.clone();
        let admin = admin.clone();
        let uri = format!("/api/documents/{id}/revisions/1/restore");
        async move {
            send(
                &app,
                Method::POST,
                &uri,
                Some(&admin),
                Some(json!({ "request_id": new_request_id(), "base_revision": base })),
            )
            .await
        }
    };
    // 화면은 방이 방금 남긴 리비전 번호를 모르므로 보던 리비전을 기준으로 보낸다(검토 2차 1번: 그래도 복원된다).
    let r = restore(current["revision"].clone()).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);

    let list = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}/revisions"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let mut kept = false;
    for rev in list.as_array().unwrap() {
        let n = rev["revision"].as_i64().unwrap();
        let one = send(
            &app,
            Method::GET,
            &format!("/api/documents/{id}/revisions/{n}"),
            Some(&admin),
            None,
        )
        .await
        .json;
        kept |= one["body"].to_string().contains("복원 전에 쓴 실시간 문단");
    }
    assert!(kept, "the live edit vanished from the history: {list}");
    let _ = a.closed().await;
}

/// 검토 7번: 파일 버전을 지울 수 없는 사람(읽기)의 요청은 워크스페이스의 열린 방을 저장시키지 않는다. 저장은 지울 수
/// 있는 사람의 요청에서만 미리 한다. 주기적 저장(3초) 전에 확인한다.
#[tokio::test]
async fn a_reader_cannot_make_rooms_persist_through_version_delete() {
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "읽기 저장 유도").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let doc = common::create_doc(&app, &admin, &ws, "열린 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let target = json!({ "target": { "kind": "document", "id": id }, "name": "첨부.txt" });
    let first = upload_small(&app, &admin, target, b"v1").await;
    let file_id = first["file_id"].as_str().unwrap().to_owned();
    upload_small(&app, &admin, json!({ "file_id": file_id }), b"v2").await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let stored = || {
        let owner = owner.clone();
        let id = id.clone();
        async move {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM fvoci.document_collab_states WHERE document_id = $1::uuid AND pending)",
            )
            .bind(id)
            .fetch_one(&owner)
            .await
            .unwrap()
        }
    };

    let mut a = connect(addr, Some(&admin), &id, true).await.unwrap();
    a.paragraph("아직 저장하지 않은 편집").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !stored().await,
        "the periodic save already ran; the check below would mean nothing"
    );
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{file_id}/versions/1"),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{:?}", r.json);
    assert!(!stored().await, "a reader's request made the room save");
    a.close().await;
    owner.close().await;
}

/// 검토 8번: 열린 방의 문서를 다른 워크스페이스로 옮기면 방도 새 워크스페이스에 속한다. 새 워크스페이스에서 실시간을
/// 끄면 그 방도 리비전을 남기고 닫혀야 한다(전에는 옛 워크스페이스로 남아 권한 재확인 때까지 열려 있었다).
#[tokio::test]
async fn a_moved_room_follows_its_document_to_the_new_workspace() {
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let from = create_workspace(&app, &admin, "옮기기 전").await;
    let to = create_workspace(&app, &admin, "옮긴 뒤").await;
    for ws in [&from, &to] {
        let on = send(
            &app,
            Method::PUT,
            &format!("/api/workspaces/{ws}/realtime"),
            Some(&admin),
            Some(json!({ "enabled": true })),
        )
        .await;
        assert_eq!(on.status, StatusCode::NO_CONTENT);
    }
    let doc = common::create_doc(&app, &admin, &from, "옮길 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let mut a = connect(addr, Some(&admin), &id, true).await.unwrap();
    a.paragraph("옮기기 전 실시간 문단").await;

    let target = json!({ "target_workspace_id": to, "target_item_id": null });
    let preview = send(
        &app,
        Method::POST,
        &format!("/api/documents/{id}/move/preview"),
        Some(&admin),
        Some(target),
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    let moved = send(
        &app,
        Method::POST,
        &format!("/api/documents/{id}/move"),
        Some(&admin),
        Some(json!({ "target_workspace_id": to, "target_item_id": null,
                     "acknowledged": { "gains": preview.json["gains"], "loses": preview.json["loses"] } })),
    )
    .await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);

    let off = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{to}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": false })),
    )
    .await;
    assert_eq!(off.status, StatusCode::NO_CONTENT, "{:?}", off.json);
    assert_eq!(a.closed().await, Some(4410));
    let saved = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert!(
        saved["body"].to_string().contains("옮기기 전 실시간 문단"),
        "{saved}"
    );
}

/// 이 문서의 파일 참조 행을 넣은 직후 1초 멈추는 시험용 트리거(그 문서만). 참조를 넣은 뒤 잠그기 전의 틈을 넓힌다.
async fn slow_ref_inserts(owner: &sqlx::PgPool, table: &str, doc: &str) -> String {
    let name = format!("test_slow_{}", uuid::Uuid::now_v7().simple());
    sqlx::query(&format!(
        "CREATE FUNCTION fvoci.{name}() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN IF NEW.document_id = '{doc}'::uuid THEN PERFORM pg_sleep(1); END IF; RETURN NULL; END $$"
    ))
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(&format!(
        "CREATE TRIGGER {name} AFTER INSERT ON fvoci.{table} FOR EACH ROW EXECUTE FUNCTION fvoci.{name}()"
    ))
    .execute(owner)
    .await
    .unwrap();
    format!("DROP TRIGGER {name} ON fvoci.{table}; DROP FUNCTION fvoci.{name}()")
}

/// 검토 9번: 참조를 적는 쪽(실시간 상태 저장·리비전 저장)은 가리키는 버전을 참조를 넣기 전에 잠가야 한다. 넣은 뒤에
/// 잠그면 그 사이 자동 정리가 먼저 잠그고(아직 보이지 않는 참조) 그 버전을 지울 수 있다.
#[tokio::test]
async fn file_refs_lock_versions_before_they_are_written() {
    let _serial = GLOBAL_PURGE.lock().await;
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();

    for path in ["live", "revision"] {
        let ws = create_workspace(&app, &admin, &format!("잠금 순서 {path}")).await;
        let doc = common::create_doc(&app, &admin, &ws, "잠금 순서 문서").await;
        let id = doc["id"].as_str().unwrap().to_owned();
        let target = json!({ "target": { "kind": "document", "id": id }, "name": "그림.txt" });
        let first = upload_small(&app, &admin, target, b"v1").await;
        let file_id = first["file_id"].as_str().unwrap().to_owned();
        upload_small(&app, &admin, json!({ "file_id": file_id }), b"v2").await;
        // 1판을 정리 대상으로 만든다: 보존 1일, 다음 판이 이틀 전에 올라온 것으로.
        sqlx::query("UPDATE fvoci.workspaces SET version_retention_days = 1 WHERE id = $1::uuid")
            .bind(&ws)
            .execute(&owner)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE fvoci.file_versions SET created_at = now() - interval '2 days' WHERE file_id = $1::uuid AND version = 2",
        )
        .bind(&file_id)
        .execute(&owner)
        .await
        .unwrap();
        let body = json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text",
            "text": format!("/api/files/{file_id}/content?version=1&inline=true") }] }] });

        // 쓰는 쪽을 띄우기 전에 그 참조 표에 멈춤 트리거를 단다.
        let table = if path == "live" {
            "document_collab_file_refs"
        } else {
            "document_file_refs"
        };
        let drop = slow_ref_inserts(&owner, table, &id).await;
        let writer = if path == "live" {
            let owner = owner.clone();
            let id = id.clone();
            let body = body.clone();
            tokio::spawn(async move {
                let stored: bool = sqlx::query_scalar(
                    "SELECT fvoci.app_system_collab_store($1::uuid, 1, '\\x00'::bytea, true, $2)",
                )
                .bind(&id)
                .bind(&body)
                .fetch_one(&owner)
                .await
                .unwrap();
                assert!(stored);
            })
        } else {
            let app = app.clone();
            let admin = admin.clone();
            let id = id.clone();
            let base = doc["revision"].clone();
            let body = body.clone();
            tokio::spawn(async move {
                let saved = send(
                    &app,
                    Method::PUT,
                    &format!("/api/documents/{id}"),
                    Some(&admin),
                    Some(
                        json!({ "request_id": new_request_id(), "base_revision": base,
                                 "title": "잠금 순서 문서", "body": body }),
                    ),
                )
                .await;
                assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
            })
        };
        drop_writer_race(writer, &owner).await;
        sqlx::raw_sql(&drop).execute(&owner).await.unwrap();
        let v1: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM fvoci.file_versions WHERE file_id = $1::uuid AND version = 1)",
        )
        .bind(&file_id)
        .fetch_one(&owner)
        .await
        .unwrap();
        assert!(
            v1,
            "{path}: cleanup removed a version a reference was being written for"
        );
    }
    owner.close().await;
}

/// 쓰는 쪽이 참조를 넣고 멈춘 사이 자동 정리를 돌리고, 쓰는 쪽이 끝나기를 기다린다. 정해 둔 시간 대신 쓰는 쪽이
/// 실제로 멈춤 트리거 안에 있을 때까지 기다린다(느린 컴퓨터에서 정리가 먼저 돌아 잘못 실패하지 않게).
async fn drop_writer_race(writer: tokio::task::JoinHandle<()>, owner: &sqlx::PgPool) {
    // 다른 역할(앱)의 대기 상태는 관리자만 본다.
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let watcher = sqlx::PgPool::connect(&admin_url).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let paused: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE wait_event = 'PgSleep')",
        )
        .fetch_one(&watcher)
        .await
        .unwrap();
        if paused {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the writer never reached the paused ref insert"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    watcher.close().await;
    let _purged: i32 = sqlx::query_scalar("SELECT fvoci.app_system_purge_old_versions()")
        .fetch_one(owner)
        .await
        .unwrap();
    writer.await.unwrap();
}

/// 검토(3차) 2번: 복원 기준을 방이 방금 남긴 리비전으로 옮기는 것은 그 실시간 내용을 보던 사람(방에 쓰기 연결이
/// 있는 사람)의 복원뿐이다. 방에 없는 사람(다른 사람의 실시간 편집을 못 본 사람)의 복원은 기존처럼 충돌로 알린다.
#[tokio::test]
async fn a_restore_from_outside_the_room_still_conflicts_with_live_edits() {
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "방 밖 복원").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let doc = common::create_doc(&app, &admin, &ws, "방 밖 복원 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let mut b = connect(addr, Some(&writer), &id, true).await.unwrap();
    b.paragraph("다른 사람의 실시간 문단").await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 관리자는 방에 없다(그 문단을 보지 못했다). 보던 리비전 1을 기준으로 되돌린다.
    let r = send(
        &app,
        Method::POST,
        &format!("/api/documents/{id}/revisions/1/restore"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "base_revision": doc["revision"] })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
    assert!(
        r.json["current"]["body"]
            .to_string()
            .contains("다른 사람의 실시간 문단")
    );
    b.close().await;
}

/// 검토(3차) 1번: 방은 이번에 실제로 남긴 리비전만 알린다. 실시간 본문이 DB 본문과 같아 쓰지 않았으면(그 사이 제목만
/// 바뀐 리비전이 있어도) 리비전을 알리지 않는다(알리면 복원이 그 제목 변경을 충돌 없이 덮는다).
#[tokio::test]
async fn a_room_reports_only_a_revision_it_wrote() {
    let state = fvoci_server::app::build_state(config_with(&[])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "쓰지 않은 리비전").await;
    let doc = common::create_doc(&app, &admin, &ws, "쓰지 않은 리비전 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let mut a = connect(addr, Some(&admin), &id, true).await.unwrap();
    // 고쳤다가 되돌려 방은 바뀐 것으로 알지만 본문은 DB와 같다.
    a.paragraph("곧 지울 문단").await;
    a.edit(|txn, fragment| {
        let last = fragment.len(txn) - 1;
        fragment.remove_range(txn, last, 1);
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // 그 사이 제목만 바뀐 리비전 2가 생긴다(실시간 중 저장은 제목만).
    let current = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{id}"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": current["revision"],
                     "title": "바뀐 제목", "body": current["body"] }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    assert_eq!(saved.json["revision"], 2);

    let doc_id = fvoci_server::ids::DocumentId(id.parse().unwrap());
    assert_eq!(state.collab.snapshot_document(doc_id, admin.id).await, None);
    a.close().await;
}
