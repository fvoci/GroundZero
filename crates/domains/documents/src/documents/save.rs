//! 본문 PUT의 판 조건·멱등 저장.

use crate::state::DocumentsState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::DocumentId;
use fvoci_core::validate::validate_title;
use fvoci_http::cache;
use fvoci_http::session::ExpectedUser;

use super::{
    ConflictBody, Document, EXPECTED_USER_DOC, SaveConflict, SaveDocument, conflict,
    conflict_reply, document_reply, representation, to_document, validate_body, write_revision,
};

/// 문서 저장. 충돌 시 409(또는 `If-Match`가 맞지 않으면 412) + 서버 최신본.
///
/// 순서: 권한 확인 → 행 잠금 → 재시도 판단 → `If-Match`(있으면) → 리비전 비교 → 내용 동일 판단 → 갱신·리비전 기록.
/// 행을 잠근 뒤 판단하므로 동시에 들어온 저장도 최신 행 기준으로 결정된다. `If-Match`는 잠근 행으로 만든 지금
/// 표현의 ETag와 강하게 비교한다(D-77). 같은 요청의 재시도는 `If-Match`보다 먼저 판단해 멱등 응답을 지킨다.
///
/// 권한 확인보다 먼저 기대 사용자를 비교한다(`ExpectedUser`): 다르면 문서를 읽거나 잠그지 않고 409 `account_changed`.
#[utoipa::path(put, path = "/api/documents/{document_id}",
    params(("document_id" = String, Path, format = Uuid),
           ("If-Match" = Option<String>, Header, description = "단건 조회의 강한 ETag. 다르면 412"),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_USER_DOC)),
    request_body = SaveDocument,
    responses((status = 200, body = Document, headers(("ETag" = String, description = "저장 뒤 표현의 ETag"))),
              (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = SaveConflict,
               description = "`conflict`면 ConflictBody(서버 최신본 포함). `account_changed`(기대 사용자가 세션의 사용자와 다름)·`invalid_state`(실시간 편집 중 본문 변경 등)면 ErrorBody. 어느 쪽도 아무것도 바꾸지 않았다"),
              (status = 412, body = ConflictBody)), tag = "documents")]
pub async fn save(
    State(state): State<DocumentsState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<DocumentId>,
    headers: HeaderMap,
    Json(input): Json<SaveDocument>,
) -> Result<Response, AppError> {
    let title = validate_title(&input.title, state.limits.max_title_chars, "title")?;
    validate_body(&input.body, &state.limits)?;

    let mut tx = state.db.begin_as(user).await?;
    let (access, current, realtime) = crate::document_locks::for_save(&mut tx, id).await?;

    // 같은 사용자의 같은 요청 재시도. 그 요청이 만든 리비전이 아직 최신이면 성공으로 돌려주고,
    // 그 뒤에 다른 저장이 있었으면 충돌로 알린다(재시도한 클라이언트가 남의 최신본을 덮지 않게).
    let replay: Option<(i32, Option<i32>)> = sqlx::query_as(
        "SELECT revision, restored_from FROM document_revisions
         WHERE document_id = $1 AND request_id = $2 AND author = app_user_id()",
    )
    .bind(id)
    .bind(input.request_id)
    .fetch_optional(tx.conn())
    .await?;
    if let Some((_, Some(_))) = replay {
        // 복원에 쓴 request_id를 저장에 다시 쓰면 재시도로 보지 않는다(엉뚱한 성공 응답 방지).
        return Err(AppError::BadRequest(
            "request_id was already used for another change".into(),
        ));
    }
    if let Some((revision, None)) = replay {
        tx.commit().await?;
        let doc = to_document(current, access);
        return if doc.revision == revision {
            document_reply(user, &doc)
        } else {
            conflict(user, doc)
        };
    }

    if headers.contains_key(header::IF_MATCH) {
        let now = to_document(current.clone(), access);
        let (_, etag) = representation(user, &now)?;
        if cache::if_match(&headers, &etag) == Some(false) {
            tx.commit().await?;
            return conflict_reply(
                StatusCode::PRECONDITION_FAILED,
                "precondition_failed",
                user,
                now,
            );
        }
    }
    if current.revision != input.base_revision {
        tx.commit().await?;
        return conflict(user, to_document(current, access));
    }
    // 내용이 같으면 새 리비전을 만들지 않는다. JSON 객체 비교는 키 순서와 무관하다.
    if current.title == title && current.body == input.body {
        tx.commit().await?;
        return document_reply(user, &to_document(current, access));
    }
    // 실시간 편집 중이면 본문은 실시간 연결만 고친다(두 길로 본문이 갈라지지 않게). 제목은 여기서 바꾼다.
    if realtime && current.body != input.body {
        return Err(AppError::Conflict(
            "realtime editing is on for this document; edit the body in the live editor".into(),
        ));
    }

    // 저장 API는 늘 저장하는 사람이 쓴 내용이다(대신 저장 기록 없음, D-95).
    let row = write_revision(
        &mut tx,
        id,
        &title,
        &input.body,
        input.request_id,
        None,
        None,
    )
    .await?;
    tx.commit().await?;
    document_reply(user, &to_document(row, access))
}
