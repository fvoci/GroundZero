//! 판 충돌 409의 공통 본문(#180의 서버 부분, D-131 2026-10-28 개정). 구조 데이터(업무·일정·항목)는 CRDT로 합치지
//! 않고 명령의 판 조건으로 막는다(D-79·D-89·D-94, #181 A). 판이 지났으면 아무것도 바꾸지 않고, 지금 자원과 요청이
//! 바꾸려던 칸 가운데 지금 값과 다른 칸을 돌려준다. 화면은 이 본문으로 입력을 보존한 채 다시 읽기를 안내하고, 칸 비교
//! 화면(#180 폼 부분, 시기 따로)도 이 본문을 쓴다. 뒤 조각도 이 모양을 따른다(노르웨이숲 태비 초안 6의 3).

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

/// 판 충돌 409 본문.
#[derive(Debug, Serialize, ToSchema)]
pub struct StaleWrite<T> {
    /// 충돌 종류(예: 업무 `conflict`·`shift_conflict`). 오류 본문(`ErrorBody`)의 `error`와 같은 자리다.
    pub error: &'static str,
    /// 사람이 읽는 영어 진단 문구.
    pub message: String,
    /// 이 요청의 id(응답 머리글 `x-request-id`와 같다, D-83).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub request_id: Option<String>,
    /// 지금 서버의 자원.
    pub current: T,
    /// 요청이 바꾸려던 칸 가운데 `current`의 값과 다른 칸 이름(요청 본문의 칸 이름, 이름 순). `current`에 없는 칸은 보지
    /// 않는다. 비어 있으면 판만 지났고 보낸 칸의 값은 지금과 같다.
    pub changed: Vec<String>,
}

impl<T: Serialize> StaleWrite<T> {
    /// `sent`는 요청이 바꾸려던 칸의 값(검증·정리한 뒤의 모양, JSON 객체)이다.
    pub fn new(error: &'static str, message: &str, current: T, sent: &Value) -> Self {
        let changed = changed_fields(sent, &current);
        Self {
            error,
            message: message.to_owned(),
            request_id: fvoci_core::logging::current_request_id(),
            current,
            changed,
        }
    }
}

/// `sent`의 칸 가운데 `current`를 JSON으로 바꾼 값과 다른 칸 이름(이름 순). `current`에 없는 칸은 넣지 않는다.
pub fn changed_fields<T: Serialize>(sent: &Value, current: &T) -> Vec<String> {
    let (Value::Object(sent), Ok(Value::Object(now))) = (sent, serde_json::to_value(current))
    else {
        return Vec::new();
    };
    let mut changed: Vec<String> = sent
        .iter()
        .filter(|(name, value)| now.get(*name).is_some_and(|current| current != *value))
        .map(|(name, _)| name.clone())
        .collect();
    changed.sort();
    changed
}

impl<T: Serialize> IntoResponse for StaleWrite<T> {
    fn into_response(self) -> Response {
        let error = self.error;
        let mut response = (StatusCode::CONFLICT, Json(self)).into_response();
        response
            .extensions_mut()
            .insert(fvoci_core::logging::ErrorCode(error));
        response
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn changed_lists_sent_fields_that_differ_from_the_current_resource() {
        let current = json!({ "title": "지금", "due_date": "2026-11-02", "priority": "high" });
        let sent = json!({ "title": "내 제목", "due_date": "2026-11-02", "assignee_ids": [] });
        // 같은 값(due_date)과 지금 자원에 없는 칸(assignee_ids)은 넣지 않는다.
        assert_eq!(changed_fields(&sent, &current), vec!["title"]);
        assert!(changed_fields(&json!({}), &current).is_empty());
        assert!(changed_fields(&json!(null), &current).is_empty());
    }
}
