//! SR-298-5: 시작 목록은 같은 요청 쿠키의 사용자에 묶이고 개인 배열·캐시 규칙을 유지한다.

use crate::common;
use axum::http::{Method, StatusCode, header};
use serde_json::Value;

#[tokio::test]
async fn startup_lists_identify_the_cookie_user_and_keep_private_arrays() {
    let state = common::state();
    let app = common::app(&state);
    let alice = common::login(&state, "list alice").await;
    let bob = common::login(&state, "list bob").await;
    let alice_space = common::create_workspace(&app, &alice, "Alice list space").await;
    let bob_space = common::create_workspace(&app, &bob, "Bob list space").await;
    for (user, own, foreign) in [
        (&bob, &bob_space, &alice_space),
        (&alice, &alice_space, &bob_space),
        (&bob, &bob_space, &alice_space),
    ] {
        for path in ["/api/workspaces", "/api/me/organizations"] {
            let (status, headers, body) =
                common::send_raw(&app, Method::GET, path, Some(user), &[], Vec::new()).await;
            assert_eq!(status, StatusCode::OK, "{path}");
            let expected = user.id.0.to_string();
            assert_eq!(
                headers
                    .get("x-fvoci-viewer-id")
                    .and_then(|v| v.to_str().ok()),
                Some(expected.as_str()),
                "{path} must identify the authenticated cookie user"
            );
            assert_eq!(
                headers[header::CACHE_CONTROL],
                "no-store, no-transform",
                "{path}"
            );
            assert!(!headers.contains_key(header::ETAG), "{path}");
            let body: Value = serde_json::from_slice(&body).expect("decode list response");
            let items = body.as_array().expect("list remains an array");
            assert!(!items.is_empty(), "{path} contains this user's membership");
            if path == "/api/workspaces" {
                assert!(
                    items
                        .iter()
                        .any(|item| item["id"].as_str() == Some(own.as_str())),
                    "own space"
                );
                assert!(
                    !items
                        .iter()
                        .any(|item| item["id"].as_str() == Some(foreign.as_str())),
                    "foreign space"
                );
            }
        }
    }
}
