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

/// 연결 주소. 웹이 하듯 기대 사용자(`expected_user`)를 연결하는 사용자로 붙인다. 값 자체(없음·다름·형식)는
/// `the_live_connection_carries_the_expected_user`가 직접 시험한다. `query`는 `?write=1`처럼 `?`로 시작하거나 비었다.
fn collab_url(addr: SocketAddr, doc: &str, user: Option<&TestUser>, query: &str) -> String {
    let base = format!("ws://{addr}/api/documents/{doc}/collab{query}");
    match user {
        Some(u) if query.is_empty() => format!("{base}?expected_user={}", u.id.0),
        Some(u) => format!("{base}&expected_user={}", u.id.0),
        None => base,
    }
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
    let mut request = collab_url(addr, doc, user, "")
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
    /// 내가 보낸 핑의 답(`Client::read_by_server`).
    Pong,
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
            Some(Ok(WsMessage::Pong(_))) => Event::Pong,
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
    let mut request = collab_url(addr, doc, Some(user), query)
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

/// 기대 사용자를 그대로(자동으로 붙이지 않고) 정해 연결한다. 업그레이드가 거절되면 HTTP 상태.
async fn connect_raw(
    addr: SocketAddr,
    user: &TestUser,
    doc: &str,
    query: &str,
) -> Result<Client, StatusCode> {
    let mut request = format!("ws://{addr}/api/documents/{doc}/collab{query}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("cookie", HeaderValue::from_str(&user.cookie).unwrap());
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static(common::PUBLIC_URL));
    match tokio_tungstenite::connect_async(request).await {
        Ok((ws, _)) => Ok(Client {
            ws,
            doc: Doc::new(),
            presence: std::collections::HashMap::new(),
            persisted: Vec::new(),
        }),
        Err(WsError::Http(response)) => {
            Err(StatusCode::from_u16(response.status().as_u16()).unwrap())
        }
        Err(err) => panic!("websocket error {err}"),
    }
}

/// 실시간 연결의 기대 사용자(#60, 사용자 결정 2026-10-22): 다른 탭에서 계정을 바꾼 뒤 y-websocket이 새 쿠키로 다시
/// 붙어도, 앞 계정의 화면이 연 연결이면 방에 들어가기 전에 4412로 닫혀 그 화면의 미동기화 편집이 새 계정 이름으로
/// 합쳐지지 않는다. 값이 없거나 형식이 틀리면 업그레이드 전에 400. 값은 비교에만 쓰고 권한을 주지 않는다.
#[tokio::test]
async fn the_live_connection_carries_the_expected_user() {
    let state = common::state();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "실시간 기대 사용자").await;
    let (w1, _) = add_member(&app, &state, &admin, &ws, "w1", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let outsider = login(&state, "outsider").await;
    let doc = common::create_doc(&app, &w1, &ws, "기대 사용자 문서").await;
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

    // 일치하면 평소대로 붙고 편집이 오간다.
    let mut watcher = connect(addr, Some(&admin), &id, true).await.unwrap();
    let mut mine = connect(addr, Some(&w1), &id, true).await.unwrap();
    mine.paragraph("w1의 편집").await;
    assert!(watcher.until(|c| c.text().contains("w1의 편집")).await);

    // w1의 세션(새 쿠키)인데 연결은 다른 사용자(admin)의 화면이 열었다: 방에 들어가기 전에 4412. 그사이 보낸 편집은
    // 방에 들어가지 않는다.
    let mut other = connect_raw(
        addr,
        &w1,
        &id,
        &format!("?write=1&expected_user={}", admin.id.0),
    )
    .await
    .unwrap();
    let fragment = other.doc.get_or_insert_xml_fragment(FRAGMENT);
    {
        let mut txn = other.doc.transact_mut();
        let p = fragment.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
        p.push_back(&mut txn, XmlTextPrelim::new("다른 계정의 편집"));
    }
    let update = other
        .doc
        .transact()
        .encode_state_as_update_v1(&yrs::StateVector::default());
    let _sent = other
        .ws
        .send(WsMessage::Binary(
            Message::Sync(SyncMessage::Update(update))
                .encode_v1()
                .into(),
        ))
        .await;
    assert_eq!(other.closed().await, Some(4412));
    // 그 뒤의 편집이 도착했을 때 거절된 편집은 없다(들어갔다면 먼저 도착했다).
    mine.paragraph("표시").await;
    assert!(watcher.until(|c| c.text().contains("표시")).await);
    assert!(
        !watcher.text().contains("다른 계정의 편집"),
        "{}",
        watcher.text()
    );

    // 값이 없거나(옛 연결) 형식이 틀리면 업그레이드 전에 400.
    for query in [
        "",
        "?write=1",
        "?expected_user=",
        "?write=1&expected_user=not-a-uuid",
    ] {
        assert_eq!(
            connect_raw(addr, &w1, &id, query).await.err(),
            Some(StatusCode::BAD_REQUEST),
            "{query}"
        );
    }

    // 쓰기 권한자의 id를 넣어도 권한이 생기지 않는다: 다르면 4412로 닫힐 뿐이고, 자기 id면 평소대로(독자의 편집기
    // 연결은 4403, 멤버가 아니면 404).
    for who in [&reader, &outsider] {
        let mut c = connect_raw(
            addr,
            who,
            &id,
            &format!("?write=1&expected_user={}", w1.id.0),
        )
        .await
        .unwrap();
        assert_eq!(c.closed().await, Some(4412));
    }
    assert_eq!(
        refused_code_with(addr, &reader, &id, "?write=1").await,
        Some(4403)
    );
    assert_eq!(
        connect(addr, Some(&outsider), &id, true).await.err(),
        Some(StatusCode::NOT_FOUND)
    );

    // 모두 나가면 남는 리비전에도 거절된 편집은 없다.
    mine.close().await;
    watcher.close().await;
    let mut body = String::new();
    for _ in 0..40 {
        body = send(
            &app,
            Method::GET,
            &format!("/api/documents/{id}"),
            Some(&admin),
            None,
        )
        .await
        .json["body"]
            .to_string();
        if body.contains("표시") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(
        body.contains("w1의 편집") && body.contains("표시"),
        "{body}"
    );
    assert!(!body.contains("다른 계정의 편집"), "{body}");
}

/// 실시간 끄기 회귀(사용자 지시 2026-10-17, 리뷰 반영): 끄는 동안 새 방이 열리지 않고(4503), 열린 방과 방 없이 남은
/// 실시간 상태의 편집이 모두 리비전으로 남은 뒤에만 꺼진다. 편집한 사람이 더는 쓸 수 없으면 끄는 관리자가 작성자가
/// 되고 실제 편집자를 리비전에 따로 적는다(D-95). 방 없이 남은 상태도 그 상태와 함께 저장한 편집자가 아직 쓸 수 있으면 그
/// 사람이 작성자이므로, 여기서 관리자가 작성자인 것은 w1이 그때 독자이기 때문이다. 같은 워크스페이스를 동시에 끄는
/// 요청이 겹쳐도 편집이 남는다. 끈 뒤 저장 API는 그 리비전 위에서만 성공한다(보통 리비전).
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
        // D-95: 관리자가 대신 남긴 리비전에는 실제로 고친 사람을 따로 적는다(열린 방은 방이 지닌 편집자, 방 없이 남은
        // 상태는 상태와 함께 저장한 편집자).
        let (last, _) = last_revision(&owner, id).await;
        assert_eq!(
            revision_authors(&owner, id, last).await.1,
            Some(vec![w1.id.0]),
            "{id}"
        );
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
    // 저장 API의 리비전은 늘 보통 리비전이다(대신 저장 기록 없음).
    let (last, author) = last_revision(&owner, &a_id).await;
    assert_eq!(author, w1.id.0);
    assert_eq!(revision_authors(&owner, &a_id, last).await.1, None);
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

    // 리비전 1로 되돌리기 전에 방의 실시간 본문이 리비전 3으로 남는다(검토 6번). 그래서 기준 2는 지났고(충돌), 그
    // 리비전이 1판을 가리키므로 되돌린 뒤에도 1판은 이력이 붙잡는다(리비전 참조 규칙, 0046).
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
    let stale = restore(2).await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{:?}", stale.json);
    let restored = restore(stale.json["current"]["revision"].as_i64().unwrap()).await;
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
        "SELECT fvoci.app_system_collab_store($1::uuid, 1, '\\x00'::bytea, true, $2, '{}'::uuid[])",
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
    // 남은 항목 ①: 방이 그 편집을 리비전으로 남기면 화면이 보던 기준은 지난 것이다. 서버는 기준을 올리지 않고 충돌로
    // 알리며 지금 내용(그 편집 포함)을 돌려준다. 사람이 그것을 보고 명시적으로 다시 복원한다.
    let first = restore(current["revision"].clone()).await;
    assert_eq!(first.status, StatusCode::CONFLICT, "{:?}", first.json);
    assert!(
        first.json["current"]["body"]
            .to_string()
            .contains("복원 전에 쓴 실시간 문단")
    );
    let r = restore(first.json["current"]["revision"].clone()).await;
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
/// 끄면 그 방도 리비전을 남기고 닫혀야 한다(전에는 옛 워크스페이스로 남아 권한 재확인 때까지 열려 있었다). #71 뒤로는
/// 옮길 때 방이 편집을 리비전으로 남기고 닫혀(4409) 새 자리에서 다시 연다.
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
    assert_eq!(a.closed().await, Some(4409));

    // 새 자리에서 다시 연 방에서 고친다(다른 연결이 받은 것을 확인한다).
    let mut b = connect(addr, Some(&admin), &id, true).await.unwrap();
    assert!(b.text().contains("옮기기 전 실시간 문단"), "{}", b.text());
    let mut c = connect(addr, Some(&admin), &id, true).await.unwrap();
    b.paragraph("옮긴 뒤 실시간 문단").await;
    assert!(c.until(|c| c.text().contains("옮긴 뒤 실시간 문단")).await);
    let off = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{to}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": false })),
    )
    .await;
    assert_eq!(off.status, StatusCode::NO_CONTENT, "{:?}", off.json);
    assert_eq!(b.closed().await, Some(4410));
    assert_eq!(c.closed().await, Some(4410));
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
    assert!(
        saved["body"].to_string().contains("옮긴 뒤 실시간 문단"),
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
                    "SELECT fvoci.app_system_collab_store($1::uuid, 1, '\\x00'::bytea, true, $2, '{}'::uuid[])",
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

/// 실시간 복원 시험 준비: 관리자와 작가가 있는 실시간 워크스페이스의 문서. (상태, 앱, 주소, 관리자, 작가, 워크스페이스, 문서 id).
async fn live_restore_setup(
    label: &str,
) -> (
    fvoci_server::http::AppState,
    Router,
    SocketAddr,
    TestUser,
    TestUser,
    String,
    String,
) {
    // 권한 재확인(기본 30초)이 시험 중에 돌아 방을 닫지 않게 한다.
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "3600")]))
            .unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, label).await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let doc = common::create_doc(&app, &admin, &ws, label).await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let id = doc["id"].as_str().unwrap().to_owned();
    (state, app, addr, admin, writer, ws, id)
}

async fn restore_rev1(
    app: &Router,
    user: &TestUser,
    id: &str,
    base: &serde_json::Value,
) -> common::Reply {
    send(
        app,
        Method::POST,
        &format!("/api/documents/{id}/revisions/1/restore"),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "base_revision": base })),
    )
    .await
}

/// 관리자 연결로 `pg_stat_activity`를 보아, `holder`(이 시험이 잠근 연결)를 기다리는 `query_part` 질의가 있을 때까지
/// 기다린다(시간을 정해 두지 않음, 다른 시험의 잠금 대기와 섞이지 않음).
async fn until_waiting_on_lock(query_part: &str, holder: i32) {
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let watcher = sqlx::PgPool::connect(&admin_url).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE wait_event_type = 'Lock' AND query LIKE '%' || $1 || '%'
                                AND $2 = ANY (pg_blocking_pids(pid)))",
        )
        .bind(query_part)
        .bind(holder)
        .fetch_one(&watcher)
        .await
        .unwrap();
        if waiting {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "nothing waited on the lock for {query_part}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    watcher.close().await;
}

/// 남은 항목 ①(실패 주입): 복원 전 체크포인트가 실패하면(리비전으로 남길 수 있는 편집자가 없음) 복원하지 않는다. 전에는
/// 체크포인트 결과와 상관없이 복원하고 방을 닫아, 리비전이 되지 않은 실시간 편집이 사라졌다.
#[tokio::test]
async fn a_failed_checkpoint_stops_the_restore() {
    let (_state, app, addr, admin, writer, ws, id) = live_restore_setup("체크포인트 실패").await;
    let mut b = connect(addr, Some(&writer), &id, true).await.unwrap();
    b.paragraph("남겨야 할 실시간 문단").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // 그 편집자를 읽기로 내린다(연결은 다음 권한 확인까지 남는다): 방은 그 편집을 리비전으로 남길 수 없다.
    let demoted = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/members/{}", writer.id.0),
        Some(&admin),
        Some(json!({ "role": "reader" })),
    )
    .await;
    assert!(demoted.status.is_success(), "{:?}", demoted.json);
    let before = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let r = restore_rev1(&app, &admin, &id, &before["revision"]).await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
    assert!(r.json.get("current").is_none(), "{:?}", r.json);
    let after = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(
        after["revision"], before["revision"],
        "the restore went ahead"
    );
    // 편집은 사라지지 않았다: 새로 붙은 연결이 방에서 그 편집을 받는다.
    let c = connect(addr, Some(&admin), &id, true).await.unwrap();
    assert!(c.text().contains("남겨야 할 실시간 문단"), "{}", c.text());
    c.close().await;
    b.close().await;
}

/// 검토(남은 항목 ①): 체크포인트에서 DB 본문이 방이 아는 것과 다르면(다른 길로 바뀜) 방은 편집을 리비전으로 남기지 않고
/// 닫는다. 그때는 보존이 확인되지 않았으므로 복원하지 않는다(전에는 성공으로 보고 복원했다).
#[tokio::test]
async fn a_checkpoint_that_finds_the_body_changed_stops_the_restore() {
    let (_state, app, addr, admin, _writer, _ws, id) =
        live_restore_setup("다른 길로 바뀐 본문").await;
    let mut a = connect(addr, Some(&admin), &id, true).await.unwrap();
    a.paragraph("방에만 있는 문단").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // 실패 주입: 방을 거치지 않고 새 리비전으로 본문을 바꾼다(방이 모르는 변경).
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    sqlx::query(
        "WITH d AS (UPDATE fvoci.documents SET body = $2, revision = revision + 1 WHERE id = $1::uuid
                    RETURNING id, revision, title, schema_version, body)
         INSERT INTO fvoci.document_revisions (document_id, revision, title, schema_version, body, author, request_id)
         SELECT id, revision, title, schema_version, body, $3, $4 FROM d",
    )
    .bind(&id)
    .bind(common::doc_body("밖에서 바뀐 본문"))
    .bind(admin.id.0)
    .bind(uuid::Uuid::now_v7())
    .execute(&owner)
    .await
    .unwrap();
    // 방 밖에서 그 리비전을 본 사람이 복원한다(기준은 맞다).
    let before = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let r = restore_rev1(&app, &admin, &id, &before["revision"]).await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
    assert!(r.json.get("current").is_none(), "{:?}", r.json);
    let restored_from: Option<i32> = sqlx::query_scalar(
        "SELECT restored_from FROM fvoci.document_revisions WHERE document_id = $1::uuid ORDER BY revision DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(
        restored_from, None,
        "the restore went ahead without the live edits saved"
    );
    owner.close().await;
    a.close().await;
}

/// 남은 항목 ①(시간 초과): 체크포인트가 정해진 시간 안에 끝나지 않으면 복원하지 않는다. 문서 행을 다른 트랜잭션이 잠가
/// 체크포인트를 붙잡는다. 전에는 체크포인트를 기다리다 그만두고 그대로 복원했다.
#[tokio::test]
async fn a_checkpoint_that_times_out_stops_the_restore() {
    let (_state, app, addr, admin, _writer, _ws, id) =
        live_restore_setup("체크포인트 시간 초과").await;
    let mut a = connect(addr, Some(&admin), &id, true).await.unwrap();
    a.paragraph("시간 초과 때 남길 문단").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let before = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let mut hold = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT 1 FROM fvoci.documents WHERE id = $1::uuid FOR UPDATE")
        .bind(&id)
        .execute(&mut *hold)
        .await
        .unwrap();
    let request = tokio::spawn({
        let (app, admin, id, base) = (
            app.clone(),
            admin.clone(),
            id.clone(),
            before["revision"].clone(),
        );
        async move { restore_rev1(&app, &admin, &id, &base).await }
    });
    // 답이 오거나(체크포인트 시간 초과) 복원 함수가 잠금을 기다리기 시작하면(고치기 전: 체크포인트를 버리고 복원) 잠금을 푼다.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let watcher = sqlx::PgPool::connect(&admin_url).await.unwrap();
    while !request.is_finished() {
        let restoring: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE wait_event_type = 'Lock'
                                AND query LIKE '%app_restore_document_revision%' AND $1 = ANY (pg_blocking_pids(pid)))",
        )
        .bind(holder)
        .fetch_one(&watcher)
        .await
        .unwrap();
        if restoring {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the restore neither answered nor proceeded"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    hold.rollback().await.unwrap();
    let r = request.await.unwrap();
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
    assert!(r.json.get("current").is_none(), "{:?}", r.json);
    let restored_from: Option<i32> = sqlx::query_scalar(
        "SELECT restored_from FROM fvoci.document_revisions WHERE document_id = $1::uuid ORDER BY revision DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(
        restored_from, None,
        "the restore went ahead after the checkpoint timed out"
    );
    watcher.close().await;
    owner.close().await;
    a.close().await;
}

/// 남은 항목 ①(직렬화): 체크포인트 → 복원 확정 → 방 초기화 사이에 들어온 편집은 방이 받아 저장 확인을 보내지 않는다(그러면
/// 클라이언트가 초안을 지운 뒤 방 초기화로 사라진다). 복원이 끝나면 연결은 4409로 닫혀 클라이언트가 그 편집을 초안으로 남긴다.
/// 문서 행을 잠가 복원 SQL을 붙잡고, 그동안 편집을 보내고 저장을 시킨다.
#[tokio::test]
async fn edits_during_a_restore_are_not_acknowledged_then_dropped() {
    let (state, app, addr, admin, _writer, ws, _wiki) = live_restore_setup("복원 중 편집").await;
    // 항목 문서: 복원은 항목 행을 공유 잠금하고(app_lock_item), 실시간 상태 저장은 항목을 건드리지 않는다. 그래서 항목
    // 행을 잡아 복원만 붙잡을 수 있다.
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": "복원", "key": "RSTR" })),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "복원 항목" })),
    )
    .await
    .json;
    let item_id = item["id"].as_str().unwrap().to_owned();
    let created = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/documents"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "복원 중 편집 문서" })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    let id = created.json["id"].as_str().unwrap().to_owned();
    // 되돌릴 거리: 제목만 바꾼 리비전 2(실시간 중 저장은 제목만).
    let retitled = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{id}"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": created.json["revision"],
                     "title": "바뀐 제목", "body": created.json["body"] }),
        ),
    )
    .await;
    assert_eq!(retitled.status, StatusCode::OK, "{:?}", retitled.json);
    let mut a = connect(addr, Some(&admin), &id, true).await.unwrap();
    // 편집을 받아 들였는지 볼 다른 연결(복원 전에 붙어 둔다).
    let mut b = connect(addr, Some(&admin), &id, true).await.unwrap();
    let current = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let mut hold = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT 1 FROM fvoci.items WHERE id = $1::uuid FOR UPDATE")
        .bind(&item_id)
        .execute(&mut *hold)
        .await
        .unwrap();
    let request = tokio::spawn({
        let (app, admin, id, base) = (
            app.clone(),
            admin.clone(),
            id.clone(),
            current["revision"].clone(),
        );
        async move { restore_rev1(&app, &admin, &id, &base).await }
    });
    until_waiting_on_lock("app_restore_document_revision", holder).await;
    // 복원이 잠금을 기다리는 동안 편집을 보내고 저장을 시킨다.
    let acks_before = a.persisted.len();
    a.paragraph("복원 중에 보낸 문단").await;
    // 방이 그 편집을 받아 들였으면 다른 연결에 보인다. 그것을 확인한 뒤 저장을 시킨다(고치기 전: 받아 저장하고 확인을
    // 보낸다). 고친 뒤에는 방이 복원 중이라 받아 들이지 않으므로, 기다림은 상한(5초)에서 끝난다.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !b.text().contains("복원 중에 보낸 문단") && tokio::time::Instant::now() < deadline
    {
        let _ = b.next(Duration::from_millis(200)).await;
    }
    let ws_id: uuid::Uuid = ws.parse().unwrap();
    state.collab.persist_workspace(ws_id).await;
    hold.rollback().await.unwrap();
    let r = request.await.unwrap();
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert!(!r.json["body"].to_string().contains("복원 중에 보낸 문단"));
    assert_eq!(a.closed().await, Some(4409));
    assert_eq!(
        a.persisted.len(),
        acks_before,
        "the edit sent during the restore was acknowledged and then reset away"
    );
    owner.close().await;
}

/// 감사 S6: 실시간 편집 중인 문서를 실시간이 꺼진 워크스페이스로 옮기면, 옮기기 전에 방을 리비전으로 남기지 않아
/// 리비전이 아직 없는 실시간 편집이 남겨졌다. 편집자가 새 자리에 쓸 수 없으면 방이 닫힐 때도 저장하지 못하고, 본문은
/// 옮기기 전 마지막 리비전으로 남아 다음 일반 저장이 그 편집을 버렸다. 옮기기 전에 열린 방을 리비전으로 남긴다.
#[tokio::test]
async fn moving_a_live_document_into_a_realtime_off_workspace_keeps_the_live_edits() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "1")])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let from = create_workspace(&app, &admin, "실시간 켠 곳").await;
    let to = create_workspace(&app, &admin, "실시간 끈 곳").await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    // 편집자는 옮기기 전 워크스페이스의 작성자일 뿐 새 워크스페이스의 멤버가 아니다.
    let (writer, _) = add_member(&app, &state, &admin, &from, "writer", "writer").await;
    let doc = common::create_doc(&app, &admin, &from, "회의록").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let mut e = connect(addr, Some(&writer), &id, true).await.unwrap();
    e.paragraph("옮기기 직전에 쓴 문단").await;
    e.sync().await;
    // 방이 그 문단을 받았는지 다른 연결로 확인한 뒤 옮긴다(리뷰: 보낸 변경이 방에 닿기 전에 옮기면 시험이 흔들린다).
    let mut watcher = connect(addr, Some(&admin), &id, true).await.unwrap();
    assert!(
        watcher
            .until(|c| c.text().contains("옮기기 직전에 쓴 문단"))
            .await
    );

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
    // 옮기면 방은 편집을 리비전으로 남기고 연결을 닫는다(새 자리의 권한·설정으로 다시 연다, #71).
    assert_eq!(e.closed().await, Some(4409));
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
        saved["body"].to_string().contains("옮기기 직전에 쓴 문단"),
        "{saved}"
    );
    watcher.close().await;
}

/// 실시간을 켠 곳(작성자 하나)과 끈 곳, 그 작성자가 실시간으로 쓰고 방이 받은 문서를 만든다.
async fn live_move_setup(
    label: &str,
) -> (
    Router,
    SocketAddr,
    TestUser,
    TestUser,
    String,
    String,
    String,
    Client,
    Client,
) {
    // 권한 재확인(기본 30초)이 시험 중에 돌아 방을 닫지 않게 한다.
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "3600")]))
            .unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let from = create_workspace(&app, &admin, &format!("{label} 켠 곳")).await;
    let to = create_workspace(&app, &admin, &format!("{label} 끈 곳")).await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let (writer, _) = add_member(&app, &state, &admin, &from, "writer", "writer").await;
    let doc = common::create_doc(&app, &admin, &from, label).await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let mut e = connect(addr, Some(&writer), &id, true).await.unwrap();
    e.paragraph("리비전이 아직 없는 문단").await;
    let mut watcher = connect(addr, Some(&admin), &id, true).await.unwrap();
    assert!(
        watcher
            .until(|c| c.text().contains("리비전이 아직 없는 문단"))
            .await
    );
    (app, addr, admin, writer, from, to, id, e, watcher)
}

async fn move_to(app: &Router, user: &TestUser, id: &str, to: &str) -> common::Reply {
    let target = json!({ "target_workspace_id": to, "target_item_id": null });
    let preview = send(
        app,
        Method::POST,
        &format!("/api/documents/{id}/move/preview"),
        Some(user),
        Some(target),
    )
    .await;
    let ack = json!({ "gains": preview.json["gains"], "loses": preview.json["loses"] });
    send(
        app,
        Method::POST,
        &format!("/api/documents/{id}/move"),
        Some(user),
        Some(json!({ "target_workspace_id": to, "target_item_id": null, "acknowledged": ack })),
    )
    .await
}

/// 리뷰(감사 S6 후속): 옮기기 전 방의 리비전 남기기가 실패해도(쓰기 오류·시간 초과) 그대로 옮겨, 실시간이 꺼진 새
/// 자리에서 리비전이 되지 않은 편집이 남겨졌다. 남기지 못하면 옮기지 않는다(409, 편집은 방에 남는다). 다시 하면 옮긴다.
/// 문서 행을 다른 트랜잭션이 잠가 체크포인트를 시간 초과로 붙잡는다(표 전체를 잠그는 트리거는 같은 바이너리의 다른
/// 시험을 늦춘다).
#[tokio::test]
async fn a_failed_checkpoint_stops_the_move() {
    let (app, _addr, admin, _writer, from, to, id, e, watcher) =
        live_move_setup("체크포인트 실패 이동").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let mut hold = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT 1 FROM fvoci.documents WHERE id = $1::uuid FOR UPDATE")
        .bind(&id)
        .execute(&mut *hold)
        .await
        .unwrap();
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move { move_to(&app, &admin, &id, &to).await }
    });
    // 답이 오거나(체크포인트 시간 초과) 이동이 문서 행 잠금을 기다리기 시작하면(고치기 전: 체크포인트 결과를 버리고
    // 옮김) 잠금을 푼다.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    // 다른 역할의 질의 내용은 관리자만 본다.
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let watcher_db = sqlx::PgPool::connect(&admin_url).await.unwrap();
    while !request.is_finished() {
        // 같은 행을 먼저 기다리는 연결(방의 리비전 남기기)이 있으면 이동은 그 연결에 막힌 것으로 보인다(행 잠금 줄).
        let moving: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity a WHERE a.wait_event_type = 'Lock'
                                AND a.query = 'SELECT 1 FROM documents WHERE id = $1 FOR UPDATE'
                                AND ($1 = ANY (pg_blocking_pids(a.pid))
                                     OR EXISTS (SELECT 1 FROM unnest(pg_blocking_pids(a.pid)) b(pid)
                                                WHERE $1 = ANY (pg_blocking_pids(b.pid)))))",
        )
        .bind(holder)
        .fetch_one(&watcher_db)
        .await
        .unwrap();
        if moving {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the move neither answered nor proceeded"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    watcher_db.close().await;
    hold.rollback().await.unwrap();
    let refused = request.await.unwrap();
    let doc = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{:?}", refused.json);
    assert!(refused.json.get("gains").is_none(), "{:?}", refused.json);
    assert_eq!(doc["workspace_id"], from.as_str(), "the document moved");

    // 다시 하면 방의 편집을 리비전으로 남기고 옮긴다.
    let moved = move_to(&app, &admin, &id, &to).await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    let saved = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(saved["workspace_id"], to.as_str());
    assert!(
        saved["body"]
            .to_string()
            .contains("리비전이 아직 없는 문단"),
        "{saved}"
    );
    owner.close().await;
    e.close().await;
    watcher.close().await;
}

/// 리뷰: 옮길 권한이 없는 요청도 권한을 보기 전에 방에 리비전을 남기게 해, 거절될 요청으로 다른 사람의 열린 편집을
/// 리비전으로 남기거나 방을 기다리게 할 수 있었다. 옮길 수 있는 사람의 요청만 방에 보낸다.
#[tokio::test]
async fn a_move_without_permission_leaves_the_room_alone() {
    let (app, _addr, admin, writer, _from, _to, id, e, watcher) =
        live_move_setup("권한 없는 이동").await;
    let elsewhere = create_workspace(&app, &writer, "작성자의 곳").await;
    let before = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    // 미리보기는 권한이 없어 거절되므로, 형식만 맞는 확인 값으로 바로 옮기기를 보낸다.
    let refused = send(
        &app,
        Method::POST,
        &format!("/api/documents/{id}/move"),
        Some(&writer),
        Some(
            json!({ "target_workspace_id": elsewhere, "target_item_id": null,
                     "acknowledged": { "gains": [], "loses": [] } }),
        ),
    )
    .await;
    assert!(
        matches!(
            refused.status,
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ),
        "{:?}",
        refused.json
    );
    let after = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(
        after["revision"], before["revision"],
        "the refused move made the room save a revision"
    );
    e.close().await;
    watcher.close().await;
}

impl Client {
    /// 문단을 보내되, 서버가 이미 연결을 닫았으면 보내지 못해도 넘어간다.
    async fn try_paragraph(&mut self, text: &'static str) {
        let before = self.doc.transact().state_vector();
        let fragment = self.doc.get_or_insert_xml_fragment(FRAGMENT);
        {
            let mut txn = self.doc.transact_mut();
            let p = fragment.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
            p.push_back(&mut txn, XmlTextPrelim::new(text));
        }
        let diff = self.doc.transact().encode_state_as_update_v1(&before);
        let frame = Message::Sync(SyncMessage::Update(diff)).encode_v1();
        let _ignored = self.ws.send(WsMessage::Binary(frame.into())).await;
    }

    /// 앞서 보낸 프레임을 서버가 모두 읽어 방의 대기열에 넣을 때까지 기다린다. 서버는 프레임을 차례로 읽고(편집은 방의
    /// 대기열에 넣은 뒤에 다음 프레임을 읽는다) 핑의 퐁은 그다음 읽기에서 보낸다: 퐁이 오면 앞의 편집은 대기열에 있다.
    async fn read_by_server(&mut self) {
        self.ws
            .send(WsMessage::Ping(Default::default()))
            .await
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            match self.next(Duration::from_millis(500)).await {
                Event::Pong => return,
                Event::Closed(code) => panic!("the connection closed ({code}) before the pong"),
                Event::Data | Event::Timeout => {
                    assert!(tokio::time::Instant::now() < deadline, "no pong");
                }
            }
        }
    }
}

/// 받은 저장 확인 가운데 이 연결이 보낸 편집을 모두 담은 것이 있는지(브라우저는 그때 복구용 초안을 지운다).
fn acked(c: &Client) -> bool {
    let mine = c.doc.client_id();
    let sent = c.doc.transact().state_vector().get(&mine);
    c.persisted.iter().any(|bytes| {
        yrs::Snapshot::decode_v1(bytes)
            .unwrap()
            .state_map
            .get(&mine)
            >= sent
    })
}

/// #71 시험 준비: 실시간을 켠 곳(작성자·독자)과 끈 곳, 작성자가 실시간으로 쓴 문단을 독자 연결이 받은 문서. 작성자는 끈
/// 곳의 멤버가 아니다. 지켜보는 연결은 독자다: 쓸 수 있는 사람이 연결돼 있으면 방이 닫힐 때 그 사람이 작성자가 되어 잃음이
/// 가려진다. 권한 재확인(기본 30초)은 시험 중에 돌지 않게 한다. (상태, 앱, 주소, 관리자, 켠 곳, 끈 곳, 문서 id, 작성자
/// 연결, 독자 연결).
async fn live_move_race_setup(label: &str) -> RaceSetup {
    live_move_race_setup_with_writer(label).await.1
}

/// (상태, 앱, 주소, 관리자, 켠 곳, 끈 곳, 문서 id, 작성자 연결, 독자 연결).
type RaceSetup = (
    fvoci_server::http::AppState,
    Router,
    SocketAddr,
    TestUser,
    String,
    String,
    String,
    Client,
    Client,
);

/// `live_move_race_setup`과 같고 작성자도 돌려준다(그 사람의 세션·권한을 바꾸는 시험).
async fn live_move_race_setup_with_writer(label: &str) -> (TestUser, RaceSetup) {
    live_move_race_setup_rechecking(label, "3600").await
}

/// `live_move_race_setup_with_writer`와 같되 권한 재확인 간격(초)을 정한다.
async fn live_move_race_setup_rechecking(label: &str, recheck_secs: &str) -> (TestUser, RaceSetup) {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", recheck_secs)]))
            .unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let from = create_workspace(&app, &admin, &format!("{label} 켠 곳")).await;
    let to = create_workspace(&app, &admin, &format!("{label} 끈 곳")).await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let (writer, _) = add_member(&app, &state, &admin, &from, "writer", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &from, "reader", "reader").await;
    let doc = common::create_doc(&app, &admin, &from, label).await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let mut e = connect(addr, Some(&writer), &id, true).await.unwrap();
    e.paragraph("체크포인트 전 문단").await;
    let mut r = connect(addr, Some(&reader), &id, true).await.unwrap();
    assert!(r.until(|c| c.text().contains("체크포인트 전 문단")).await);
    (writer, (state, app, addr, admin, from, to, id, e, r))
}

/// 이 워크스페이스의 문서 나무 잠금(이동이 새 자리에서 `app_lock_move`로 잡는 것)을 쥔 트랜잭션과 그 연결의 pid.
async fn hold_tree(
    owner: &sqlx::PgPool,
    workspace: &str,
) -> (sqlx::Transaction<'static, sqlx::Postgres>, i32) {
    let mut hold = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT fvoci.app_lock_tree($1::uuid, NULL, NULL)")
        .bind(workspace)
        .execute(&mut *hold)
        .await
        .unwrap();
    (hold, holder)
}

async fn get_doc(app: &Router, user: &TestUser, id: &str) -> serde_json::Value {
    send(
        app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(user),
        None,
    )
    .await
    .json
}

/// #71: 옮기기 전 체크포인트가 답한 뒤 이동이 커밋되기까지 온 편집. 전에는 방이 그 편집을 받아 저장 확인을 보냈고(브라우저는
/// 초안을 지운다), 옮긴 곳에서 편집자가 쓸 수 없고 실시간이 꺼져 있으면 방이 닫힐 때 리비전으로 남지 못해 사라졌다. 이제
/// 이동은 방 안에서 한다: 그동안 편집은 방의 대기열에서 기다리고(넣지도 확인하지도 않음), 커밋 뒤 그 편집을 넣어 리비전(옮기는
/// 사람이 대신 작성자)으로 남긴 다음에야 확인을 보내고 연결을 4409로 닫는다. 새 자리의 나무 잠금을 쥐어 이동을 체크포인트
/// 뒤에 붙잡는다.
#[tokio::test]
async fn edits_between_the_checkpoint_and_the_move_are_kept_or_never_acknowledged() {
    let (_state, app, _addr, admin, _from, to, id, mut e, mut r) =
        live_move_race_setup("체크포인트 뒤 편집").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let (hold, holder) = hold_tree(&owner, &to).await;
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move { move_to(&app, &admin, &id, &to).await }
    });
    // 이동이 새 자리 잠금을 기다린다: 체크포인트는 이미 답했다.
    until_waiting_on_lock("app_lock_move", holder).await;
    e.paragraph("체크포인트 뒤 문단").await;
    // 이동이 붙잡힌 동안 주기 저장(3초)보다 오래 지켜본다.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    while tokio::time::Instant::now() < deadline {
        let _ = e.next(Duration::from_millis(100)).await;
        let _ = r.next(Duration::from_millis(100)).await;
    }
    let acked_while_held = acked(&e);
    hold.rollback().await.unwrap();
    let moved = request.await.unwrap();
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    // 방이 연결을 닫을 때까지(고친 뒤: 4409) 오는 저장 확인을 모은다.
    let writer_closed = e.closed().await;
    let acked_at_all = acked(&e);
    // 고치기 전에는 연결이 열려 있다. 편집자가 권한 재확인으로 닫히는 것처럼 닫는다(마지막 사람이 나가면 방이 닫힌다).
    e.close().await;
    r.close().await;
    let mut saved = get_doc(&app, &admin, &id).await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !saved["body"].to_string().contains("체크포인트 뒤 문단")
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(100)).await;
        saved = get_doc(&app, &admin, &id).await;
    }
    let kept = saved["body"].to_string().contains("체크포인트 뒤 문단");
    assert!(
        kept || !acked_at_all,
        "the edit made after the checkpoint was acknowledged but is not in the moved document: {saved}"
    );
    // 고친 동작: 이동이 끝나기 전에는 확인하지 않고, 커밋 뒤 리비전으로 남긴 다음 확인하고 4409로 닫는다.
    assert!(
        !acked_while_held,
        "the edit was acknowledged while the move was pending"
    );
    assert!(kept, "{saved}");
    assert!(acked_at_all, "the kept edit was never acknowledged");
    assert_eq!(writer_closed, Some(4409));
    assert_eq!(saved["workspace_id"], to.as_str());
    let (last, author) = last_revision(&owner, &id).await;
    assert_eq!(
        author, admin.id.0,
        "the mover stands in for an editor who cannot write in the new place"
    );
    // D-95: 체크포인트 리비전은 고친 사람(아직 옛 자리에서 쓸 수 있는 작성자)이 남긴 보통 리비전이다. 커밋 뒤 리비전은
    // 옮기는 사람이 대신 남겼으므로 실제로 고친 사람을 따로 적는다.
    let (editor, checkpoint_edited_by) = revision_authors(&owner, &id, last - 1).await;
    assert_ne!(editor, admin.id.0);
    assert_eq!(checkpoint_edited_by, None, "an editor's own revision");
    assert_eq!(
        revision_authors(&owner, &id, last).await.1,
        Some(vec![editor]),
        "the stand-in revision does not name the actual editor"
    );
    // 이동의 답은 그 리비전까지 담은 문서다(화면이 그 판으로 이어 간다).
    assert_eq!(moved.json["document"]["revision"], saved["revision"]);
    assert!(
        moved.json["document"]["body"]
            .to_string()
            .contains("체크포인트 뒤 문단")
    );
    owner.close().await;
}

/// #71: 이동이 붙잡혀 있는 동안 대기열에서 기다린 편집은, 이동이 일어나지 않으면(확인한 미리보기가 달라 409) 평소처럼
/// 받아 다른 연결에 보내고 저장을 확인한다. 연결은 그대로다.
#[tokio::test]
async fn edits_held_by_a_move_that_does_not_happen_are_accepted() {
    let (_state, app, _addr, admin, from, to, id, mut e, mut r) =
        live_move_race_setup("일어나지 않은 이동").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let (hold, holder) = hold_tree(&owner, &to).await;
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move {
            // 미리보기와 다른 확인 값: 잠근 뒤 다시 계산한 미리보기와 달라 옮기지 않는다.
            send(
                &app,
                Method::POST,
                &format!("/api/documents/{id}/move"),
                Some(&admin),
                Some(json!({ "target_workspace_id": to, "target_item_id": null,
                             "acknowledged": { "gains": ["없는 사람"], "loses": [] } })),
            )
            .await
        }
    });
    until_waiting_on_lock("app_lock_move", holder).await;
    e.paragraph("체크포인트 뒤 문단").await;
    // 서버가 그 편집을 방의 대기열(붙잡힌 이동 명령 뒤)에 넣은 것을 확인한 뒤 잠금을 푼다. 방은 이동 명령 안에 있어 아직
    // 그 편집을 넣지도 확인하지도 않았다.
    e.read_by_server().await;
    assert!(
        !acked(&e),
        "the held edit was acknowledged while the move was pending"
    );
    hold.rollback().await.unwrap();
    let refused = request.await.unwrap();
    assert_eq!(refused.status, StatusCode::CONFLICT, "{:?}", refused.json);
    assert!(refused.json["gains"].is_array(), "{:?}", refused.json);
    assert!(
        r.until(|c| c.text().contains("체크포인트 뒤 문단")).await,
        "the held edit did not reach the other connection"
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let mut closed = None;
    while !acked(&e) && tokio::time::Instant::now() < deadline {
        if let Event::Closed(code) = e.next(Duration::from_millis(200)).await {
            closed = Some(code);
            break;
        }
    }
    assert_eq!(closed, None, "the connection was closed");
    assert!(acked(&e), "the held edit was not acknowledged");
    let doc = get_doc(&app, &admin, &id).await;
    assert_eq!(doc["workspace_id"], from.as_str(), "the document moved");
    owner.close().await;
    e.close().await;
    r.close().await;
}

/// D-95 조건 4(#71): 이동 요청이 방이 그 명령을 처리하기 전에 끊기면 방은 체크포인트 리비전을 남기지 않고 옮기지도 않는다.
/// 그 명령 뒤에서 기다린 편집은 평소처럼 받아 확인하고 원래 자리에 남는다. 방은 주기 저장에서 붙잡는다(상태 행을 잠가
/// 저장이 기다림). 이동은 요청 처리기가 미리보기 확인 뒤 부르는 `Rooms::move_document`를 한 번만 진행해(이동 명령을 방의
/// 대기열에 넣고 답을 기다리는 자리) 버린다: 요청이 끊기면 axum이 처리기를 그 자리에서 버리는 것과 같다. 시험과 방은 한
/// 스레드에서 돌므로, 진행한 뒤 버리기까지 방이 그 명령을 처리하지 않는다.
#[tokio::test]
async fn a_move_dropped_before_the_room_handles_it_changes_nothing() {
    let (state, app, _addr, admin, from, to, id, mut e, mut r) =
        live_move_race_setup("끊긴 이동").await;
    assert!(
        e.until(acked).await,
        "the first edit was never acknowledged"
    );
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let before = last_revision(&owner, &id).await;
    let mut hold = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query(
        "SELECT 1 FROM fvoci.document_collab_states WHERE document_id = $1::uuid FOR UPDATE",
    )
    .bind(&id)
    .execute(&mut *hold)
    .await
    .unwrap();
    // 다음 편집의 주기 저장이 그 잠금을 기다린다: 방은 그동안 대기열을 읽지 않는다.
    e.paragraph("붙잡힌 저장의 문단").await;
    until_waiting_on_lock("app_system_collab_store", holder).await;

    let preview = send(
        &app,
        Method::POST,
        &format!("/api/documents/{id}/move/preview"),
        Some(&admin),
        Some(json!({ "target_workspace_id": to, "target_item_id": null })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    let request = fvoci_server::collab::room::MoveRequest {
        doc: fvoci_server::ids::DocumentId(uuid::Uuid::parse_str(&id).unwrap()),
        user: admin.id,
        input: serde_json::from_value(json!({ "target_workspace_id": to, "target_item_id": null,
            "acknowledged": { "gains": preview.json["gains"], "loses": preview.json["loses"] } }))
        .unwrap(),
    };
    let mut moving = Box::pin(
        state
            .collab
            .move_document(&state.db, &state.config.limits, request),
    );
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(
        moving.as_mut().poll(&mut cx).is_pending(),
        "the move did not wait for the room"
    );
    // 그 뒤에 보낸 편집은 방의 대기열에서 이동 명령 뒤에 기다린다.
    e.paragraph("끊긴 이동 뒤 문단").await;
    e.read_by_server().await;
    // 방이 이동 명령을 처리하기 전에 요청이 끊긴다. 그다음 방을 놓아 준다.
    drop(moving);
    hold.rollback().await.unwrap();

    assert!(
        r.until(|c| c.text().contains("끊긴 이동 뒤 문단")).await,
        "the held edit did not reach the other connection"
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let mut closed = None;
    while !acked(&e) && tokio::time::Instant::now() < deadline {
        if let Event::Closed(code) = e.next(Duration::from_millis(200)).await {
            closed = Some(code);
            break;
        }
    }
    assert_eq!(closed, None, "the connection was closed");
    assert!(acked(&e), "the held edit was not acknowledged");
    let doc = get_doc(&app, &admin, &id).await;
    assert_eq!(doc["workspace_id"], from.as_str(), "the document moved");
    assert_eq!(
        last_revision(&owner, &id).await,
        before,
        "the dropped move wrote a checkpoint revision"
    );
    assert_eq!(collab_states(&owner, &id, true).await, 1);
    owner.close().await;
    e.close().await;
    r.close().await;
}

/// #71: 열린 방의 문서를 옮기면, 옮기기 직전에 쓴 편집은 리비전에 남고 연결은 4409로 닫힌다(새 자리의 권한·설정으로 다시
/// 연다). 전에는 방이 옛 연결을 그대로 받아, 새 자리에서 권한이 없는 편집자의 편집도 다음 권한 재확인까지 받았다. 옮긴
/// 뒤에 보낸 편집은 받지 않으므로 확인도 오지 않는다(브라우저 초안에 남는다).
#[tokio::test]
async fn a_move_closes_the_room_after_saving_its_edits() {
    let (_state, app, _addr, admin, _from, to, id, mut e, mut r) =
        live_move_race_setup("옮기면 닫히는 방").await;
    e.paragraph("옮기기 직전 문단").await;
    assert!(r.until(|c| c.text().contains("옮기기 직전 문단")).await);
    let moved = move_to(&app, &admin, &id, &to).await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    e.try_paragraph("옮긴 뒤 문단").await;
    assert_eq!(
        e.closed().await,
        Some(4409),
        "the editor's connection stayed open after the move"
    );
    assert_eq!(r.closed().await, Some(4409));
    assert!(!acked(&e), "an edit sent after the move was acknowledged");
    let saved = get_doc(&app, &admin, &id).await;
    assert_eq!(saved["workspace_id"], to.as_str());
    assert!(
        saved["body"].to_string().contains("옮기기 직전 문단"),
        "{saved}"
    );
    assert!(
        !saved["body"].to_string().contains("옮긴 뒤 문단"),
        "{saved}"
    );
}

/// #71: 열린 방이 없는 문서를 옮기는 동안에는 방이 새로 열리지 않는다(다시 붙으라는 4503). 전에는 옮기는 사이 들어온
/// 편집자가 옛 자리의 권한으로 방을 열어, 옮긴 뒤에도 그 방에 편집을 보낼 수 있었다. 옮긴 뒤 새 자리의 멤버가 아닌
/// 사람은 붙지 못하고, 이동이 끝나면 막음이 풀린다.
#[tokio::test]
async fn no_room_opens_while_a_document_moves() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "3600")]))
            .unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let from = create_workspace(&app, &admin, "방 없이 옮기는 곳").await;
    let to = create_workspace(&app, &admin, "방 없이 옮겨 갈 곳").await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let (writer, _) = add_member(&app, &state, &admin, &from, "writer", "writer").await;
    let doc = common::create_doc(&app, &admin, &from, "방 없이 옮길 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let (hold, holder) = hold_tree(&owner, &to).await;
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move { move_to(&app, &admin, &id, &to).await }
    });
    until_waiting_on_lock("app_lock_move", holder).await;
    assert_eq!(
        refused_code(addr, &writer, &id).await,
        Some(4503),
        "a live editing room opened while the document was moving"
    );
    hold.rollback().await.unwrap();
    let moved = request.await.unwrap();
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    assert_eq!(
        connect(addr, Some(&writer), &id, true).await.err(),
        Some(StatusCode::NOT_FOUND)
    );
    // 이동이 끝나면 막음이 풀린다: 되돌려 옮기면 그 편집자가 다시 방을 연다.
    let back = move_to(&app, &admin, &id, &from).await;
    assert_eq!(back.status, StatusCode::OK, "{:?}", back.json);
    let w = connect(addr, Some(&writer), &id, true).await.unwrap();
    w.close().await;
    owner.close().await;
}

/// 이 문서의 실시간 상태 행 수(`pending`이면 리비전으로 아직 남지 않은 것만).
async fn collab_states(owner: &sqlx::PgPool, id: &str, pending: bool) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.document_collab_states WHERE document_id = $1::uuid AND (pending OR NOT $2)",
    )
    .bind(id)
    .bind(pending)
    .fetch_one(owner)
    .await
    .unwrap()
}

/// 마지막 리비전의 번호와 작성자.
async fn last_revision(owner: &sqlx::PgPool, id: &str) -> (i32, uuid::Uuid) {
    sqlx::query_as(
        "SELECT revision, author FROM fvoci.document_revisions WHERE document_id = $1::uuid ORDER BY revision DESC LIMIT 1",
    )
    .bind(id)
    .fetch_one(owner)
    .await
    .unwrap()
}

/// 그 리비전의 작성자와 실제 편집자(작성자가 고친 사람 대신 남겼을 때만 있다, D-95).
async fn revision_authors(
    owner: &sqlx::PgPool,
    id: &str,
    revision: i32,
) -> (uuid::Uuid, Option<Vec<uuid::Uuid>>) {
    sqlx::query_as(
        "SELECT author, edited_by FROM fvoci.document_revisions WHERE document_id = $1::uuid AND revision = $2",
    )
    .bind(id)
    .bind(revision)
    .fetch_one(owner)
    .await
    .unwrap()
}

/// 이 문서의 방이 닫혀 서버의 방 목록에서 빠질 때까지 기다린다(방은 닫히며 리비전·상태를 다 남긴 뒤에 빠진다).
async fn until_no_room(state: &fvoci_server::http::AppState, id: &str) {
    let doc = fvoci_server::ids::DocumentId(uuid::Uuid::parse_str(id).unwrap());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while state.collab.has_room(doc) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the live editing room of {id} stayed open"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 실시간 상태 행이 조건에 맞을 때까지 기다린다(방이 닫히며 저장하는 것).
async fn until_collab_states(owner: &sqlx::PgPool, id: &str, pending: bool, want: i64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while collab_states(owner, id, pending).await != want {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the live state of {id} did not reach {want} (pending: {pending})"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// #71 리뷰: 열린 방 없이 남은 실시간 상태(앞선 방이 리비전을 남기지 못하고 닫힘: 편집자가 독자가 되어 쓸 수 있는 사람이
/// 없었다)는 옮기기 전에 리비전이 되지 않았다. 그 편집은 이미 저장 확인을 받았고(브라우저는 초안을 지운다), 실시간이 꺼진
/// 곳으로 옮기면 되살릴 길이 없어 다음 일반 저장이 버렸다. 이제 방이 없으면 옮기기 전에 그 상태를 리비전으로 남긴다(옮기는
/// 사람이 대신 작성자, 실제 편집자는 리비전에 따로 적는다, D-95). 남기지 못하면 옮기지 않고 409이며 상태는 남는다. 방을 연
/// 적 없는 문서와 방을 열기만 한 문서(편집기가 저장한 본문의 null 속성은 Yjs에 남지 않는다)는 옮겨도 상태 행도 리비전도
/// 새로 생기지 않는다.
#[tokio::test]
async fn a_move_without_a_room_saves_a_live_state_left_pending() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "1")])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let from = create_workspace(&app, &admin, "남은 상태 켠 곳").await;
    let to = create_workspace(&app, &admin, "남은 상태 끈 곳").await;
    let (writer, _) = add_member(&app, &state, &admin, &from, "writer", "writer").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let left = common::create_doc(&app, &admin, &from, "방 없이 남은 상태").await;
    let left_id = left["id"].as_str().unwrap().to_owned();
    let never = common::create_doc(&app, &admin, &from, "방을 연 적 없는 문서").await;
    let never_id = never["id"].as_str().unwrap().to_owned();
    let opened = common::create_doc(&app, &admin, &from, "방을 열기만 한 문서").await;
    let opened_id = opened["id"].as_str().unwrap().to_owned();
    // 실시간을 켜기 전에 편집기처럼 null 속성이 든 본문을 저장한다.
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{opened_id}"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "base_revision": opened["revision"],
            "title": "방을 열기만 한 문서",
            "body": { "type": "doc", "content": [{ "type": "orderedList", "attrs": { "start": 3, "type": null },
                "content": [{ "type": "listItem", "content": [{ "type": "paragraph",
                    "content": [{ "type": "text", "text": "null 속성" }] }] }] }] } })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    // 방을 열기만 한 문서: 방이 닫히며 본문에서 만든 상태를 저장한다(리비전으로 남길 것은 없음).
    let o = connect(addr, Some(&admin), &opened_id, true).await.unwrap();
    o.close().await;
    until_collab_states(&owner, &opened_id, false, 1).await;

    // 작성자가 실시간으로 쓰고 저장 확인을 받은 뒤 독자가 되면 그 연결은 닫히고(4403), 방은 쓸 수 있는 사람이 없어 리비전을
    // 남기지 못한 채 실시간 상태만 남기고 닫힌다.
    let mut e = connect(addr, Some(&writer), &left_id, true).await.unwrap();
    e.paragraph("방 없이 남은 글").await;
    assert!(e.until(acked).await, "the edit was never acknowledged");
    let demoted = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/members/{}", writer.id.0),
        Some(&admin),
        Some(json!({ "role": "reader" })),
    )
    .await;
    assert!(demoted.status.is_success(), "{:?}", demoted.json);
    assert_eq!(e.closed().await, Some(4403));
    until_collab_states(&owner, &left_id, true, 1).await;
    // 저장 대기 행은 확인을 보낸 주기 저장이 이미 만들었다(방이 닫혔다는 표시가 아니다). 방이 서버의 방 목록에서 빠질
    // 때까지 기다려, 아래 이동이 방 없는 길을 지나게 한다.
    until_no_room(&state, &left_id).await;
    let before = get_doc(&app, &admin, &left_id).await;
    assert!(
        !before["body"].to_string().contains("방 없이 남은 글"),
        "{before}"
    );

    // D-95 조건 3: 그 상태를 리비전으로 남기지 못하면(여기서는 본문 한도를 낮춘 서버라 다시 해도 남길 수 없다) 옮기지 않고
    // 409이며, 저장 대기 상태는 그대로 남는다.
    let strict = fvoci_server::app::build_state(config_with(&[
        ("FVOCI_COLLAB_RECHECK_SECS", "1"),
        ("FVOCI_MAX_DOCUMENT_BYTES", "40"),
    ]))
    .unwrap();
    let refused = move_to(&common::app(&strict), &admin, &left_id, &to).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{:?}", refused.json);
    assert_eq!(refused.json["error"], "invalid_state", "{:?}", refused.json);
    assert_eq!(
        get_doc(&app, &admin, &left_id).await["workspace_id"],
        from.as_str(),
        "the document moved"
    );
    assert_eq!(last_revision(&owner, &left_id).await.0, before["revision"]);
    assert_eq!(collab_states(&owner, &left_id, true).await, 1);

    // 방 없이 실시간이 꺼진 곳으로 옮긴다: 옮기기 전에 그 상태가 리비전으로 남는다. 고친 사람은 쓸 수 없어 옮기는 사람이
    // 대신 작성자가 되고, 실제 편집자를 리비전에 따로 적는다(D-95, 상태와 함께 저장한 편집자).
    let moved = move_to(&app, &admin, &left_id, &to).await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    let after = get_doc(&app, &admin, &left_id).await;
    assert_eq!(after["workspace_id"], to.as_str());
    assert!(
        after["body"].to_string().contains("방 없이 남은 글"),
        "the acknowledged live edit left without a room is not in the moved document: {after}"
    );
    assert_eq!(moved.json["document"]["revision"], after["revision"]);
    let (saved_at, author) = last_revision(&owner, &left_id).await;
    assert_eq!(author, admin.id.0);
    assert_eq!(
        revision_authors(&owner, &left_id, saved_at).await.1,
        Some(vec![writer.id.0]),
        "the mover's stand-in revision does not name the actual editor"
    );
    assert_eq!(collab_states(&owner, &left_id, true).await, 0);
    // 기록 API는 대신 저장한 리비전에 실제 편집자 이름을 함께 보인다. 보통 리비전(문서를 만든 리비전)은 없다.
    let history = send(
        &app,
        Method::GET,
        &format!("/api/documents/{left_id}/revisions"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(history[0]["revision"], saved_at);
    assert_eq!(history[0]["author_name"], "admin");
    assert_eq!(history[0]["edited_by_names"], json!(["writer"]));
    assert_eq!(history[1]["edited_by_names"], serde_json::Value::Null);
    let one = send(
        &app,
        Method::GET,
        &format!("/api/documents/{left_id}/revisions/{saved_at}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(one["edited_by_names"], json!(["writer"]));

    // 방을 연 적 없는 문서: 상태 행도 리비전도 생기지 않는다.
    let never_before = last_revision(&owner, &never_id).await;
    let moved = move_to(&app, &admin, &never_id, &to).await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    assert_eq!(collab_states(&owner, &never_id, false).await, 0);
    assert_eq!(last_revision(&owner, &never_id).await, never_before);

    // 방을 열기만 한 문서: 저장된 상태가 본문과 같은 내용이라 리비전이 생기지 않는다.
    let opened_before = last_revision(&owner, &opened_id).await;
    let moved = move_to(&app, &admin, &opened_id, &to).await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    assert_eq!(
        last_revision(&owner, &opened_id).await,
        opened_before,
        "moving a document whose room only opened wrote a revision"
    );
    owner.close().await;
}

impl Client {
    /// `read_by_server`와 같지만 서버가 연결을 닫았어도 멈추지 않는다. 닫혔으면 그 코드.
    async fn try_read_by_server(&mut self) -> Option<u16> {
        let _ignored = self.ws.send(WsMessage::Ping(Default::default())).await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            match self.next(Duration::from_millis(500)).await {
                Event::Pong => return None,
                Event::Closed(code) => return Some(code),
                Event::Data | Event::Timeout => {
                    assert!(tokio::time::Instant::now() < deadline, "no pong");
                }
            }
        }
    }
}

/// 요청이 `limit` 안에 끝나면 참. 잠금을 쥔 시험이 끝없이 기다리지 않고 잠금을 풀 수 있게 한다.
async fn finishes_within<T>(request: &tokio::task::JoinHandle<T>, limit: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    while !request.is_finished() {
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    true
}

/// #64 리뷰(CodeRabbit r4343460648): 열린 방이 자기 명령 안에서 부르는 이동 트랜잭션이 잠금을 끝없이 기다려, 다른 변경이
/// 새 자리를 오래 쥐면 방(편집·저장·권한 재확인)과 이동 요청이 함께 멈췄다. 잠금 대기는 한도(10초)에서 끝나고 409
/// `invalid_state`로 옮기지 않는다. 방은 그대로 살아 그동안 기다린 편집을 받아 확인한다.
#[tokio::test]
async fn a_move_held_past_the_lock_limit_is_refused_and_the_room_carries_on() {
    let (state, app, _addr, admin, from, to, id, mut e, mut r) =
        live_move_race_setup("잠금 한도 이동").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let (hold, holder) = hold_tree(&owner, &to).await;
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move { move_to(&app, &admin, &id, &to).await }
    });
    until_waiting_on_lock("app_lock_move", holder).await;
    e.paragraph("잠금을 기다리는 동안 쓴 문단").await;
    e.read_by_server().await;
    let answered = finishes_within(&request, Duration::from_secs(30)).await;
    hold.rollback().await.unwrap();
    assert!(
        answered,
        "the move kept waiting on the lock past the limit (the room was stuck)"
    );
    let refused = request.await.unwrap();
    assert_eq!(refused.status, StatusCode::CONFLICT, "{:?}", refused.json);
    assert_eq!(refused.json["error"], "invalid_state", "{:?}", refused.json);
    assert!(refused.json.get("gains").is_none(), "{:?}", refused.json);
    // 방은 살아 있다: 기다린 편집을 받아 다른 연결에 보내고 확인한다. 연결은 열려 있다.
    assert!(
        r.until(|c| c.text().contains("잠금을 기다리는 동안 쓴 문단"))
            .await,
        "the held edit did not reach the other connection"
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let mut closed = None;
    while !acked(&e) && tokio::time::Instant::now() < deadline {
        if let Event::Closed(code) = e.next(Duration::from_millis(200)).await {
            closed = Some(code);
            break;
        }
    }
    assert_eq!(closed, None, "the connection was closed");
    assert!(acked(&e), "the held edit was not acknowledged");
    let doc = fvoci_server::ids::DocumentId(uuid::Uuid::parse_str(&id).unwrap());
    assert!(state.collab.has_room(doc), "the room closed");
    assert_eq!(
        get_doc(&app, &admin, &id).await["workspace_id"],
        from.as_str(),
        "the document moved"
    );
    // 잠금이 풀리면 다시 해서 옮긴다.
    let moved = move_to(&app, &admin, &id, &to).await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    assert!(
        get_doc(&app, &admin, &id).await["body"]
            .to_string()
            .contains("잠금을 기다리는 동안 쓴 문단")
    );
    owner.close().await;
    e.close().await;
    r.close().await;
}

/// #64 리뷰: 방이 없는 이동도 문서 행을 다른 트랜잭션이 한도보다 오래 잠그면 끝없이 기다리지 않고 409 `invalid_state`로
/// 옮기지 않는다. 그 요청이 막아 둔 방 열기도 풀린다.
#[tokio::test]
async fn a_move_without_a_room_held_past_the_lock_limit_is_refused() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "3600")]))
            .unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let from = create_workspace(&app, &admin, "잠금 한도 방 없는 곳").await;
    let to = create_workspace(&app, &admin, "잠금 한도 방 없이 갈 곳").await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let doc = common::create_doc(&app, &admin, &from, "잠금 한도 방 없는 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let mut hold = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT 1 FROM fvoci.documents WHERE id = $1::uuid FOR UPDATE")
        .bind(&id)
        .execute(&mut *hold)
        .await
        .unwrap();
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move { move_to(&app, &admin, &id, &to).await }
    });
    until_waiting_on_lock("FROM documents WHERE id = $1 FOR UPDATE", holder).await;
    let answered = finishes_within(&request, Duration::from_secs(30)).await;
    hold.rollback().await.unwrap();
    assert!(answered, "the move kept waiting on the lock past the limit");
    let refused = request.await.unwrap();
    assert_eq!(refused.status, StatusCode::CONFLICT, "{:?}", refused.json);
    assert_eq!(refused.json["error"], "invalid_state", "{:?}", refused.json);
    assert_eq!(
        get_doc(&app, &admin, &id).await["workspace_id"],
        from.as_str()
    );
    // 막음이 풀렸다: 방을 열 수 있다.
    let c = connect(addr, Some(&admin), &id, true).await.unwrap();
    c.close().await;
    owner.close().await;
}

/// #64 리뷰(CodeRabbit 요약의 남은 우려): 이동이 잠금을 기다리는 동안 방은 권한 재확인을 하지 못한다. 그동안 세션이 끝난
/// 작성자가 보낸 편집은 커밋 전에 받은 것이라 옮긴 문서에 리비전으로 들어갔다. 이제 넣기 전에 세션을 다시 보고, 끝났으면
/// 그 연결을 닫고(4403) 편집을 버린다(확인하지 않는다).
#[tokio::test]
async fn an_edit_sent_after_the_session_ended_during_a_move_is_dropped() {
    let (writer, (_state, app, _addr, admin, _from, to, id, mut e, r)) =
        live_move_race_setup_with_writer("세션이 끝난 이동").await;
    // 잠금 대기 전에 받아 확인한(저장 확인을 보낸) 편집: 세션이 끝나도 옮긴 문서의 리비전에 남아야 한다.
    assert!(
        e.until(acked).await,
        "the edit before the wait was never acknowledged"
    );
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let (hold, holder) = hold_tree(&owner, &to).await;
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move { move_to(&app, &admin, &id, &to).await }
    });
    until_waiting_on_lock("app_lock_move", holder).await;
    let logout = send(&app, Method::POST, "/auth/logout", Some(&writer), None).await;
    assert!(logout.status.is_success(), "{:?}", logout.json);
    e.paragraph("세션이 끝난 뒤 문단").await;
    e.read_by_server().await;
    hold.rollback().await.unwrap();
    let moved = request.await.unwrap();
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    assert_eq!(
        e.closed().await,
        Some(4403),
        "the connection whose session ended was not closed for lost access"
    );
    assert!(
        !acked(&e),
        "the edit sent after the session ended was acknowledged"
    );
    let saved = get_doc(&app, &admin, &id).await;
    assert_eq!(saved["workspace_id"], to.as_str());
    assert!(
        saved["body"].to_string().contains("체크포인트 전 문단"),
        "{saved}"
    );
    assert!(
        !saved["body"].to_string().contains("세션이 끝난 뒤 문단"),
        "an edit sent after the session ended was saved: {saved}"
    );
    owner.close().await;
    r.close().await;
}

/// #64 리뷰: 마지막 주기 재확인 뒤 쓰기 권한을 잃은 작성자의 편집이, 이동이 잠금을 기다리는 동안 대기열에 들어와 옛 권한으로
/// 받아졌다. 이제 이동 트랜잭션 바로 앞에서 권한을 다시 보아 그 연결을 닫는다(4403). 그 뒤 보낸 편집은 받지 않는다.
#[tokio::test]
async fn an_editor_who_lost_write_access_before_a_move_cannot_edit_during_it() {
    let (writer, (_state, app, _addr, admin, from, to, id, mut e, r)) =
        live_move_race_setup_with_writer("권한을 잃은 이동").await;
    let demoted = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/members/{}", writer.id.0),
        Some(&admin),
        Some(json!({ "role": "reader" })),
    )
    .await;
    assert!(demoted.status.is_success(), "{:?}", demoted.json);
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let (hold, holder) = hold_tree(&owner, &to).await;
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move { move_to(&app, &admin, &id, &to).await }
    });
    until_waiting_on_lock("app_lock_move", holder).await;
    e.try_paragraph("권한을 잃은 뒤 문단").await;
    let closed_while_held = e.try_read_by_server().await;
    hold.rollback().await.unwrap();
    let moved = request.await.unwrap();
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    let closed = match closed_while_held {
        Some(code) => Some(code),
        None => e.closed().await,
    };
    assert_eq!(
        closed,
        Some(4403),
        "the demoted editor's connection stayed open"
    );
    let saved = get_doc(&app, &admin, &id).await;
    assert_eq!(saved["workspace_id"], to.as_str());
    assert!(
        !saved["body"].to_string().contains("권한을 잃은 뒤 문단"),
        "an edit from an editor who lost write access was saved: {saved}"
    );
    owner.close().await;
    r.close().await;
}

/// D-95 조건 4(방 없는 복원, main에도 있던 빈틈): 방 없이 남은 저장 대기 상태(확인받은 편집)가 있는 문서를 복원하면 그
/// 상태를 리비전으로 남기지 않고 바로 복원해, 다른 리비전으로 바뀐 본문이 그 상태를 이어받지 못하게 되어 편집이 사라졌다.
/// 이제 이동처럼 방이 열리지 않게 막고 먼저 리비전으로 남긴다. 작성자는 상태와 함께 저장한 편집자 가운데 아직 쓸 수 있는
/// 사람뿐이다(복원하는 사람은 대신 저장하지 않는다). 남기지 못하면 409이고 상태는 남는다. 남겼으면 기준이 지나 복원은
/// 충돌(409와 그 편집을 담은 최신본)이다. 저장 대기 상태가 없는 문서는 평소처럼 복원한다. 저장 API와 복원의 리비전에는
/// 대신 저장 기록이 없다.
#[tokio::test]
async fn a_restore_without_a_room_saves_a_live_state_left_pending_first() {
    // 권한 재확인(기본 30초)이 시험 중에 돌아 방을 닫지 않게 한다.
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "3600")]))
            .unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "방 없는 복원").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    // 리비전 2(저장 API, 실시간을 켜기 전): 복원하면 본문이 바뀌도록. 저장 대기 상태가 없을 문서도 같이 만든다.
    let mut ids = Vec::new();
    for title in ["방 없는 복원", "대기 상태 없는 문서"] {
        let doc = common::create_doc(&app, &admin, &ws, title).await;
        let id = doc["id"].as_str().unwrap().to_owned();
        let saved = send(
            &app,
            Method::PUT,
            &format!("/api/documents/{id}"),
            Some(&admin),
            Some(
                json!({ "request_id": new_request_id(), "base_revision": 1, "title": title,
                         "body": { "type": "doc", "content": [{ "type": "paragraph",
                             "content": [{ "type": "text", "text": "두 번째 판" }] }] } }),
            ),
        )
        .await;
        assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
        assert_eq!(revision_authors(&owner, &id, 2).await, (admin.id.0, None));
        ids.push(id);
    }
    let (id, plain_id) = (ids[0].clone(), ids[1].clone());
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    // 작성자가 실시간으로 쓰고 확인받은 뒤 독자가 되면 방은 리비전을 남기지 못한 채 상태만 남기고 닫힌다.
    let mut e = connect(addr, Some(&writer), &id, true).await.unwrap();
    e.paragraph("방 없이 남은 복원 전 글").await;
    assert!(e.until(acked).await, "the edit was never acknowledged");
    let role = |role: &'static str| {
        let (app, admin, ws, writer) = (app.clone(), admin.clone(), ws.clone(), writer.clone());
        async move {
            let r = send(
                &app,
                Method::PUT,
                &format!("/api/workspaces/{ws}/members/{}", writer.id.0),
                Some(&admin),
                Some(json!({ "role": role })),
            )
            .await;
            assert!(r.status.is_success(), "{:?}", r.json);
        }
    };
    role("reader").await;
    // 권한 재확인(3600초)을 기다리지 않고 독자가 된 연결을 닫는다.
    e.close().await;
    until_no_room(&state, &id).await;
    until_collab_states(&owner, &id, true, 1).await;
    let restore = |base: i64| {
        let (app, admin, id) = (app.clone(), admin.clone(), id.clone());
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/documents/{id}/revisions/1/restore"),
                Some(&admin),
                Some(json!({ "request_id": new_request_id(), "base_revision": base })),
            )
            .await
        }
    };

    // 쓸 수 있는 편집자가 없다: 복원하는 사람은 대신 저장하지 않으므로 409이고 상태와 리비전은 그대로다.
    let refused = restore(2).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{:?}", refused.json);
    assert_eq!(refused.json["error"], "invalid_state", "{:?}", refused.json);
    assert_eq!(last_revision(&owner, &id).await, (2, admin.id.0));
    assert_eq!(collab_states(&owner, &id, true).await, 1);
    assert!(!state.collab.has_room(fvoci_server::ids::DocumentId(
        uuid::Uuid::parse_str(&id).unwrap()
    )));

    // 편집자가 다시 쓸 수 있으면 그 사람이 작성자로 먼저 리비전을 남기고(대신 저장 아님), 복원은 기준이 지나 충돌이다.
    role("writer").await;
    let conflict = restore(2).await;
    assert_eq!(conflict.status, StatusCode::CONFLICT, "{:?}", conflict.json);
    assert!(
        conflict.json["current"]["body"]
            .to_string()
            .contains("방 없이 남은 복원 전 글"),
        "{:?}",
        conflict.json
    );
    assert_eq!(conflict.json["current"]["revision"], 3);
    assert_eq!(revision_authors(&owner, &id, 3).await, (writer.id.0, None));
    assert_eq!(collab_states(&owner, &id, true).await, 0);

    // 지금 기준으로 다시 하면 복원한다. 복원 리비전에는 대신 저장 기록이 없다.
    let restored = restore(3).await;
    assert_eq!(restored.status, StatusCode::OK, "{:?}", restored.json);
    assert_eq!(revision_authors(&owner, &id, 4).await, (admin.id.0, None));

    // 저장 대기 상태가 없는 문서는 평소처럼 복원한다.
    let plain_restored = restore_rev1(&app, &admin, &plain_id, &json!(2)).await;
    assert_eq!(
        plain_restored.status,
        StatusCode::OK,
        "{:?}",
        plain_restored.json
    );
    assert_eq!(collab_states(&owner, &plain_id, false).await, 0);
    owner.close().await;
}

/// D-95: 마지막으로 고친 사람이 쓸 수 없어 연결된 다른 작성자가 대신 리비전을 남기면, 그 리비전에 실제로 고친 사람을 따로
/// 적는다(작성자는 남긴 사람 그대로). 기록 API는 그 이름을 보이고, 이름을 볼 수 없는 사람에게는 null이다.
#[tokio::test]
async fn a_connected_writer_saving_for_an_editor_is_recorded_as_a_stand_in() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "1")])).unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "연결된 작성자의 대신 저장").await;
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let (first, _) = add_member(&app, &state, &admin, &ws, "first-editor", "writer").await;
    let (second, _) = add_member(&app, &state, &admin, &ws, "second-writer", "writer").await;
    let doc = common::create_doc(&app, &admin, &ws, "대신 저장 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let mut a = connect(addr, Some(&first), &id, true).await.unwrap();
    let mut b = connect(addr, Some(&second), &id, true).await.unwrap();
    a.paragraph("처음 편집자의 문단").await;
    assert!(b.until(|c| c.text().contains("처음 편집자의 문단")).await);
    let demoted = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/members/{}", first.id.0),
        Some(&admin),
        Some(json!({ "role": "reader" })),
    )
    .await;
    assert!(demoted.status.is_success(), "{:?}", demoted.json);
    assert_eq!(a.closed().await, Some(4403));
    // 반출 직전처럼 방에 지금 리비전을 남기게 한다: 고친 사람은 쓸 수 없고 연결된 다른 작성자가 남긴다.
    let doc_id = fvoci_server::ids::DocumentId(uuid::Uuid::parse_str(&id).unwrap());
    state
        .collab
        .snapshot_documents(&std::collections::HashSet::from([doc_id]))
        .await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let (last, author) = last_revision(&owner, &id).await;
    assert_eq!(author, second.id.0, "the connected writer did not save");
    assert_eq!(
        revision_authors(&owner, &id, last).await.1,
        Some(vec![first.id.0]),
        "the stand-in revision does not name the actual editor"
    );
    let history = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}/revisions"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(history[0]["revision"], last);
    assert_eq!(history[0]["author_name"], "second-writer");
    assert_eq!(history[0]["edited_by_names"], json!(["first-editor"]));
    // 고친 사람이 워크스페이스를 떠나 함께 속한 곳이 없으면 그 이름은 보이지 않는다(작성자 이름과 같은 규칙).
    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", first.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(removed.status.is_success(), "{:?}", removed.json);
    for path in [
        format!("/api/documents/{id}/revisions"),
        format!("/api/documents/{id}/revisions/{last}"),
    ] {
        let seen = send(&app, Method::GET, &path, Some(&second), None)
            .await
            .json;
        let row = if seen.is_array() { &seen[0] } else { &seen };
        assert_eq!(row["author_name"], "second-writer", "{seen}");
        assert_eq!(row["edited_by_names"], json!([null]), "{seen}");
    }
    owner.close().await;
    b.close().await;
}

/// #64 반박 검증: 방 없이 남은 저장 대기 상태(확인받은 편집)를 다시 연 방(독자 연결이면 충분하다)이 이어받으면 그 편집을
/// 아직 리비전이 아닌 것으로 보지 않아, 방 안의 복원·이동이 체크포인트를 건너뛰고 그 편집을 잃었다(방 없는 길만 고쳤던 것).
/// 이제 방이 이어받은 상태가 본문과 다르면 리비전으로 남길 것으로 본다. 방 안 복원의 체크포인트 작성자는 main과 같다(고친
/// 사람, 그다음 연결된 작성자. 연결된 복원하는 사람도 든다): 복원하는 사람이 대신 남기면 실제 편집자를 `edited_by`에 적는다.
#[tokio::test]
async fn a_room_reopened_on_a_pending_state_saves_it_before_a_restore_or_move() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "3600")]))
            .unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let from = create_workspace(&app, &admin, "다시 연 방 켠 곳").await;
    let to = create_workspace(&app, &admin, "다시 연 방 끈 곳").await;
    let (writer, _) = add_member(&app, &state, &admin, &from, "writer", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &from, "reader", "reader").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let mut docs = Vec::new();
    for title in [
        "다시 연 방 복원",
        "다시 연 방 대신 저장 복원",
        "다시 연 방 이동",
    ] {
        let doc = common::create_doc(&app, &admin, &from, title).await;
        let id = doc["id"].as_str().unwrap().to_owned();
        // 리비전 2(저장 API, 실시간을 켜기 전): 복원하면 본문이 바뀌도록.
        let saved = send(
            &app,
            Method::PUT,
            &format!("/api/documents/{id}"),
            Some(&admin),
            Some(
                json!({ "request_id": new_request_id(), "base_revision": 1, "title": title,
                         "body": { "type": "doc", "content": [{ "type": "paragraph",
                             "content": [{ "type": "text", "text": "두 번째 판" }] }] } }),
            ),
        )
        .await;
        assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
        docs.push(id);
    }
    let (a, c, b) = (docs[0].clone(), docs[1].clone(), docs[2].clone());
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);
    let role = |role: &'static str| {
        let (app, admin, from, writer) = (app.clone(), admin.clone(), from.clone(), writer.clone());
        async move {
            let r = send(
                &app,
                Method::PUT,
                &format!("/api/workspaces/{from}/members/{}", writer.id.0),
                Some(&admin),
                Some(json!({ "role": role })),
            )
            .await;
            assert!(r.status.is_success(), "{:?}", r.json);
        }
    };
    // 세 문서 모두 작성자가 실시간으로 쓰고 확인받은 뒤 독자가 되어, 방이 리비전을 남기지 못한 채 상태만 남기고 닫힌다.
    let mut editors = Vec::new();
    for (id, text) in [
        (&a, "복원 전 확인받은 글"),
        (&c, "대신 저장될 확인받은 글"),
        (&b, "이동 전 확인받은 글"),
    ] {
        let mut e = connect(addr, Some(&writer), id, true).await.unwrap();
        e.paragraph(text).await;
        assert!(e.until(acked).await, "the edit was never acknowledged");
        editors.push(e);
    }
    role("reader").await;
    // 권한 재확인(3600초)을 기다리지 않고 독자가 된 연결을 닫는다.
    for e in editors {
        e.close().await;
    }
    for id in [&a, &c, &b] {
        // 방이 닫히며 남긴 것: 쓸 수 있는 편집자가 없어 저장 대기 상태다.
        until_no_room(&state, id).await;
        until_collab_states(&owner, id, true, 1).await;
    }
    let restore = |id: &str, base: i64| {
        let (app, admin, id) = (app.clone(), admin.clone(), id.to_owned());
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/documents/{id}/revisions/1/restore"),
                Some(&admin),
                Some(json!({ "request_id": new_request_id(), "base_revision": base })),
            )
            .await
        }
    };

    // 복원하는 관리자가 쓰기 연결로 방을 다시 열었다. 고친 사람은 쓸 수 없으므로 체크포인트는 연결된 작성자인 복원하는
    // 사람이 대신 남기고(main과 같은 작성자 순서) 실제 편집자를 따로 적는다(D-95). 기준이 지나 복원은 충돌이다.
    let restorer = connect(addr, Some(&admin), &c, true).await.unwrap();
    let stood_in = restore(&c, 2).await;
    assert_eq!(stood_in.status, StatusCode::CONFLICT, "{:?}", stood_in.json);
    assert!(
        stood_in.json["current"]["body"]
            .to_string()
            .contains("대신 저장될 확인받은 글"),
        "the acknowledged edit was restored away: {:?}",
        stood_in.json
    );
    assert_eq!(
        revision_authors(&owner, &c, 3).await,
        (admin.id.0, Some(vec![writer.id.0])),
        "the connected restorer's stand-in revision does not name the actual editor"
    );
    assert_eq!(collab_states(&owner, &c, true).await, 0);
    restorer.close().await;

    // 고친 사람이 다시 쓸 수 있고 독자만 방을 열었다: 체크포인트가 그 사람의 리비전으로 남기고 복원은 충돌이다.
    role("writer").await;
    let r = connect(addr, Some(&reader), &a, true).await.unwrap();
    let conflict = restore(&a, 2).await;
    assert_eq!(conflict.status, StatusCode::CONFLICT, "{:?}", conflict.json);
    assert!(
        conflict.json["current"]["body"]
            .to_string()
            .contains("복원 전 확인받은 글"),
        "the acknowledged edit was restored away: {:?}",
        conflict.json
    );
    assert_eq!(revision_authors(&owner, &a, 3).await, (writer.id.0, None));
    r.close().await;

    // 독자만 방을 연 문서를 실시간이 꺼진 곳으로 옮긴다: 옮기기 전에 그 상태가 리비전으로 남는다.
    let r = connect(addr, Some(&reader), &b, true).await.unwrap();
    let moved = move_to(&app, &admin, &b, &to).await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    let after = get_doc(&app, &admin, &b).await;
    assert_eq!(after["workspace_id"], to.as_str());
    assert!(
        after["body"].to_string().contains("이동 전 확인받은 글"),
        "the acknowledged edit is not in the moved document: {after}"
    );
    drop(r);
    owner.close().await;
}

/// #64 반박 검증: 복원도 잠금을 끝없이 기다리지 않는다(이동과 같은 한도, 409 `invalid_state`). 방 없는 복원은 그동안 방
/// 열기를 막으므로 더 중요하다. 막음은 답한 뒤 풀린다.
#[tokio::test]
async fn a_restore_held_past_the_lock_limit_is_refused() {
    let (state, app, addr, admin, _writer, _ws, id) = live_restore_setup("잠금 한도 복원").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let mut hold = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT 1 FROM fvoci.documents WHERE id = $1::uuid FOR UPDATE")
        .bind(&id)
        .execute(&mut *hold)
        .await
        .unwrap();
    let request = tokio::spawn({
        let (app, admin, id) = (app.clone(), admin.clone(), id.clone());
        async move { restore_rev1(&app, &admin, &id, &json!(1)).await }
    });
    until_waiting_on_lock("app_restore_document_revision", holder).await;
    let answered = finishes_within(&request, Duration::from_secs(30)).await;
    hold.rollback().await.unwrap();
    assert!(
        answered,
        "the restore kept waiting on the lock past the limit"
    );
    let refused = request.await.unwrap();
    assert_eq!(refused.status, StatusCode::CONFLICT, "{:?}", refused.json);
    assert_eq!(refused.json["error"], "invalid_state", "{:?}", refused.json);
    let doc = fvoci_server::ids::DocumentId(uuid::Uuid::parse_str(&id).unwrap());
    assert!(!state.collab.has_room(doc));
    let c = connect(addr, Some(&admin), &id, true).await.unwrap();
    c.close().await;
    owner.close().await;
}

/// 방 안 이동이 잠금을 기다리는 동안 작성자의 쓰기 권한을 거둔 결과.
struct RevokedDuringMove {
    /// 이동 요청의 답.
    reply: common::Reply,
    /// 이동 뒤 문서(옮겼으면 새 자리).
    doc: serde_json::Value,
    /// 거둔 뒤 보낸 편집이 확인됐는지.
    after_acked: bool,
    /// 거둔 뒤 보낸 편집이 다른 연결(독자)에 전해졌는지(방이 받아들였는지).
    after_relayed: bool,
    /// 작성자 연결의 닫힘 코드(열린 채면 `None`).
    closed: Option<u16>,
    /// 원래 워크스페이스.
    from: String,
}

/// 잠금을 기다리는 방 안 이동 중에 작성자의 쓰기 권한을 거둔다(`demote`: 독자로, 아니면 워크스페이스에서 내보냄). 거두기는
/// 이동 트랜잭션과 잠금을 나누지 않아(0081의 워크스페이스 행 `FOR NO KEY UPDATE`·멤버 행 잠금도 이동이 기다리는 동안에는
/// 이동이 쥔 잠금과 겹치지 않는다) 이동이 기다리는 동안 커밋된다. 거두기 전에 보낸 편집은 확인을 받은 뒤다.
async fn revoke_while_a_move_waits(
    recheck_secs: &str,
    label: &str,
    demote: bool,
) -> RevokedDuringMove {
    let (writer, (_state, app, _addr, admin, from, to, id, mut e, mut r)) =
        live_move_race_setup_rechecking(label, recheck_secs).await;
    assert!(
        e.until(acked).await,
        "the edit before the wait was never acknowledged"
    );
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let (hold, holder) = hold_tree(&owner, &to).await;
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move { move_to(&app, &admin, &id, &to).await }
    });
    until_waiting_on_lock("app_lock_move", holder).await;
    let path = format!("/api/workspaces/{from}/members/{}", writer.id.0);
    let revoked = if demote {
        send(
            &app,
            Method::PUT,
            &path,
            Some(&admin),
            Some(json!({ "role": "reader" })),
        )
        .await
    } else {
        send(&app, Method::DELETE, &path, Some(&admin), None).await
    };
    assert!(revoked.status.is_success(), "{:?}", revoked.json);
    assert!(
        !request.is_finished(),
        "the revocation did not commit while the move waited"
    );
    e.paragraph("거둔 뒤 보낸 문단").await;
    let closed_early = e.try_read_by_server().await;
    hold.rollback().await.unwrap();
    let reply = request.await.unwrap();
    let after_relayed = r.until(|c| c.text().contains("거둔 뒤 보낸 문단")).await;
    let closed = match closed_early {
        Some(code) => Some(code),
        None if reply.status == StatusCode::OK => e.closed().await,
        None => None,
    };
    let doc = get_doc(&app, &admin, &id).await;
    let after_acked = acked(&e);
    owner.close().await;
    r.close().await;
    RevokedDuringMove {
        reply,
        doc,
        after_acked,
        after_relayed,
        closed,
        from,
    }
}

/// W1b(사용자 요구 2b·2c, 방 안 이동, 재확인 간격 안): 이동이 잠금을 기다리는 동안 작성자를 독자로 바꾸고 재확인 간격(이
/// 시험은 3600초) 안에 편집을 보낸다. 강등은 이동이 기다리는 동안 커밋되고 이동은 그대로 옮긴다. 강등 전에 확인받은 편집은
/// 옮긴 문서에 남는다. 강등 뒤 보낸 편집은 다음 재확인 전이라 받아들여 옮긴 문서의 리비전에 들고 확인된다(연결은 4409):
/// 문서화된 권한 철회 지연(재확인 간격 안의 편집은 들어간다)과 같다. 간격이 지나면 대기 중에도 닫힌다(아래 시험).
#[tokio::test]
async fn an_edit_within_the_recheck_interval_after_a_demotion_during_a_move_is_kept() {
    let out = revoke_while_a_move_waits("3600", "기다리는 이동 중 강등", true).await;
    assert_eq!(out.reply.status, StatusCode::OK, "{:?}", out.reply.json);
    let body = out.doc["body"].to_string();
    assert!(
        body.contains("체크포인트 전 문단"),
        "an acknowledged edit was lost: {body}"
    );
    // 확인받은 편집은 어떤 경우에도 옮긴 문서에 있다.
    assert!(
        !out.after_acked || body.contains("거둔 뒤 보낸 문단"),
        "{body}"
    );
    // 재확인 간격 안: 강등 뒤 편집이 받아들여지고 리비전·확인까지 간다.
    assert!(out.after_relayed && body.contains("거둔 뒤 보낸 문단") && out.after_acked);
    assert_eq!(out.closed, Some(4409));
}

/// 재확인 간격 1초인 방에서 이동이 잠금을 기다리는 동안 작성자를 독자로 바꾼다. `later_lock`이면 이동이 첫 잠금(새 자리
/// 나무)을 지난 뒤 문서 행 잠금(`FOR UPDATE`)에서 기다리게 한다(재확인 뒤 남은 잠금에서 오래 기다리는 경우). 강등 응답
/// 뒤 연결이 닫히기까지 걸린 시간(측정값)을 돌려준다.
async fn demote_while_a_rechecking_move_waits(label: &str, later_lock: bool) -> Duration {
    let (writer, (_state, app, _addr, admin, from, to, id, mut e, r)) =
        live_move_race_setup_rechecking(label, "1").await;
    assert!(
        e.until(acked).await,
        "the edit before the wait was never acknowledged"
    );
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let (tree, tree_holder) = hold_tree(&owner, &to).await;
    let request = tokio::spawn({
        let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
        async move { move_to(&app, &admin, &id, &to).await }
    });
    until_waiting_on_lock("app_lock_move", tree_holder).await;
    let row = if later_lock {
        // 체크포인트는 이미 끝났다. 문서 행을 쥔 뒤 나무 잠금을 풀어 이동이 다음 잠금에서 기다리게 한다.
        let mut row = owner.begin().await.unwrap();
        let row_holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *row)
            .await
            .unwrap();
        sqlx::query("SELECT 1 FROM fvoci.documents WHERE id = $1::uuid FOR UPDATE")
            .bind(&id)
            .execute(&mut *row)
            .await
            .unwrap();
        tree.rollback().await.unwrap();
        until_waiting_on_lock("FROM documents WHERE id = $1 FOR UPDATE", row_holder).await;
        row
    } else {
        tree
    };
    let demoted = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/members/{}", writer.id.0),
        Some(&admin),
        Some(json!({ "role": "reader" })),
    )
    .await;
    assert!(demoted.status.is_success(), "{:?}", demoted.json);
    let demoted_at = tokio::time::Instant::now();
    // 이동이 아직 기다리는 동안 다음 재확인이 연결을 닫는다(고치기 전: 닫히지 않고 이동이 끝나지 않음).
    let closed_while_waiting = e.closed().await;
    let close_after = demoted_at.elapsed();
    let still_waiting = !request.is_finished();
    e.try_paragraph("재확인 뒤 보낸 문단").await;
    let _closed = e.try_read_by_server().await;
    row.rollback().await.unwrap();
    let moved = request.await.unwrap();
    assert!(
        still_waiting,
        "the move ended before the recheck closed the editor"
    );
    assert_eq!(
        closed_while_waiting,
        Some(4403),
        "the demoted editor's connection stayed open while the move waited"
    );
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    let saved = get_doc(&app, &admin, &id).await;
    assert_eq!(saved["workspace_id"], to.as_str());
    let body = saved["body"].to_string();
    assert!(
        body.contains("체크포인트 전 문단"),
        "an acknowledged edit was lost: {body}"
    );
    assert!(
        !body.contains("재확인 뒤 보낸 문단"),
        "an edit sent after the recheck closed the editor was saved: {body}"
    );
    owner.close().await;
    r.close().await;
    eprintln!(
        "MEASURED {label}: demotion response -> 4403 close {close_after:?} (recheck interval 1s)"
    );
    close_after
}

/// W1b(coordinator 판단 B): 이동 트랜잭션이 잠금을 기다리는 동안에도 방은 재확인 간격마다 세션·옛 자리 권한을 다시 본다.
/// 전에는 기다리는 동안 재확인이 멈춰, 간격이 지나도 강등된 작성자의 편집을 옛 권한으로 받아 옮긴 문서에 넣었다(창이 잠금
/// 대기 전체로 늘어남). 이제 다음 재확인(간격 1초)에서 그 연결을 4403으로 닫고, 그 뒤 편집은 들어가지 않는다. 강등 전에
/// 확인받은 편집은 옮긴 문서에 남는다.
#[tokio::test]
async fn an_editor_demoted_while_a_move_waits_is_closed_at_the_next_recheck() {
    let took = demote_while_a_rechecking_move_waits("대기 중 재확인 강등", false).await;
    // 간격(1초) + 재확인 한 번(연결마다 짧은 읽기 둘)과 시험 쪽 지연의 여유.
    assert!(took < Duration::from_secs(4), "{took:?}");
}

/// W1b(추가 지시 2-1b·c): 이동이 마지막 재확인 뒤 남은 잠금(문서 행)에서 오래 기다려도 재확인은 계속 돈다.
#[tokio::test]
async fn an_editor_demoted_while_a_move_waits_on_a_later_lock_is_closed_at_the_next_recheck() {
    let took = demote_while_a_rechecking_move_waits("남은 잠금 대기 중 강등", true).await;
    assert!(took < Duration::from_secs(4), "{took:?}");
}

/// W1b(사용자 요구 2b·2c, 방 안 이동): 같은 것을 워크스페이스에서 내보내기로. 내보내면 이동의 접근 변화가 사용자가 확인한
/// 미리보기와 달라져 이동은 일어나지 않는다(409, 새 미리보기). 확인받은 편집은 원래 자리에 남는다. 내보낸 뒤 보낸 편집은
/// 이동과 상관없이 평소처럼 방이 받는다(다음 권한 재확인 때까지, 문서화된 권한 철회 지연).
#[tokio::test]
async fn write_access_removed_while_a_move_waits() {
    let out = revoke_while_a_move_waits("3600", "기다리는 이동 중 내보냄", false).await;
    assert_eq!(
        out.reply.status,
        StatusCode::CONFLICT,
        "{:?}",
        out.reply.json
    );
    assert!(out.reply.json["loses"].is_array(), "{:?}", out.reply.json);
    let body = out.doc["body"].to_string();
    assert!(
        body.contains("체크포인트 전 문단"),
        "an acknowledged edit was lost: {body}"
    );
    assert_eq!(
        out.doc["workspace_id"],
        out.from.as_str(),
        "the document moved"
    );
    // 지금 동작: 이동이 없었으므로 방이 그대로 받는다(연결은 열린 채).
    assert!(out.after_relayed);
    assert_eq!(out.closed, None);
}

/// W1b(사용자 요구 5): 마지막 연결이 나가 방이 닫히며 마지막 리비전을 남기는 중에(방은 아직 목록에 있고 대기열이 열려 있다)
/// 복원·이동이 오면, 그 명령은 닫히는 방의 대기열에 들어갔다가 방이 목록에서 빠진 뒤 다시 찾아 방 없는 길로 간다. 방 없는
/// 길은 닫히는 방의 저장이 끝난 뒤에야 돈다: 확인받은 편집은 리비전에 남고 복원은 기준이 지나 충돌, 이동은 그 편집과 함께
/// 옮긴다. 방의 마지막 리비전 남기기를 문서 행 잠금으로 붙잡는다.
#[tokio::test]
async fn a_restore_or_move_racing_a_closing_room_keeps_its_edits() {
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_COLLAB_RECHECK_SECS", "3600")]))
            .unwrap();
    let app = common::app(&state);
    let addr = serve(app.clone()).await;
    let admin = login(&state, "admin").await;
    let from = create_workspace(&app, &admin, "닫히는 방 경합 켠 곳").await;
    let to = create_workspace(&app, &admin, "닫히는 방 경합 끈 곳").await;
    let (writer, _) = add_member(&app, &state, &admin, &from, "writer", "writer").await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let mut docs = Vec::new();
    for title in ["닫히는 방과 복원", "닫히는 방과 이동"] {
        let doc = common::create_doc(&app, &admin, &from, title).await;
        let id = doc["id"].as_str().unwrap().to_owned();
        let saved = send(
            &app,
            Method::PUT,
            &format!("/api/documents/{id}"),
            Some(&admin),
            Some(
                json!({ "request_id": new_request_id(), "base_revision": 1, "title": title,
                         "body": { "type": "doc", "content": [{ "type": "paragraph",
                             "content": [{ "type": "text", "text": "두 번째 판" }] }] } }),
            ),
        )
        .await;
        assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
        docs.push(id);
    }
    let on = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{from}/realtime"),
        Some(&admin),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(on.status, StatusCode::NO_CONTENT);

    for (n, id) in docs.iter().enumerate() {
        let text = if n == 0 {
            "닫히는 방의 복원 전 글"
        } else {
            "닫히는 방의 이동 전 글"
        };
        let mut e = connect(addr, Some(&writer), id, true).await.unwrap();
        e.paragraph(text).await;
        assert!(e.until(acked).await, "the edit was never acknowledged");
        // 문서 행을 잠가 둔 채 마지막 연결이 나간다: 방은 마지막 리비전을 남기려고 그 잠금을 기다린다(목록에 남아 있다).
        let mut hold = owner.begin().await.unwrap();
        let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *hold)
            .await
            .unwrap();
        sqlx::query("SELECT 1 FROM fvoci.documents WHERE id = $1::uuid FOR UPDATE")
            .bind(id)
            .execute(&mut *hold)
            .await
            .unwrap();
        e.close().await;
        until_waiting_on_lock("FROM documents WHERE id = $1 FOR UPDATE", holder).await;
        let doc = fvoci_server::ids::DocumentId(uuid::Uuid::parse_str(id).unwrap());
        assert!(
            state.collab.has_room(doc),
            "the closing room left the list early"
        );
        let request = tokio::spawn({
            let (app, admin, id, to) = (app.clone(), admin.clone(), id.clone(), to.clone());
            async move {
                if n == 0 {
                    restore_rev1(&app, &admin, &id, &json!(2)).await
                } else {
                    move_to(&app, &admin, &id, &to).await
                }
            }
        });
        // 그 요청이 닫히는 방에 닿아 기다리게 둔 뒤 잠금을 푼다.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !request.is_finished(),
            "the request did not wait for the closing room"
        );
        hold.rollback().await.unwrap();
        let reply = request.await.unwrap();
        let current = get_doc(&app, &admin, id).await;
        assert!(
            current["body"].to_string().contains(text),
            "the acknowledged edit was lost: {current}"
        );
        if n == 0 {
            // 닫히는 방이 남긴 리비전 때문에 기준(2)이 지났다: 복원하지 않고 충돌(최신본에 그 편집).
            assert_eq!(reply.status, StatusCode::CONFLICT, "{:?}", reply.json);
            assert_eq!(current["revision"], 3);
            assert_eq!(revision_authors(&owner, id, 3).await, (writer.id.0, None));
        } else {
            assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
            assert_eq!(current["workspace_id"], to.as_str());
        }
        until_no_room(&state, id).await;
    }
    owner.close().await;
}
