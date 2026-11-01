//! Aggregate transport resources. SQL retains each existing resource's projection and RLS.
use fvoci_core::access::Grade;
use fvoci_core::ids::{ItemId, ModuleId, UserId, WorkspaceId};
use fvoci_core::module::ModuleKind;
use fvoci_core::recurrence::RecurrenceRule;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScreenStatusCategory {
    Todo,
    Doing,
    Done,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScreenPriority {
    None,
    Low,
    Medium,
    High,
    Urgent,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub enum ScreenLinkKind {
    #[serde(rename = "FS")]
    FinishStart,
    #[serde(rename = "SS")]
    StartStart,
    #[serde(rename = "FF")]
    FinishFinish,
    #[serde(rename = "SF")]
    StartFinish,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SectionError {
    Unavailable,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenItem {
    pub id: ItemId,
    pub module_id: ModuleId,
    pub workspace_id: WorkspaceId,
    pub reference: Option<String>,
    pub title: String,
    pub description: String,
    pub visibility: String,
    pub access: Grade,
    pub archived: bool,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type=String,format=DateTime)]
    pub updated_at: OffsetDateTime,
    pub status: String,
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub is_template: bool,
    pub version: i32,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenModule {
    pub id: ModuleId,
    pub workspace_id: WorkspaceId,
    pub kind: ModuleKind,
    pub name: String,
    pub key: String,
    pub previous_keys: Vec<String>,
    pub archived: bool,
    pub icon: String,
    pub description: String,
    pub access: Grade,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenDocument {
    pub id: Uuid,
    pub reference: Option<String>,
    pub title: String,
    pub revision: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type=String,format=DateTime)]
    pub updated_at: OffsetDateTime,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenTask {
    pub id: Uuid,
    pub item_id: ItemId,
    pub module_id: ModuleId,
    pub workspace_id: WorkspaceId,
    pub parent_id: Option<Uuid>,
    pub reference: Option<String>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub status_category: ScreenStatusCategory,
    pub priority: ScreenPriority,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub estimate_minutes: Option<i32>,
    pub tags: Vec<String>,
    pub milestone: bool,
    pub series_id: Option<Uuid>,
    pub occurrence_date: Option<String>,
    pub version: i32,
    pub move_version: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type=Option<String>,format=DateTime)]
    pub completed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type=String,format=DateTime)]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenStatus {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub position: f64,
    pub category: ScreenStatusCategory,
    pub task_count: i32,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenAssignee {
    pub task_id: Uuid,
    pub user_id: UserId,
    pub display_name: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenDependency {
    pub predecessor_id: Uuid,
    pub successor_id: Uuid,
    pub kind: ScreenLinkKind,
    #[schema(minimum = 0, maximum = 3650)]
    pub lag_days: i32,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenSeries {
    pub id: Uuid,
    pub root_task_id: Uuid,
    pub rule: Option<RecurrenceRule>,
    pub timezone: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenChecklistItem {
    pub id: Uuid,
    pub task_id: Uuid,
    pub text: String,
    pub checked: bool,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ScreenAssignable {
    pub user_id: UserId,
    pub display_name: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ReadSection<T> {
    Ready { value: T },
    Failed { error: SectionError },
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct TaskContextRead {
    pub viewer_id: UserId,
    pub item: ScreenItem,
    pub module: ScreenModule,
    pub documents: Vec<ScreenDocument>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct TaskBoardRead {
    pub viewer_id: UserId,
    pub item_id: ItemId,
    pub item_access: Grade,
    pub archived: bool,
    pub tasks: Vec<ScreenTask>,
    pub statuses: Vec<ScreenStatus>,
    pub assignees: Vec<ScreenAssignee>,
    pub dependencies: Vec<ScreenDependency>,
    pub series: Vec<ScreenSeries>,
    pub checklist: Vec<ScreenChecklistItem>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct TaskDetailRead {
    pub viewer_id: UserId,
    pub task: ScreenTask,
    pub item: ScreenItem,
    pub archived: bool,
    pub tasks: Vec<ScreenTask>,
    pub statuses: ReadSection<Vec<ScreenStatus>>,
    pub assignees: Vec<ScreenAssignee>,
    pub dependencies: Vec<ScreenDependency>,
    pub series: Vec<ScreenSeries>,
    pub checklist: Vec<ScreenChecklistItem>,
    pub assignable: ReadSection<Vec<ScreenAssignable>>,
}
