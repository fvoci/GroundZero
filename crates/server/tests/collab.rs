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
    /// 받은 참여자 표시(client id → JSON 글).
    presence: std::collections::HashMap<u64, String>,
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
    a.close().await;
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
    let mut request = format!("ws://{addr}/api/documents/{doc}/collab")
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
    // 문서 A: w1이 고친 채로 연결이 열려 있다.
    a.paragraph("열린 방의 글").await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    role("reader").await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    b.close().await;
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
    assert_eq!(a.closed().await, Some(4410));
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
