//! 본문·참조·첨부 주소 검증.

use crate::state::DocumentLimits as Limits;
use fvoci_core::error::{AppError, InvalidReason};
use serde_json::Value;
use uuid::Uuid;

pub(crate) fn empty_body() -> Value {
    serde_json::json!({ "type": "doc", "content": [{ "type": "paragraph" }] })
}

/// 본문 검증: `type: "doc"` 객체이고 직렬화 크기가 상한 이하.
pub fn validate_body(body: &Value, limits: &Limits) -> Result<(), AppError> {
    let is_doc = body
        .as_object()
        .and_then(|o| o.get("type"))
        .and_then(Value::as_str)
        == Some("doc");
    if !is_doc {
        return Err(invalid_body(
            "body must be a document object with type \"doc\"",
        ));
    }
    let size = serde_json::to_vec(body)
        .map_err(|e| invalid_body(format!("body is not serializable: {e}")))?
        .len();
    if size > limits.max_document_bytes {
        return Err(AppError::invalid(
            "body",
            InvalidReason::TooLong,
            u64::try_from(limits.max_document_bytes).ok(),
            format!(
                "body is {size} bytes; the limit is {}",
                limits.max_document_bytes
            ),
        ));
    }
    validate_references(body, limits.max_references_per_document)
}

/// 참조 노드(`type: "reference"`) 검사: `attrs.kind`는 item·document·user, `attrs.id`는 UUID,
/// 개수는 상한 이하. 표시 이름 같은 다른 속성은 받지 않는다(제목이 본문에 남지 않게, D-18).
/// 이미지 주소와 링크 주소도 같은 순회에서 확인한다(D-27). 깊은 중첩에서도 스택이 넘치지 않도록 반복으로 순회한다.
fn validate_references(body: &Value, max: usize) -> Result<(), AppError> {
    let mut stack = vec![body];
    let mut count = 0usize;
    while let Some(node) = stack.pop() {
        match node {
            Value::Array(items) => stack.extend(items),
            Value::Object(map) => {
                match map.get("type").and_then(Value::as_str) {
                    Some("image") => check_image_node(map)?,
                    Some("link") => check_link_mark(map)?,
                    _ => {}
                }
                if map.get("type").and_then(Value::as_str) == Some("reference") {
                    count += 1;
                    if count > max {
                        return Err(AppError::invalid(
                            "body",
                            InvalidReason::TooMany,
                            u64::try_from(max).ok(),
                            format!("a document can hold at most {max} references"),
                        ));
                    }
                    check_reference_node(map)?;
                }
                stack.extend(map.values());
            }
            _ => {}
        }
    }
    Ok(())
}

/// 이미지 노드의 주소는 이 서비스의 첨부 파일 내용 주소만 받는다(D-27). 바깥 주소는 읽는 사람의 접속 정보를
/// 남에게 보내므로(추적 이미지) 받지 않는다.
fn check_image_node(node: &serde_json::Map<String, Value>) -> Result<(), AppError> {
    let src = node
        .get("attrs")
        .and_then(|a| a.get("src"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if is_file_url(src) {
        Ok(())
    } else {
        Err(invalid_body(
            "images must point to an attachment of this service",
        ))
    }
}

/// 첨부 주소의 표준형: `/api/files/{소문자 하이픈 id}/content` 뒤에 없음·`?inline=true`·`?version=N`·
/// `?version=N&inline=true`(N은 0으로 시작하지 않는 양의 32비트 정수). 화면(`fileContentUrl`)이 만드는 꼴이고,
/// DB가 리비전 본문에서 버전 참조를 뽑는 규칙(D-56)이 이 꼴을 빠짐없이 알아본다.
fn is_file_url(url: &str) -> bool {
    let Some((id, query)) = url
        .strip_prefix("/api/files/")
        .and_then(|rest| rest.split_once("/content"))
    else {
        return false;
    };
    let version_ok = |v: &str| {
        !v.starts_with('0') && v.bytes().all(|b| b.is_ascii_digit()) && v.parse::<i32>().is_ok()
    };
    Uuid::parse_str(id).is_ok_and(|u| u.hyphenated().to_string() == id)
        && match query {
            "" | "?inline=true" => true,
            q => q
                .strip_prefix("?version=")
                .is_some_and(|v| version_ok(v.strip_suffix("&inline=true").unwrap_or(v))),
        }
}

/// 링크 표시의 주소: 스킴이 있으면 http·https·mailto만(`javascript:`·`data:` 등 거부), 없으면 상대 경로·앵커.
fn check_link_mark(mark: &serde_json::Map<String, Value>) -> Result<(), AppError> {
    let href = mark
        .get("attrs")
        .and_then(|a| a.get("href"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let raw = href;
    let href = href.trim();
    let lower = href.to_ascii_lowercase();
    // 스킴이 있으면 http·https·mailto만, 없으면(상대 경로·#앵커) 다른 호스트로 가는 `//`만 막는다.
    let scheme = lower.split_once(':').map(|(s, _)| s).filter(|s| {
        s.starts_with(|c: char| c.is_ascii_alphabetic())
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    let ok = match scheme {
        Some(s) => matches!(s, "http" | "https" | "mailto"),
        None => !href.starts_with("//") && (!lower.starts_with("/api/files/") || is_file_url(raw)),
    };
    if ok && !href.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(invalid_body(
            "links must be http, https, mailto or a relative path",
        ))
    }
}

/// 참조 노드는 `type`·`attrs`(와 편집기가 붙이는 `marks`)만 가진다. 제목 같은 글자를 노드에 두지 않는다.
/// id는 하이픈이 있는 36자 표준 형식만 받는다(DB 추출 함수와 같은 규칙).
fn check_reference_node(node: &serde_json::Map<String, Value>) -> Result<(), AppError> {
    let bad = || invalid_body("reference nodes need attrs {kind, id} only");
    if node
        .keys()
        .any(|k| !matches!(k.as_str(), "type" | "attrs" | "marks"))
    {
        return Err(bad());
    }
    let attrs = node
        .get("attrs")
        .and_then(Value::as_object)
        .ok_or_else(bad)?;
    if attrs.len() != 2 {
        return Err(bad());
    }
    let kind = attrs.get("kind").and_then(Value::as_str);
    if !matches!(kind, Some("item" | "document" | "task" | "user")) {
        return Err(bad());
    }
    let id = attrs.get("id").and_then(Value::as_str).ok_or_else(bad)?;
    let canonical = id.len() == 36
        && id.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    if !canonical {
        return Err(bad());
    }
    Ok(())
}

fn invalid_body(message: impl Into<String>) -> AppError {
    AppError::invalid("body", InvalidReason::InvalidFormat, None, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fvoci_core::validate::validate_title;
    use serde_json::json;

    fn limits(max: usize) -> Limits {
        Limits {
            max_document_bytes: max,
            max_title_chars: 10,
            max_references_per_document: 2,
            max_document_depth: 3,
        }
    }

    #[test]
    fn body_must_be_doc_object() {
        assert!(validate_body(&serde_json::json!({ "type": "doc" }), &limits(100)).is_ok());
        assert!(validate_body(&serde_json::json!({ "type": "paragraph" }), &limits(100)).is_err());
        assert!(validate_body(&serde_json::json!([1, 2]), &limits(100)).is_err());
    }

    #[test]
    fn body_size_is_limited() {
        let big = serde_json::json!({ "type": "doc", "content": "x".repeat(200) });
        assert!(matches!(
            validate_body(&big, &limits(100)),
            Err(AppError::Invalid(_))
        ));
    }

    #[test]
    fn title_validation() {
        assert_eq!(validate_title("  회의록  ", 10, "title").unwrap(), "회의록");
        assert!(validate_title("   ", 10, "title").is_err());
        assert!(validate_title("가나다라마바사아자차카", 10, "title").is_err());
        assert!(validate_title("a\u{0007}b", 10, "title").is_err());
    }

    #[test]
    fn reference_nodes_are_checked_and_limited() {
        let id = "0198f1f2-0000-7000-8000-000000000001";
        let reference = |kind: &str, id: &str| json!({ "type": "reference", "attrs": { "kind": kind, "id": id } });
        let doc = |nodes: Vec<Value>| json!({ "type": "doc", "content": [{ "type": "paragraph", "content": nodes }] });
        let l = limits(10_000);
        assert!(
            validate_body(&doc(vec![reference("item", id), reference("user", id)]), &l).is_ok()
        );
        assert!(validate_body(&doc(vec![reference("workspace", id)]), &l).is_err());
        assert!(validate_body(&doc(vec![reference("item", "DEV-1")]), &l).is_err());
        // DB 추출과 같은 형식만: 하이픈 없는 형식·중괄호·urn은 거부한다.
        for odd in [
            "0198f1f20000700080000000000000001",
            "{0198f1f2-0000-7000-8000-000000000001}",
            "urn:uuid:0198f1f2-0000-7000-8000-000000000001",
        ] {
            assert!(
                validate_body(&doc(vec![reference("item", odd)]), &l).is_err(),
                "{odd}"
            );
        }
        let with_text =
            json!({ "type": "reference", "text": "비밀", "attrs": { "kind": "item", "id": id } });
        assert!(
            validate_body(&doc(vec![with_text]), &l).is_err(),
            "no title outside attrs"
        );
        let labelled =
            json!({ "type": "reference", "attrs": { "kind": "item", "id": id, "label": "비밀" } });
        assert!(
            validate_body(&doc(vec![labelled]), &l).is_err(),
            "labels are not stored"
        );
        let three = doc(vec![
            reference("item", id),
            reference("item", id),
            reference("user", id),
        ]);
        assert!(
            validate_body(&three, &l).is_err(),
            "limit is 2 in this test"
        );
    }

    #[test]
    fn images_and_links_must_stay_inside_the_service() {
        let doc = |node: serde_json::Value| json!({ "type": "doc", "content": [node] });
        let image =
            |src: &str| doc(json!({ "type": "image", "attrs": { "src": src, "alt": "x" } }));
        let id = "01a1481e-6cb0-73cb-a92b-c7cf6cbc3ebf";
        for ok in [
            format!("/api/files/{id}/content"),
            format!("/api/files/{id}/content?version=2&inline=true"),
            format!("/api/files/{id}/content?inline=true"),
            format!("/api/files/{id}/content?version=2147483647"),
        ] {
            assert!(validate_body(&image(&ok), &limits(10_000)).is_ok(), "{ok}");
        }
        for bad in [
            "https://tracker.example/pixel.png".to_owned(),
            format!("/api/files/{id}/content?version=1&x=1"),
            "/api/files/not-an-id/content".to_owned(),
            "javascript:alert(1)".to_owned(),
            // 표준형이 아니면 버전 참조(D-56)를 놓치므로 받지 않는다.
            format!("/api/files/{}/content?version=1", id.to_uppercase()),
            format!("/api/files/{}/content?version=1", id.replace('-', "")),
            format!("/api/files/{id}/content?inline=true&version=1"),
            format!("/api/files/{id}/content?version=01"),
            format!("/api/files/{id}/content?version=+1"),
            format!("/api/files/{id}/content?version=0"),
            format!("/api/files/{id}/content?version=99999999999"),
        ] {
            assert!(
                validate_body(&image(&bad), &limits(10_000)).is_err(),
                "{bad}"
            );
        }
        let link = |href: &str| {
            doc(
                json!({ "type": "paragraph", "content": [{ "type": "text", "text": "a",
                "marks": [{ "type": "link", "attrs": { "href": href } }] }] }),
            )
        };
        for ok in [
            "https://example.test",
            "mailto:a@example.test",
            "/docs/somewhere",
            "/api/files/01a1481e-6cb0-73cb-a92b-c7cf6cbc3ebf/content?version=3",
        ] {
            assert!(validate_body(&link(ok), &limits(10_000)).is_ok(), "{ok}");
        }
        for bad in [
            "javascript:alert(1)",
            "//evil.example",
            "data:text/html,x",
            "/api/files/x/content",
            "/API/files/01a1481e-6cb0-73cb-a92b-c7cf6cbc3ebf/content?version=03",
            " /api/files/01a1481e-6cb0-73cb-a92b-c7cf6cbc3ebf/content?version=3 ",
        ] {
            assert!(validate_body(&link(bad), &limits(10_000)).is_err(), "{bad}");
        }
    }
}
