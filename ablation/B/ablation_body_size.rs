//! Exact serialization limits and validation ordering; no database is involved.
use fvoci_server::config::Limits;
use fvoci_server::error::AppError;
use fvoci_server::features::documents::validate_body;
use serde_json::{Value, json};

fn limits(bytes: usize, references: usize) -> Limits {
    Limits {
        max_document_bytes: bytes,
        max_title_chars: 100,
        max_references_per_document: references,
        max_document_depth: 32,
    }
}

#[allow(
    clippy::panic,
    reason = "test helper must fail on an unexpected result"
)]
fn error(body: &Value, bytes: usize, references: usize) -> String {
    match validate_body(body, &limits(bytes, references)) {
        Err(AppError::BadRequest(message)) => message,
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[test]
fn serialized_byte_limits_are_exact_for_unicode_escapes_numbers_and_large_bodies() {
    let mut deep = json!({"text": "끝"});
    for _ in 0..128 {
        deep = json!([deep]);
    }
    let bodies = [
        json!({"type": "doc"}),
        json!({"type": "doc", "text": "한글🦀é\"\\\n\r\t\u{0000}\u{001f}"}),
        json!({"type": "doc", "numbers": [i64::MIN, u64::MAX, -0.0, 1.25e-20, 1.25e20],
               "values": [null, true, false, [], {}]}),
        json!({"type": "doc", "text": "한\n🦀".repeat(131_072)}),
        json!({"type": "doc", "deep": deep}),
    ];
    for body in bodies {
        let before = body.clone();
        let bytes = serde_json::to_vec(&body).unwrap().len();
        assert!(validate_body(&body, &limits(bytes, 2)).is_ok());
        assert!(validate_body(&body, &limits(bytes + 1, 2)).is_ok());
        assert_eq!(
            error(&body, bytes - 1, 2),
            format!("body is {bytes} bytes; the limit is {}", bytes - 1)
        );
        assert_eq!(body, before, "validation must not change the input");
    }
}

#[test]
fn type_then_size_then_content_errors_keep_their_precedence() {
    for body in [
        json!(null),
        json!([]),
        json!({"type": "paragraph", "text": "x".repeat(100)}),
    ] {
        assert_eq!(
            error(&body, 0, 0),
            "body must be a document object with type \"doc\""
        );
    }
    let cases = [
        (
            json!({"type": "image", "attrs": {"src": "https://example.test/x"}}),
            "images must point to an attachment of this service",
        ),
        (
            json!({"type": "link", "attrs": {"href": "javascript:alert(1)"}}),
            "links must be http, https, mailto or a relative path",
        ),
        (
            json!({"type": "reference", "attrs": {"kind": "item", "id": "bad"}}),
            "reference nodes need attrs {kind, id} only",
        ),
    ];
    for (node, expected) in cases {
        let body = json!({"type": "doc", "content": [node]});
        let bytes = serde_json::to_vec(&body).unwrap().len();
        assert_eq!(
            error(&body, bytes - 1, 2),
            format!("body is {bytes} bytes; the limit is {}", bytes - 1)
        );
        assert_eq!(error(&body, bytes, 2), expected);
    }
    // The existing stack visits the last array entry first.
    let body = json!({"type": "doc", "content": [
        {"type": "image"}, {"type": "link", "attrs": {"href": "javascript:x"}}
    ]});
    assert_eq!(
        error(&body, usize::MAX, 2),
        "links must be http, https, mailto or a relative path"
    );
}

#[test]
fn reference_limit_is_inclusive_and_precedes_invalid_excess_reference() {
    let reference = json!({"type": "reference", "attrs": {
        "kind": "item", "id": "0198f1f2-0000-7000-8000-000000000001"
    }});
    let exact = json!({"type": "doc", "content": [reference.clone(), reference.clone()]});
    assert!(validate_body(&exact, &limits(usize::MAX, 2)).is_ok());
    let excess = json!({"type": "doc", "content": [
        {"type": "reference"}, reference.clone(), reference
    ]});
    assert_eq!(
        error(&excess, usize::MAX, 2),
        "a document can hold at most 2 references"
    );
    assert_eq!(
        error(&excess, usize::MAX, 0),
        "a document can hold at most 0 references"
    );
}
