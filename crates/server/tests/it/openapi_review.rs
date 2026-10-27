use fvoci_server::http::ApiDoc;
use utoipa::OpenApi;

#[test]
fn timer_conflict_and_missing_workspace_organization_declare_error_body()
-> Result<(), serde_json::Error> {
    let doc = serde_json::to_value(ApiDoc::openapi())?;
    for (path, status) in [("/api/me/timer", "409"), ("/api/workspaces", "404")] {
        assert_eq!(
            doc["paths"][path]["post"]["responses"][status]["content"]["application/json"]["schema"],
            serde_json::json!({ "$ref": "#/components/schemas/ErrorBody" }),
            "POST {path} {status} must describe the handler's error body"
        );
    }
    Ok(())
}
