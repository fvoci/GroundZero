//! 첨부 귀속은 인증된 업데이트의 연결에서 정한다. 문서·Yjs 속성에는 신원 정보를 쓰지 않는다(CR183-4).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;
use yrs::types::text::YChange;
use yrs::types::xml::{Xml, XmlFragment, XmlOut};
use yrs::{Assoc, Doc, IndexedSequence, OffsetKind, Out, ReadTxn, Text, Transact, XmlTextRef};

use super::convert::{self, FRAGMENT};
use fvoci_core::error::AppError;
use fvoci_core::ids::UserId;

/// app_collab_load가 반환하는 상태와 서버 전용 첨부 귀속.
pub(super) type LoadRow = (
    i32,
    String,
    Value,
    Option<Vec<u8>>,
    Option<Vec<Uuid>>,
    Option<Value>,
);

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct Attribution(Vec<Entry>);

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Entry {
    node: String,
    value: String,
    editor: Option<UserId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    span: Option<Span>,
}

/// 한 XmlText의 링크 서식에는 여러 편집자의 글자가 섞인다. CRDT 문자 ID 범위마다 귀속을 유지한다.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
struct Span {
    client: u64,
    clock: u32,
    len: u32,
}

type Previous = HashMap<(String, String, Option<u64>), BTreeMap<u32, Entry>>;

/// Yjs의 문자 ID 시계는 UTF-16이다. StickyIndex의 상대 위치도 같은 단위로 읽어야 한글·이모지의 귀속이 흔들리지 않는다.
pub(super) fn new_document() -> Doc {
    Doc::with_options(yrs::Options {
        offset_kind: OffsetKind::Utf16,
        ..Default::default()
    })
}

impl Attribution {
    pub(super) fn load(doc: &Doc, stored: Option<Value>) -> Result<Self, AppError> {
        let mut refs: Self = match stored {
            Some(value) => serde_json::from_value(value)
                .map_err(|_| AppError::Upstream("invalid stored attachment attribution".into()))?,
            None => Self::default(),
        };
        refs.observe(doc, None);
        Ok(refs)
    }

    /// 같은 노드의 같은 참조는 다른 편집자가 글·다른 속성을 고쳐도 원래 귀속을 유지한다.
    pub(super) fn observe(&mut self, doc: &Doc, editor: Option<UserId>) {
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let mut values = Vec::new();
        collect(
            &txn,
            fragment.children(&txn),
            doc.offset_kind(),
            &mut values,
        );
        let mut previous = Previous::new();
        for entry in std::mem::take(&mut self.0) {
            previous
                .entry(key(&entry))
                .or_default()
                .insert(entry.span.map_or(0, |s| s.clock), entry);
        }
        for entry in values {
            attribute(entry, &previous, editor, &mut self.0);
        }
    }

    pub(super) fn json(&self) -> Result<Value, AppError> {
        serde_json::to_value(self)
            .map_err(|_| AppError::Upstream("could not encode attachment attribution".into()))
    }
}

fn collect<T: ReadTxn>(
    txn: &T,
    children: impl Iterator<Item = XmlOut>,
    offset_kind: OffsetKind,
    out: &mut Vec<Entry>,
) {
    for child in children {
        let node = format!("{:?}", child.id());
        match child {
            XmlOut::Element(element) => {
                for (key, value) in element.attributes(txn) {
                    if let Out::Any(any) = value {
                        collect_values(&format!("{node}/{key}"), &convert::from_any(&any), out);
                    }
                }
                collect(txn, element.children(txn), offset_kind, out);
            }
            XmlOut::Text(text) => {
                collect_text(txn, &text, &node, offset_kind, out);
            }
            XmlOut::Fragment(fragment) => collect(txn, fragment.children(txn), offset_kind, out),
        }
    }
}

fn collect_values(node: &str, value: &Value, out: &mut Vec<Entry>) {
    match value {
        Value::String(text) => {
            for (at, _) in text.match_indices("/api/files/") {
                if let Some(reference) = reference(&text[at + "/api/files/".len()..]) {
                    out.push(Entry {
                        node: node.to_owned(),
                        value: reference,
                        editor: None,
                        span: None,
                    });
                }
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                collect_values(&format!("{node}/{key}"), value, out);
            }
        }
        Value::Array(values) => {
            for (i, value) in values.iter().enumerate() {
                collect_values(&format!("{node}/{i}"), value, out);
            }
        }
        _ => {}
    }
}

fn key(entry: &Entry) -> (String, String, Option<u64>) {
    (
        entry.node.clone(),
        entry.value.clone(),
        entry.span.map(|s| s.client),
    )
}

fn attribute(mut entry: Entry, previous: &Previous, editor: Option<UserId>, out: &mut Vec<Entry>) {
    let old = previous.get(&key(&entry));
    let Some(span) = entry.span else {
        entry.editor = old
            .and_then(|rows| rows.first_key_value())
            .map_or(editor, |(_, row)| row.editor);
        out.push(entry);
        return;
    };
    let end = span.clock + span.len;
    let mut cursor = span.clock;
    if let Some(rows) = old {
        let from = rows
            .range(..=cursor)
            .next_back()
            .map_or(cursor, |(clock, _)| *clock);
        for (&clock, row) in rows.range(from..end) {
            let Some(old_span) = row.span else {
                continue;
            };
            let start = cursor.max(clock);
            let stop = end.min(clock + old_span.len);
            if stop <= start {
                continue;
            }
            segment(&entry, cursor, start, editor, out);
            segment(&entry, start, stop, row.editor, out);
            cursor = stop;
        }
    }
    segment(&entry, cursor, end, editor, out);
}

fn segment(entry: &Entry, start: u32, end: u32, editor: Option<UserId>, out: &mut Vec<Entry>) {
    if start == end {
        return;
    }
    let mut piece = entry.clone();
    piece.editor = editor;
    if let Some(span) = &mut piece.span {
        span.clock = start;
        span.len = end - start;
    }
    if let Some(last) = out.last_mut()
        && let (Some(left), Some(right)) = (last.span.as_mut(), piece.span)
        && last.node == piece.node
        && last.value == piece.value
        && last.editor == piece.editor
        && left.client == right.client
        && left.clock + left.len == right.clock
    {
        left.len += right.len;
        return;
    }
    out.push(piece);
}

fn width(text: &str, kind: OffsetKind) -> u32 {
    u32::try_from(match kind {
        OffsetKind::Bytes => text.len(),
        OffsetKind::Utf16 => text.encode_utf16().count(),
    })
    .unwrap_or(u32::MAX)
}

fn collect_text<T: ReadTxn>(
    txn: &T,
    text: &XmlTextRef,
    node: &str,
    kind: OffsetKind,
    out: &mut Vec<Entry>,
) {
    let mut offset = 0;
    for diff in text.diff(txn, YChange::identity) {
        let Out::Any(yrs::Any::String(chunk)) = diff.insert else {
            continue;
        };
        for (at, _) in chunk.match_indices("/api/files/") {
            if let Some(reference) = reference(&chunk[at + "/api/files/".len()..])
                && let Some(index) =
                    text.sticky_index(txn, offset + width(&chunk[..at], kind), Assoc::After)
                && let Some(id) = index.id()
            {
                out.push(Entry {
                    node: format!("{node}/text/{id}"),
                    value: reference,
                    editor: None,
                    span: None,
                });
            }
        }
        let mut refs = Vec::new();
        for (key, value) in diff
            .attributes
            .into_iter()
            .flat_map(|attrs| (*attrs).into_iter())
        {
            collect_values(
                &format!("{node}/{key}"),
                &convert::from_any(&value),
                &mut refs,
            );
        }
        if !refs.is_empty() {
            for span in text_spans(txn, text, &chunk, offset, kind) {
                out.extend(refs.iter().cloned().map(|mut entry| {
                    entry.span = Some(span);
                    entry
                }));
            }
        }
        offset += width(&chunk, kind);
    }
}

fn text_spans<T: ReadTxn>(
    txn: &T,
    text: &XmlTextRef,
    chunk: &str,
    offset: u32,
    kind: OffsetKind,
) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut at = offset;
    for ch in chunk.chars() {
        if let Some(index) = text.sticky_index(txn, at, Assoc::After)
            && let Some(id) = index.id()
        {
            let len = u32::try_from(ch.len_utf16()).unwrap_or(1);
            if let Some(last) = spans.last_mut()
                && last.client == id.client.get()
                && last.clock + last.len == id.clock
            {
                last.len += len;
            } else {
                spans.push(Span {
                    client: id.client.get(),
                    clock: id.clock,
                    len,
                });
            }
        }
        at += width(ch.encode_utf8(&mut [0; 4]), kind);
    }
    spans
}

/// app_file_refs와 같은 UUID·version 형태만 귀속한다. 주변 글이나 URL의 다른 질의 변경은 삽입자를 바꾸지 않는다.
fn reference(rest: &str) -> Option<String> {
    let (file, query) = rest.split_once("/content?")?;
    if !(32..=36).contains(&file.len()) {
        return None;
    }
    let file = Uuid::parse_str(file).ok()?;
    let query = query
        .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\\'))
        .next()?;
    let raw = query
        .split('&')
        .find_map(|part| part.strip_prefix("version="))?;
    if raw.is_empty() || !raw.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits = raw.trim_start_matches('0');
    if digits.len() > 9 {
        return None;
    }
    let version = if digits.is_empty() {
        0
    } else {
        digits.parse::<u32>().ok()?
    };
    Some(format!("/api/files/{file}/content?version={version}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use yrs::types::Attrs;
    use yrs::types::xml::XmlTextPrelim;

    #[test]
    fn removing_a_readers_link_text_cannot_lend_their_identity_to_another_editors_text() {
        let doc = new_document();
        let kind = OffsetKind::Utf16;
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let text = fragment.push_back(&mut doc.transact_mut(), XmlTextPrelim::new(""));
        let a = UserId(Uuid::now_v7());
        let b = UserId(Uuid::now_v7());
        let attrs = Attrs::from([(
            Arc::from("link"),
            yrs::Any::from(HashMap::from([(
                "href".to_owned(),
                yrs::Any::from(format!("/api/files/{}/content?version=1", Uuid::now_v7())),
            )])),
        )]);
        let mut attribution = Attribution::default();
        text.insert_with_attributes(&mut doc.transact_mut(), 0, "가😀", attrs.clone());
        attribution.observe(&doc, Some(a));
        text.insert_with_attributes(&mut doc.transact_mut(), width("가😀", kind), "나😀", attrs);
        attribution.observe(&doc, Some(b));
        text.remove_range(&mut doc.transact_mut(), 0, width("가😀", kind));
        attribution.observe(&doc, Some(a));
        assert!(!attribution.0.is_empty());
        assert!(attribution.0.iter().all(|entry| entry.editor == Some(b)));
    }
}
