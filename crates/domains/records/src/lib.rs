//! 기록·학습 도메인: 기록·강좌·스터디·Zotero 가져오기.

pub mod courses;
pub mod records;
mod state;
pub mod study;
pub mod zotero;
pub use state::RecordsState;

use axum::{
    Router,
    extract::FromRef,
    routing::{get, post, put},
};
use utoipa::OpenApi;

#[derive(Debug, OpenApi)]
#[openapi(
    paths(
        study::get,
        study::add_units,
        study::update_unit,
        study::delete_unit,
        study::set_progress,
        study::plan,
        study::participants,
        zotero::list_collections,
        zotero::import,
        records::kinds,
        records::list,
        records::create,
        records::update,
        records::delete,
        courses::get,
        courses::set_details,
        courses::add_episodes,
        courses::update_episode,
        courses::delete_episode,
        courses::set_progress,
        courses::participants,
    ),
    components(schemas(
        study::MyUnitProgress,
        study::StudyUnit,
        study::StudyPlan,
        study::NewStudyUnit,
        study::AddStudyUnits,
        study::UnitsAdded,
        study::UpdateStudyUnit,
        study::SetUnitProgress,
        study::PlanEntry,
        study::PlanUnits,
        study::UnitsPlanned,
        study::StudyParticipant,
        zotero::ZoteroLibrary,
        zotero::ZoteroCollection,
        zotero::ZoteroImport,
        zotero::ZoteroImported,
        records::FieldType,
        records::ChoiceDef,
        records::FieldDef,
        records::RecordKindDef,
        records::Record,
        records::RecordFile,
        records::RecordFileInput,
        records::NewRecord,
        records::EditRecord,
        records::RecordVersion,
        courses::CourseDetails,
        courses::MyProgress,
        courses::Episode,
        courses::Course,
        courses::NewEpisode,
        courses::AddEpisodes,
        courses::EpisodesAdded,
        courses::UpdateEpisode,
        courses::SetProgress,
        courses::Participant,
    ))
)]
pub struct RecordsApi;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    RecordsState: FromRef<S>,
{
    Router::new()
        .route("/record-kinds", get(records::kinds))
        .route("/zotero/collections", post(zotero::list_collections))
        .route("/items/{item_id}/zotero/import", post(zotero::import))
        .route("/items/{item_id}/study", get(study::get))
        .route("/items/{item_id}/study/units", post(study::add_units))
        .route("/items/{item_id}/study/plan", put(study::plan))
        .route(
            "/items/{item_id}/study/participants",
            get(study::participants),
        )
        .route(
            "/study-units/{unit_id}",
            put(study::update_unit).delete(study::delete_unit),
        )
        .route("/study-units/{unit_id}/progress", put(study::set_progress))
        .route(
            "/items/{item_id}/records",
            get(records::list).post(records::create),
        )
        .route(
            "/records/{record_id}",
            put(records::update).delete(records::delete),
        )
        .route(
            "/items/{item_id}/course",
            get(courses::get).put(courses::set_details),
        )
        .route(
            "/items/{item_id}/course/participants",
            get(courses::participants),
        )
        .route("/items/{item_id}/episodes", post(courses::add_episodes))
        .route(
            "/episodes/{episode_id}",
            put(courses::update_episode).delete(courses::delete_episode),
        )
        .route(
            "/episodes/{episode_id}/progress",
            put(courses::set_progress),
        )
}
