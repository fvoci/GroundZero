//! 안전한 형식의 미리보기와 첨부 내용 스트리밍.
use crate::{FilesState, model::ContentQuery, resource::one_file};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::FileId;
use fvoci_http::session::CurrentUser;
use std::fmt::Write as _;
/// 브라우저 안에서 열어도 되는 형식(스크립트를 실행할 수 없는 것만). SVG·HTML은 늘 내려받기.
const INLINE_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "application/pdf",
    "text/plain",
];

/// 저장된 형식 문자열을 안전한 `type/subtype`으로(매개변수·이상한 문자는 버림).
fn safe_type(raw: &str) -> String {
    let base = raw
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let ok = base.split_once('/').is_some_and(|(t, s)| {
        !t.is_empty()
            && !s.is_empty()
            && base
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/.+-_".contains(c))
    });
    if ok {
        base
    } else {
        "application/octet-stream".into()
    }
}

/// `Content-Disposition` 값. 이름은 RFC 5987로, ASCII 대체 이름도 함께.
#[derive(Debug, Clone, Copy)]
enum Disposition {
    Inline,
    Attachment,
}

fn disposition(mode: Disposition, name: &str) -> String {
    let fallback: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            let _ignored = write!(encoded, "%{b:02X}");
        }
    }
    format!(
        "{}; filename=\"{fallback}\"; filename*=UTF-8''{encoded}",
        if matches!(mode, Disposition::Inline) {
            "inline"
        } else {
            "attachment"
        }
    )
}

/// 내려받기(볼 수 있는 사람). 최신 또는 `version`. `inline=true`는 이미지·PDF·텍스트만 브라우저 안에서 연다.
#[utoipa::path(get, path = "/api/files/{file_id}/content", operation_id = "file_content",
    params(("file_id" = String, Path, format = Uuid), ContentQuery),
    responses((status = 200, content_type = "application/octet-stream", body = Vec<u8>),
              (status = 404, body = ErrorBody)), tag = "files")]
pub async fn content(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<FileId>,
    Query(query): Query<ContentQuery>,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let file = one_file(&mut tx, id).await?;
    let version = query.version.unwrap_or(file.current_version);
    let found: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT storage_key, size, content_type FROM file_versions WHERE file_id = $1 AND version = $2",
    )
    .bind(id)
    .bind(version)
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;
    let (key, size, content_type) = found.ok_or(AppError::NotFound)?;
    let content_type = safe_type(&content_type);
    let viewable = INLINE_TYPES.contains(&content_type.as_str());
    // 내려받기가 허용되지 않으면 안전한 형식을 브라우저 안에서 보는 것만 된다.
    if !file.can_download && !(query.inline && viewable) {
        return Err(AppError::Forbidden);
    }
    let disposition_kind = if query.inline && viewable {
        Disposition::Inline
    } else {
        Disposition::Attachment
    };
    let stream = state.storage.get(&key).await?;
    let mut response = Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    let shown = if content_type == "text/plain" {
        "text/plain; charset=utf-8".to_owned()
    } else {
        content_type
    };
    let set = |headers: &mut HeaderMap, name: header::HeaderName, value: String| {
        if let Ok(v) = HeaderValue::from_str(&value) {
            headers.insert(name, v);
        }
    };
    set(headers, header::CONTENT_TYPE, shown);
    set(headers, header::CONTENT_LENGTH, size.to_string());
    set(
        headers,
        header::CONTENT_DISPOSITION,
        disposition(disposition_kind, &file.name),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'",
        ),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_are_cleaned() {
        assert_eq!(safe_type("Image/PNG; charset=x"), "image/png");
        assert_eq!(safe_type("text/html\r\nX: y"), "application/octet-stream");
        assert_eq!(safe_type(""), "application/octet-stream");
        assert_eq!(
            safe_type("application/vnd.ms-excel"),
            "application/vnd.ms-excel"
        );
    }

    #[test]
    fn dispositions_quote_names_safely() {
        assert_eq!(
            disposition(Disposition::Attachment, "회의 \"요약\".pdf"),
            "attachment; filename=\"__ ____.pdf\"; filename*=UTF-8''%ED%9A%8C%EC%9D%98%20%22%EC%9A%94%EC%95%BD%22.pdf"
        );
        assert!(disposition(Disposition::Inline, "a.png").starts_with("inline;"));
    }
}
