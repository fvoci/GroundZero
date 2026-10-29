//! 기록·학습 API의 자원 경계와 칸 검증 계약.

use fvoci_records::RecordsApi;
use utoipa::OpenApi;

#[test]
fn learning_reads_publish_separate_resources() -> Result<(), serde_json::Error> {
    let api = serde_json::to_value(RecordsApi::openapi())?;
    for path in [
        "/api/items/{item_id}/episodes",
        "/api/items/{item_id}/episode-progress",
        "/api/items/{item_id}/study/units",
        "/api/items/{item_id}/study/progress",
    ] {
        assert!(
            api["paths"][path]["get"].is_object(),
            "missing resource GET {path}"
        );
    }
    for (schema, bundled) in [
        ("Course", "episodes"),
        ("StudyPlan", "units"),
        ("Episode", "mine"),
        ("StudyUnit", "mine"),
    ] {
        assert!(
            api["components"]["schemas"][schema]["properties"][bundled].is_null(),
            "{schema} still bundles {bundled}"
        );
    }
    Ok(())
}

#[test]
fn writing_titles_keep_the_fixed_300_character_contract() -> Result<(), serde_json::Error> {
    let api = serde_json::to_value(RecordsApi::openapi())?;
    for schema in [
        "NewRecord",
        "EditRecord",
        "NewEpisode",
        "UpdateEpisode",
        "NewStudyUnit",
        "UpdateStudyUnit",
    ] {
        assert_eq!(
            api["components"]["schemas"][schema]["properties"]["title"]["maxLength"], 300,
            "{schema} title must keep its fixed limit independent of configuration"
        );
    }
    assert_eq!(
        api["components"]["schemas"]["AddEpisodes"]["properties"]["episodes"]["minItems"],
        1
    );
    assert_eq!(
        api["components"]["schemas"]["AddEpisodes"]["properties"]["episodes"]["maxItems"],
        1000
    );
    Ok(())
}

#[test]
fn expected_user_is_documented_as_optional_on_every_command() -> Result<(), serde_json::Error> {
    let api = serde_json::to_value(RecordsApi::openapi())?;
    let paths = api["paths"].as_object();
    assert!(paths.is_some());
    for (path, methods) in paths.into_iter().flatten() {
        for method in ["post", "put", "patch", "delete"] {
            let operation = &methods[method];
            if operation.is_null() {
                continue;
            }
            let header = operation["parameters"].as_array().and_then(|parameters| {
                parameters
                    .iter()
                    .find(|parameter| parameter["name"] == "x-fvoci-expected-user")
            });
            assert!(
                header.is_some(),
                "missing expected-user contract on {method} {path}"
            );
            assert_eq!(
                header.map(|value| &value["required"]),
                Some(&serde_json::json!(false)),
                "expected-user must remain optional on {method} {path}"
            );
        }
    }
    Ok(())
}

#[test]
fn study_plan_entries_treat_an_omitted_date_like_null() -> Result<(), serde_json::Error> {
    use fvoci_records::study::PlanUnits;
    use serde_json::json;

    let api = serde_json::to_value(RecordsApi::openapi())?;
    assert_eq!(
        api["components"]["schemas"]["PlanEntry"]["required"],
        json!(["unit_id"]),
        "the plan date must remain optional in the request contract"
    );

    let unit_id = "01900000-0000-7000-8000-000000000001";
    let with_null: PlanUnits = serde_json::from_value(json!({
        "entries": [{ "unit_id": unit_id, "planned_on": null }]
    }))?;
    let omitted: PlanUnits = serde_json::from_value(json!({
        "entries": [{ "unit_id": unit_id }]
    }))?;

    // The plan handler passes these serialized entries to app_plan_units.
    let expected = json!([{ "unit_id": unit_id, "planned_on": null }]);
    assert_eq!(serde_json::to_value(&with_null.entries)?, expected);
    assert_eq!(
        serde_json::to_value(&omitted.entries)?,
        expected,
        "omitting the date must clear the personal plan just like explicit null"
    );
    Ok(())
}
