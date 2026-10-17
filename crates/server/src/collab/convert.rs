//! 문서 본문(ProseMirror JSON) ↔ Yjs XmlFragment 변환(D-70). 브라우저의 @tiptap/y-tiptap(y-prosemirror)과 같은
//! 표현을 쓴다: 조각 이름 `default`, 노드는 같은 이름의 XmlElement, 노드 속성은 값이 null이 아닌 것만 요소 속성,
//! 이어진 글자 노드는 XmlText 하나로 모으고 표시(mark)는 글자 서식 속성 `{표시 이름: 표시 속성(없으면 {})}`.
//! 기준 파일: `contracts/collab`(웹 테스트 `apps/web/tests/collab-fixture.test.ts`가 만들고 확인한다).

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Map, Value};
use yrs::types::Attrs;
use yrs::types::text::YChange;
use yrs::types::xml::{Xml, XmlElementPrelim, XmlFragment, XmlOut, XmlTextPrelim};
use yrs::{Any, Number, Out, ReadTxn, Text, TransactionMut, XmlFragmentRef, XmlTextRef};

/// 브라우저 편집기(Collaboration 확장)가 쓰는 조각 이름.
pub const FRAGMENT: &str = "default";

/// JSON 본문(`{"type":"doc","content":[...]}`)을 비어 있는 조각에 쓴다.
pub fn write_doc(txn: &mut TransactionMut, fragment: &XmlFragmentRef, doc: &Value) {
    write_children(txn, fragment, content(doc));
}

/// 조각을 JSON 본문으로 읽는다.
pub fn read_doc<T: ReadTxn>(txn: &T, fragment: &XmlFragmentRef) -> Value {
    let mut doc = Map::new();
    doc.insert("type".into(), Value::from("doc"));
    let children = read_children(txn, fragment.children(txn));
    if !children.is_empty() {
        doc.insert("content".into(), Value::Array(children));
    }
    Value::Object(doc)
}

fn content(node: &Value) -> &[Value] {
    node.get("content")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn write_children<F: XmlFragment>(txn: &mut TransactionMut, parent: &F, nodes: &[Value]) {
    let mut i = 0;
    while i < nodes.len() {
        if nodes[i].get("type").and_then(Value::as_str) == Some("text") {
            // 이어진 글자 노드는 XmlText 하나. 서식은 매번 명시해 왼쪽 서식을 이어받지 않게 한다.
            let text: XmlTextRef = parent.push_back(txn, XmlTextPrelim::new(""));
            while let Some(node) = nodes
                .get(i)
                .filter(|n| n.get("type").and_then(Value::as_str) == Some("text"))
            {
                let chunk = node.get("text").and_then(Value::as_str).unwrap_or_default();
                let at = text.len(txn);
                // 빈 서식으로 넣어 왼쪽 서식을 끊고, 표시는 하나씩 입힌다. 같은 자리에 나중에 입힌 서식이 앞에 놓이므로 거꾸로 입혀 JSON 순서를 지킨다.
                text.insert_with_attributes(txn, at, chunk, Attrs::new());
                let len = text.len(txn) - at;
                for (name, value) in marks(node).into_iter().rev() {
                    text.format(txn, at, len, Attrs::from([(name, value)]));
                }
                i += 1;
            }
            continue;
        }
        let node = &nodes[i];
        let tag = node
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("paragraph");
        let element = parent.push_back(txn, XmlElementPrelim::empty(tag));
        if let Some(attrs) = node.get("attrs").and_then(Value::as_object) {
            for (key, value) in attrs {
                if !value.is_null() {
                    element.insert_attribute(txn, key.as_str(), to_any(value));
                }
            }
        }
        write_children(txn, &element, content(node));
        i += 1;
    }
}

fn marks(node: &Value) -> Vec<(Arc<str>, Any)> {
    let mut attrs = Vec::new();
    for mark in node
        .get("marks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = mark.get("type").and_then(Value::as_str) else {
            continue;
        };
        let value = mark
            .get("attrs")
            .filter(|a| a.is_object())
            .map_or_else(|| Any::from(HashMap::<String, Any>::new()), to_any);
        attrs.push((Arc::from(name), value));
    }
    attrs
}

fn read_children<T: ReadTxn>(txn: &T, children: impl Iterator<Item = XmlOut>) -> Vec<Value> {
    let mut out = Vec::new();
    for child in children {
        match child {
            XmlOut::Element(element) => {
                let mut node = Map::new();
                node.insert("type".into(), Value::from(element.tag().as_ref()));
                let mut attrs = Map::new();
                for (key, value) in element.attributes(txn) {
                    if let Out::Any(any) = value {
                        let value = from_any(&any);
                        if !value.is_null() {
                            attrs.insert(key.to_owned(), value);
                        }
                    }
                }
                if !attrs.is_empty() {
                    node.insert("attrs".into(), Value::Object(attrs));
                }
                let children = read_children(txn, element.children(txn));
                if !children.is_empty() {
                    node.insert("content".into(), Value::Array(children));
                }
                out.push(Value::Object(node));
            }
            XmlOut::Text(text) => {
                for diff in text.diff(txn, YChange::identity) {
                    let Out::Any(Any::String(chunk)) = diff.insert else {
                        continue;
                    };
                    let mut node = Map::new();
                    node.insert("type".into(), Value::from("text"));
                    node.insert("text".into(), Value::from(chunk.as_ref()));
                    let mut marks: Vec<(String, Value)> = diff
                        .attributes
                        .into_iter()
                        .flat_map(|attrs| attrs.into_iter())
                        .filter(|(_, v)| !matches!(v, Any::Null | Any::Undefined))
                        .map(|(k, v)| (k.to_string(), from_any(&v)))
                        .collect();
                    // 서식 속성의 순서는 해시 순서다. ProseMirror는 스키마 순서로 다시 정렬하므로 이름순으로 고정한다.
                    marks.sort_by(|a, b| a.0.cmp(&b.0));
                    if !marks.is_empty() {
                        let marks = marks
                            .into_iter()
                            .map(|(name, attrs)| {
                                let mut mark = Map::new();
                                mark.insert("type".into(), Value::from(name));
                                mark.insert("attrs".into(), attrs);
                                Value::Object(mark)
                            })
                            .collect();
                        node.insert("marks".into(), Value::Array(marks));
                    }
                    out.push(Value::Object(node));
                }
            }
            XmlOut::Fragment(fragment) => out.extend(read_children(txn, fragment.children(txn))),
        }
    }
    out
}

fn to_any(value: &Value) -> Any {
    match value {
        Value::Null => Any::Null,
        Value::Bool(b) => Any::from(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Any::Number(i.into()),
            None => Any::Number(n.as_f64().unwrap_or_default().into()),
        },
        Value::String(s) => Any::from(s.as_str()),
        Value::Array(items) => Any::from(items.iter().map(to_any).collect::<Vec<_>>()),
        Value::Object(map) => Any::from(
            map.iter()
                .map(|(k, v)| (k.clone(), to_any(v)))
                .collect::<HashMap<String, Any>>(),
        ),
    }
}

fn from_any(any: &Any) -> Value {
    match any {
        Any::Null | Any::Undefined | Any::Buffer(_) => Value::Null,
        Any::Bool(b) => Value::Bool(*b),
        Any::Number(Number::Int(i)) => Value::from(*i),
        Any::Number(Number::Float(f)) => {
            // 브라우저가 정수를 실수로 보낸 경우(예: 2.0)도 JSON에서는 정수로 둔다.
            if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "checked: integral and within 2^53"
                )]
                Value::from(*f as i64)
            } else {
                Value::from(*f)
            }
        }
        Any::String(s) => Value::from(s.as_ref()),
        Any::Array(items) => Value::Array(items.iter().map(from_any).collect()),
        Any::Map(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), from_any(v))).collect()),
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use yrs::updates::decoder::Decode;
    use yrs::{Doc, Options, StateVector, Transact, Update};

    use super::*;

    const STORED: &str = include_str!("../../../../contracts/collab/stored.json");
    const EXPECTED: &str = include_str!("../../../../contracts/collab/expected.json");
    const FROM_JS: &str = include_str!("../../../../contracts/collab/from-js.b64");

    /// 표시 순서는 의미가 없으므로 비교 전에 이름순으로 맞춘다.
    fn normalize(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if let Some(Value::Array(marks)) = map.get_mut("marks") {
                    marks.sort_by(|a, b| a["type"].as_str().cmp(&b["type"].as_str()));
                }
                map.values_mut().for_each(normalize);
            }
            Value::Array(items) => items.iter_mut().for_each(normalize),
            _ => {}
        }
    }

    fn parse(text: &str) -> Value {
        let mut value: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        normalize(&mut value);
        value
    }

    #[test]
    fn reads_what_the_browser_writes() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(FROM_JS.trim())
            .unwrap_or_default();
        let doc = Doc::new();
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let update = Update::decode_v1(&bytes).ok();
        assert!(update.is_some(), "fixture decodes");
        if let Some(update) = update {
            assert!(doc.transact_mut().apply_update(update).is_ok());
        }
        let mut read = read_doc(&doc.transact(), &fragment);
        normalize(&mut read);
        assert_eq!(read, parse(EXPECTED));
    }

    /// 서버가 쓴 결과를 브라우저 테스트가 읽어 확인한다(`from-rust.b64`).
    #[test]
    fn writes_what_the_browser_reads() {
        let doc = Doc::with_options(Options {
            client_id: yrs::block::ClientID::new(1),
            ..Options::default()
        });
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let stored: Value = serde_json::from_str(STORED).unwrap_or(Value::Null);
        write_doc(&mut doc.transact_mut(), &fragment, &stored);
        let mut read = read_doc(&doc.transact(), &fragment);
        normalize(&mut read);
        assert_eq!(read, parse(EXPECTED), "round trip through Yjs");
        let bytes = doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../contracts/collab/from-rust.b64"
        );
        if std::env::var("FVOCI_UPDATE_COLLAB_FIXTURES").as_deref() == Ok("1") {
            assert!(std::fs::write(path, format!("{encoded}\n")).is_ok());
        }
        // 서식 속성은 해시 순서라 바이트는 실행마다 다를 수 있다. 저장된 파일이 같은 문서로 읽히는지만 본다.
        let committed = std::fs::read_to_string(path).unwrap_or_default();
        let committed = base64::engine::general_purpose::STANDARD
            .decode(committed.trim())
            .unwrap_or_default();
        let from_file = Doc::new();
        if let Ok(update) = Update::decode_v1(&committed) {
            assert!(from_file.transact_mut().apply_update(update).is_ok());
        }
        let file_fragment = from_file.get_or_insert_xml_fragment(FRAGMENT);
        let mut file_doc = read_doc(&from_file.transact(), &file_fragment);
        normalize(&mut file_doc);
        assert_eq!(
            file_doc,
            parse(EXPECTED),
            "run with FVOCI_UPDATE_COLLAB_FIXTURES=1"
        );
        // 쓴 문서는 Yjs 갱신으로 다시 읽어도 같다.
        let other = Doc::new();
        let decoded = Update::decode_v1(
            &doc.transact()
                .encode_state_as_update_v1(&StateVector::default()),
        )
        .ok();
        if let Some(update) = decoded {
            assert!(other.transact_mut().apply_update(update).is_ok());
        }
        let other_fragment = other.get_or_insert_xml_fragment(FRAGMENT);
        let mut again = read_doc(&other.transact(), &other_fragment);
        normalize(&mut again);
        assert_eq!(again, parse(EXPECTED));
    }
}
