pub fn validate_body(body: &Value, limits: &Limits) -> Result<(), AppError> {
    let is_doc = body
        .as_object()
        .and_then(|o| o.get("type"))
        .and_then(Value::as_str)
        == Some("doc");
    if !is_doc {
        return Err(AppError::BadRequest(
            "body must be a document object with type \"doc\"".into(),
        ));
    }
    // to_vec와 같은 직렬화 경로로 바이트만 센다. 상한을 넘겨도 끝까지 세어 오류의 실제 크기를 유지한다.
    struct ByteCount(usize);
    impl std::io::Write for ByteCount {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("serialized body size overflow"))?;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = ByteCount(0);
    serde_json::to_writer(&mut count, body)
        .map_err(|e| AppError::BadRequest(format!("body is not serializable: {e}")))?;
    let size = count.0;
    if size > limits.max_document_bytes {
        return Err(AppError::BadRequest(format!(
            "body is {size} bytes; the limit is {}",
            limits.max_document_bytes
        )));
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
                        return Err(AppError::BadRequest(format!(
                            "a document can hold at most {max} references"
                        )));
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
        Err(AppError::BadRequest(
            "images must point to an attachment of this service".into(),
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
        Err(AppError::BadRequest(
            "links must be http, https, mailto or a relative path".into(),
        ))
    }
}

/// 참조 노드는 `type`·`attrs`(와 편집기가 붙이는 `marks`)만 가진다. 제목 같은 글자를 노드에 두지 않는다.
/// id는 하이픈이 있는 36자 표준 형식만 받는다(DB 추출 함수와 같은 규칙).
fn check_reference_node(node: &serde_json::Map<String, Value>) -> Result<(), AppError> {
    let bad = || AppError::BadRequest("reference nodes need attrs {kind, id} only".into());
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

