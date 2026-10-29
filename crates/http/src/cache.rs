//! ETag 생성과 조건부 요청 머리글 비교(D-77).

use axum::http::{HeaderMap, header};
use sha2::{Digest, Sha256};

/// 강한 ETag(따옴표 포함). 실제 응답 바이트와 요청한 사용자로 만든다. 같은 바이트라도 사용자가 다르면 값이
/// 달라 사용자를 바꾼 브라우저가 앞 사람의 검증자로 304를 받지 않는다. 앞의 표지는 계산 방식이 바뀌면 올린다.
pub fn strong_etag(user: &[u8], body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"fvoci-representation-1\0");
    hasher.update(user);
    hasher.update(b"\0");
    hasher.update(body);
    let digest = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut etag = String::with_capacity(34);
    etag.push('"');
    for byte in &digest[..16] {
        etag.push(char::from(HEX[usize::from(byte >> 4)]));
        etag.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    etag.push('"');
    etag
}

/// 조건 머리글에 담긴 엔터티 태그.
enum Tags {
    Absent,
    /// `*`
    Any,
    /// (약한지, 따옴표 포함 태그)
    List(Vec<(bool, String)>),
}

fn tags(headers: &HeaderMap, name: header::HeaderName) -> Tags {
    let mut found = false;
    let mut out = Vec::new();
    for value in headers.get_all(name) {
        found = true;
        // 읽을 수 없는 값은 아무것과도 맞지 않는 것으로 본다.
        let Ok(text) = value.to_str() else { continue };
        for part in text.split(',') {
            let part = part.trim();
            if part == "*" {
                return Tags::Any;
            }
            match part.strip_prefix("W/") {
                Some(tag) => out.push((true, tag.to_owned())),
                None if !part.is_empty() => out.push((false, part.to_owned())),
                None => {}
            }
        }
    }
    if found { Tags::List(out) } else { Tags::Absent }
}

/// `If-None-Match`가 지금 표현과 맞는지(약한 비교). 맞으면 304를 돌려준다.
pub fn none_match(headers: &HeaderMap, etag: &str) -> bool {
    match tags(headers, header::IF_NONE_MATCH) {
        Tags::Absent => false,
        Tags::Any => true,
        Tags::List(list) => list.iter().any(|(_, tag)| tag == etag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn with(name: header::HeaderName, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            name,
            HeaderValue::from_str(value).unwrap_or(HeaderValue::from_static("")),
        );
        h
    }

    #[test]
    fn etags_depend_on_bytes_and_user() {
        let a = strong_etag(b"u1", b"{}");
        assert!(a.starts_with('"') && a.ends_with('"') && a.len() == 34);
        assert_eq!(a, strong_etag(b"u1", b"{}"));
        assert_ne!(a, strong_etag(b"u2", b"{}"));
        assert_ne!(a, strong_etag(b"u1", b"{ }"));
    }

    #[test]
    fn etags_preserve_the_domain_and_truncated_digest_bytes() {
        for (user, body, expected) in [
            (
                b"".as_slice(),
                b"".as_slice(),
                "\"4a46a75d39f81999433449f43c77bfe1\"",
            ),
            (b"u1", b"{}", "\"e9139f4f87432a88b31104891eb1491e\""),
            (b"u2", b"{}", "\"a15e964cf2c9e415f3f33dc7c6cedf53\""),
            (b"u1", b"{ }", "\"b1c8bd8f4d20777303d4974130a6dcfe\""),
        ] {
            assert_eq!(strong_etag(user, body), expected);
        }

        // Binary users/bodies, including NUL and every byte value, must keep the
        // original domain separation, first 128 bits, padding and lowercase.
        let body: Vec<u8> = (0..=255).collect();
        let mut covers_zero_byte = false;
        for len in 0..=body.len() {
            let user = &body[len..];
            let bytes = &body[..len];
            let mut hasher = Sha256::new();
            hasher.update(b"fvoci-representation-1\0");
            hasher.update(user);
            hasher.update(b"\0");
            hasher.update(bytes);
            let digest = hasher.finalize();
            covers_zero_byte |= digest[..16].contains(&0);
            let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
            assert_eq!(strong_etag(user, bytes), format!("\"{hex}\""));
        }
        assert!(
            covers_zero_byte,
            "corpus must exercise leading-zero padding"
        );
    }

    #[test]
    fn if_none_match_uses_weak_comparison() {
        let tag = "\"abc\"";
        assert!(none_match(&with(header::IF_NONE_MATCH, "\"abc\""), tag));
        assert!(none_match(&with(header::IF_NONE_MATCH, "W/\"abc\""), tag));
        assert!(none_match(
            &with(header::IF_NONE_MATCH, "\"x\", W/\"abc\""),
            tag
        ));
        assert!(none_match(&with(header::IF_NONE_MATCH, "*"), tag));
        assert!(!none_match(&with(header::IF_NONE_MATCH, "\"abd\""), tag));
        assert!(!none_match(&HeaderMap::new(), tag));
    }
}
