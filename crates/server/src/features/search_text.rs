//! 검색 색인용 본문 글자 뽑기 (D-39).
//!
//! Tiptap JSON 본문에서 글자만 모은다. 블록 사이는 줄바꿈, 체크 항목은 `[x]`·`[ ]`를 앞에 붙인다. 참조는
//! 표시 이름(권한에 따라 달라짐)이 아니라 짧은 표기(`[reference kind="…" id="…"]`)로 남겨, 색인에 다른 사람이
//! 볼 수 없는 이름이 들어가지 않게 한다. 너무 길면 자른다.

use serde_json::Value;

/// 색인에 넣는 본문 글자 상한(바이트에 가깝게 문자 수로).
pub const MAX_INDEX_CHARS: usize = 100_000;

const BLOCKS: &[&str] = &[
    "paragraph",
    "heading",
    "blockquote",
    "codeBlock",
    "listItem",
    "taskItem",
    "horizontalRule",
    "table",
    "tableRow",
];

fn walk(node: &Value, out: &mut String) {
    if out.chars().count() >= MAX_INDEX_CHARS {
        return;
    }
    let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "text" => {
            if let Some(text) = node.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
            return;
        }
        "hardBreak" => {
            out.push('\n');
            return;
        }
        "reference" => {
            let attrs = node.get("attrs");
            let ref_kind = attrs
                .and_then(|a| a.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let id = attrs
                .and_then(|a| a.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            out.push_str(&format!("[reference kind=\"{ref_kind}\" id=\"{id}\"]"));
            return;
        }
        "taskItem" => {
            let checked = node
                .get("attrs")
                .and_then(|a| a.get("checked"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            out.push_str(if checked { "[x] " } else { "[ ] " });
        }
        _ => {}
    }
    if let Some(children) = node.get("content").and_then(Value::as_array) {
        for child in children {
            walk(child, out);
        }
    }
    if BLOCKS.contains(&kind) && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// 본문 JSON의 글자.
#[must_use]
pub fn plain_text(body: &Value) -> String {
    let mut out = String::new();
    walk(body, &mut out);
    let trimmed = out.trim();
    trimmed.chars().take(MAX_INDEX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_text_checklists_and_reference_shortcodes() {
        let body = json!({ "type": "doc", "content": [
            { "type": "heading", "attrs": { "level": 1 }, "content": [{ "type": "text", "text": "회의" }] },
            { "type": "paragraph", "content": [
                { "type": "text", "text": "담당 " },
                { "type": "reference", "attrs": { "kind": "user", "id": "u1" } },
                { "type": "hardBreak" },
                { "type": "text", "text": "다음 주" }
            ] },
            { "type": "taskList", "content": [
                { "type": "taskItem", "attrs": { "checked": true }, "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "자료 수집" }] }
                ] },
                { "type": "taskItem", "attrs": { "checked": false }, "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "초안" }] }
                ] }
            ] }
        ] });
        assert_eq!(
            plain_text(&body),
            "회의\n담당 [reference kind=\"user\" id=\"u1\"]\n다음 주\n[x] 자료 수집\n[ ] 초안"
        );
    }

    #[test]
    fn ignores_unknown_shapes_and_caps_length() {
        assert_eq!(plain_text(&json!({ "nonsense": true })), "");
        let long = "가".repeat(MAX_INDEX_CHARS + 10);
        let body = json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": long }] }] });
        assert_eq!(plain_text(&body).chars().count(), MAX_INDEX_CHARS);
    }
}
