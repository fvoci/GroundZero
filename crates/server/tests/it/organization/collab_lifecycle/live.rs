//! 실제 WebSocket으로 동기화하고 보낸 편집을 포함하는 저장 확인을 기다린다.

use crate::common::{self, TestUser};
use futures_util::{SinkExt, StreamExt};
use fvoci_server::collab::convert::{self, FRAGMENT};
use fvoci_server::collab::room::MSG_PERSISTED;
use serde_json::Value;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::{Message as Frame, client::IntoClientRequest};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use yrs::sync::{Message, MessageReader, SyncMessage};
use yrs::types::xml::XmlFragment;
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, Snapshot, Transact, Update};

pub(super) struct Client {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    doc: Doc,
}

impl Client {
    pub(super) async fn connect(addr: SocketAddr, document: &str, user: &TestUser) -> Self {
        let mut request = format!(
            "ws://{addr}/api/documents/{document}/collab?write=1&expected_user={}",
            user.id.0
        )
        .into_client_request()
        .unwrap();
        request
            .headers_mut()
            .insert("cookie", user.cookie.parse().unwrap());
        request
            .headers_mut()
            .insert("origin", common::PUBLIC_URL.parse().unwrap());
        let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        let mut client = Self {
            ws,
            doc: Doc::new(),
        };
        let state = client.doc.transact().state_vector();
        assert!(
            client
                .send(Message::Sync(SyncMessage::SyncStep1(state)))
                .await
        );
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                for message in client
                    .messages()
                    .await
                    .expect("connection closed during sync")
                {
                    if matches!(message, Message::Sync(SyncMessage::SyncStep2(_))) {
                        client.apply(message).await;
                        return;
                    }
                    client.apply(message).await;
                }
            }
        })
        .await
        .expect("sync deadline");
        client
    }

    async fn send(&mut self, message: Message) -> bool {
        self.ws
            .send(Frame::Binary(message.encode_v1().into()))
            .await
            .is_ok()
    }

    async fn messages(&mut self) -> Option<Vec<Message>> {
        loop {
            match self.ws.next().await {
                Some(Ok(Frame::Binary(bytes))) => {
                    let mut decoder = DecoderV1::from(bytes.as_ref());
                    return Some(
                        MessageReader::new(&mut decoder)
                            .collect::<Result<_, _>>()
                            .unwrap(),
                    );
                }
                Some(Ok(Frame::Close(_))) | Some(Err(_)) | None => return None,
                Some(Ok(_)) => {}
            }
        }
    }

    async fn apply(&mut self, message: Message) {
        match message {
            Message::Sync(SyncMessage::SyncStep1(state)) => {
                let update = self.doc.transact().encode_state_as_update_v1(&state);
                assert!(
                    self.send(Message::Sync(SyncMessage::SyncStep2(update)))
                        .await
                );
            }
            Message::Sync(SyncMessage::SyncStep2(bytes) | SyncMessage::Update(bytes)) => {
                self.doc
                    .transact_mut()
                    .apply_update(Update::decode_v1(&bytes).unwrap())
                    .unwrap();
            }
            _ => {}
        }
    }

    pub(super) fn body(&self) -> Value {
        let fragment = self.doc.get_or_insert_xml_fragment(FRAGMENT);
        convert::read_doc(&self.doc.transact(), &fragment)
    }

    pub(super) async fn edit_and_wait(&mut self, text: &str) -> bool {
        let before = self.doc.transact().state_vector();
        let fragment = self.doc.get_or_insert_xml_fragment(FRAGMENT);
        {
            let mut tx = self.doc.transact_mut();
            let len = fragment.len(&tx);
            fragment.remove_range(&mut tx, 0, len);
            convert::write_doc(&mut tx, &fragment, &common::doc_body(text));
        }
        let update = self.doc.transact().encode_state_as_update_v1(&before);
        if !self.send(Message::Sync(SyncMessage::Update(update))).await {
            return false;
        }
        let mine = self.doc.client_id();
        let sent = self.doc.transact().state_vector().get(&mine);
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(messages) = self.messages().await {
                for message in messages {
                    if let Message::Custom(MSG_PERSISTED, bytes) = &message
                        && Snapshot::decode_v1(bytes).unwrap().state_map.get(&mine) >= sent
                    {
                        return true;
                    }
                    self.apply(message).await;
                }
            }
            false
        })
        .await
        .expect("edit was neither acknowledged nor refused before deadline")
    }

    pub(super) async fn close(&mut self) {
        let _closed = self.ws.close(None).await;
    }
}
