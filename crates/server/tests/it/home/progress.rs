//! D-120·D-19와 집계 읽기의 HTTP 계약.
use crate::common::{self, send};
use crate::sql_support::{World, fixture, made, one};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

async fn home(w: &World, user: &common::TestUser) -> Value {
    let response = send(
        &common::app(&w.state),
        Method::GET,
        &format!("/api/me/home?organization_id={}&tz=UTC", w.org),
        Some(user),
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK, "{:?}", response.json);
    response.json
}

#[tokio::test]
async fn progress_keeps_course_completion_and_study_amount_units_separate_for_each_user() {
    let course = World::of_kind("course").await;
    let me = course.member("member").await;
    one(&course.state, &course.owner,
        "SELECT fvoci.app_add_episodes($1,'[{\"request_id\":\"0197d9b0-0000-7000-8000-000000000001\",\"title\":\"first\"},{\"request_id\":\"0197d9b0-0000-7000-8000-000000000002\",\"title\":\"second\"}]',@ids)",
        &[course.item]).await;
    fixture(
        "INSERT INTO fvoci.episode_progress(episode_id,user_id,status,watch_count)
             SELECT id,$2,'done',3 FROM fvoci.course_episodes WHERE item_id=$1 AND number=1",
        &[course.item, me.id.0],
    )
    .await;
    fixture(
        "INSERT INTO fvoci.episode_progress(episode_id,user_id,status,watch_count)
             SELECT id,$2,'done',1 FROM fvoci.course_episodes WHERE item_id=$1",
        &[course.item, course.owner.id.0],
    )
    .await;
    assert_eq!(
        home(&course, &me).await["active_items"][0]["progress"],
        json!({"kind":"course","done":1,"total":2})
    );
    assert_eq!(
        home(&course, &course.owner).await["active_items"][0]["progress"],
        json!({"kind":"course","done":2,"total":2})
    );

    let study = World::of_kind("study").await;
    let learner = study.member("member").await;
    one(&study.state, &study.owner,
        "SELECT fvoci.app_add_study_units($1,'[{\"request_id\":\"0197d9b0-0000-7000-8000-000000000003\",\"title\":\"pages\",\"unit\":\"pages\",\"amount\":20},{\"request_id\":\"0197d9b0-0000-7000-8000-000000000004\",\"title\":\"problems\",\"unit\":\"problems\",\"amount\":10}]',@ids)",
        &[study.item]).await;
    fixture(
        "INSERT INTO fvoci.unit_progress(unit_id,user_id,status,done_amount)
             SELECT id,$2,'in_progress',5 FROM fvoci.study_units WHERE item_id=$1 AND unit='pages'",
        &[study.item, learner.id.0],
    )
    .await;
    fixture(
        "INSERT INTO fvoci.unit_progress(unit_id,user_id,status,done_amount)
             SELECT id,$2,'done',0 FROM fvoci.study_units WHERE item_id=$1 AND unit='problems'",
        &[study.item, learner.id.0],
    )
    .await;
    assert_eq!(
        home(&study, &learner).await["active_items"][0]["progress"],
        json!({
            "kind":"study","amounts":[{"unit":"pages","done":5,"target":20},{"unit":"problems","done":10,"target":10}]
        })
    );
    assert_eq!(
        home(&study, &study.owner).await["active_items"][0]["progress"],
        json!({
            "kind":"study","amounts":[{"unit":"pages","done":0,"target":20},{"unit":"problems","done":0,"target":10}]
        })
    );
}

#[tokio::test]
async fn task_aggregate_http_groups_and_counts_only_assigned_tasks_and_validates_the_day() {
    let w = World::new().await;
    let me = w.member("member").await;
    let unassigned = w.task().await;
    let assigned = made(&w.state, &w.owner,
        "SELECT fvoci.app_create_task($1,$2,'mine',NULL,'','할 일','none',NULL,NULL,NULL,'{}',ARRAY[$3],false,@ids) AS id",
        &[w.item,Uuid::now_v7(),me.id.0]).await;
    let app = common::app(&w.state);
    let base = format!("/api/me/tasks?organization_id={}", w.org);
    let response = send(
        &app,
        Method::GET,
        &format!("{base}&today=2026-10-28"),
        Some(&me),
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK, "{:?}", response.json);
    assert_eq!(
        response.json["counts"],
        json!({"overdue":0,"today":0,"upcoming":0,"undated":1,"done":0})
    );
    let tasks = response.json["workspaces"][0]["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1, "unassigned task {unassigned}");
    assert_eq!(tasks[0]["id"], assigned.to_string());
    assert_eq!(tasks[0]["bucket"], "none");
    assert_eq!(home(&w, &me).await["my_tasks"], response.json);
    let invalid = send(
        &app,
        Method::GET,
        &format!("{base}&today=2026-13-99"),
        Some(&me),
        None,
    )
    .await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid.json["field"], "today");
    assert_eq!(invalid.json["reason"], "invalid_format");
    let denied = send(
        &app,
        Method::GET,
        &format!("/api/me/home?organization_id={}", Uuid::now_v7()),
        Some(&me),
        None,
    )
    .await;
    assert_eq!(denied.status, StatusCode::NOT_FOUND);
}
