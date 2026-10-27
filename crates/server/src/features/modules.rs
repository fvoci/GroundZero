//! 모듈 인스턴스·키·항목·항목 아래 문서 (D-14, D-15, D-16, D-17, D-18, D-48, D-59).
//!
//! 모듈은 같은 기능 유형을 여러 개 둘 수 있다(예: DEV·OPS 둘 다 프로젝트). 기능 유형은 만든 뒤 바꾸지 않는다.
//! 키는 워크스페이스 안에서 대소문자 구분 없이 유일하고, 과거 키도 다른 모듈이 다시 쓰지 못한다.
//! 번호는 모듈마다 하나의 번호열을 항목·문서·작업이 함께 쓴다. 할당은 DB 함수 `app_allocate_number`만 한다.
//! 참조 문자열(`DEV-3`)은 저장하지 않고 현재 키로 만든다. 링크는 내부 id 기준이다.
//!
//! 권한은 `migrations/202610250001_initial_state.sql`의 DB 함수가 판정한다. 모듈 관리는 관리자, 항목 생성은 작성자 이상,
//! 항목 권한은 `app_access`, 항목 아래 문서는 `app_access`(항목 권한과 문서 규칙 중 낮은 쪽).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::Grade;
use crate::db::{UserTx, is_unique_violation, needed_ids, new_ids};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::documents::{
    Access, CreateDocument, Document, DocumentSummary, NewDocumentPlace, SCHEMA_VERSION,
    SUMMARY_COLUMNS, SummaryRow, TREE_ORDER, initial_body, load_with_access, max_depth,
    to_document, to_summaries,
};
use crate::features::workspaces::validate_title;

time::serde::format_description!(item_day, Date, "[year]-[month]-[day]");
use crate::http::AppState;
use crate::ids::{DocumentId, ItemId, ModuleId, WorkspaceId};

/// 설명 글 상한(문자). 제목보다 길게 받지만 본문은 문서로 쓴다.
const MAX_DESCRIPTION_CHARS: usize = 2_000;

/// 모듈 기능 유형 (D-14). 만든 뒤 바꾸지 않는다 (D-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleKind {
    Project,
    Research,
    Study,
    Course,
    Hobby,
}

impl ModuleKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Research => "research",
            Self::Study => "study",
            Self::Course => "course",
            Self::Hobby => "hobby",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "project" => Some(Self::Project),
            "research" => Some(Self::Research),
            "study" => Some(Self::Study),
            "course" => Some(Self::Course),
            "hobby" => Some(Self::Hobby),
            _ => None,
        }
    }
}

/// 모듈 인스턴스.
#[derive(Debug, Serialize, ToSchema)]
pub struct Module {
    pub id: ModuleId,
    pub workspace_id: WorkspaceId,
    pub kind: ModuleKind,
    pub name: String,
    /// 현재 키. 예: `DEV`.
    pub key: String,
    /// 예전 키. `@` 검색과 옛 참조 해석에 쓴다 (D-18).
    pub previous_keys: Vec<String>,
    /// 보관됨: 읽기 전용이며 번호·키는 그대로 남는다.
    pub archived: bool,
    /// 아이콘(그림 문자 등, 없으면 빈 문자열).
    pub icon: String,
    /// 설명(관리자가 쓰는 글, 없으면 빈 문자열).
    pub description: String,
    /// 현재 사용자의 접근 등급(D-113, `app_access('module', id)`): read·write·owner.
    pub access: Grade,
}

/// 모듈 생성 요청(관리자).
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateModule {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub kind: ModuleKind,
    pub name: String,
    /// 2~10자 영문·숫자, 영문으로 시작. 소문자는 대문자로 바꾼다. `WIKI`는 예약.
    pub key: String,
}

/// 모듈 이름·키 변경 요청(관리자).
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateModule {
    pub name: String,
    pub key: String,
}

/// 보관 상태 변경 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetArchived {
    pub archived: bool,
}

/// 모듈 항목 (D-14). 새 항목은 기본 비공개다 (D-20).
#[derive(Debug, Serialize, ToSchema)]
pub struct Item {
    pub id: ItemId,
    pub module_id: ModuleId,
    pub workspace_id: WorkspaceId,
    /// 예: `DEV-3`. 키를 볼 수 없으면 없다.
    pub reference: Option<String>,
    pub title: String,
    pub description: String,
    /// `workspace` 또는 `private`.
    pub visibility: String,
    pub access: Access,
    pub archived: bool,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    /// 항목 상태: `planned`·`active`·`on_hold`·`done`·`cancelled`.
    pub status: String,
    #[serde(with = "item_day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub starts_on: Option<Date>,
    #[serde(with = "item_day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub ends_on: Option<Date>,
    /// 재사용 템플릿으로 표시됨(D-33).
    pub is_template: bool,
    /// 항목 정보(제목·설명·공개 범위)의 판. 고칠 때 읽은 판을 `base_version`으로 보낸다(D-104). 항목 정보 패널(`profile`)의
    /// 판과는 따로다.
    pub version: i32,
}

/// 항목 생성 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateItem {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// 항목 변경 요청. 공개 범위는 항목 소유 권한자(만든 사람·관리자)만 바꾼다. `base_version`(읽은 `version`)이 지났으면
/// 409와 서버 최신본(D-104).
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateItem {
    pub base_version: i32,
    pub title: String,
    pub description: String,
    /// `workspace` 또는 `private`. 바꿀 때만 보낸다. 없거나 null이면 지금 값 그대로다(연 뒤 다른 소유 권한자가 바꾼
    /// 공개 범위를 설명만 고친 저장이 되돌리지 않게, 감사 W13).
    #[serde(default)]
    pub visibility: Option<String>,
}

/// 키 정규화: 앞뒤 공백 제거, 대문자. 형식이 틀리면 400 (D-18).
pub fn normalize_key(raw: &str) -> Result<String, AppError> {
    let key = raw.trim().to_ascii_uppercase();
    let mut chars = key.chars();
    let starts_with_letter = chars.next().is_some_and(|c| c.is_ascii_uppercase());
    let rest_ok = chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if !(starts_with_letter && rest_ok && (2..=10).contains(&key.len())) {
        return Err(AppError::BadRequest(
            "key must be 2-10 letters or digits starting with a letter".into(),
        ));
    }
    if key == "WIKI" {
        return Err(AppError::BadRequest("WIKI is reserved".into()));
    }
    Ok(key)
}

fn validate_description(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(AppError::BadRequest(format!(
            "description must be at most {MAX_DESCRIPTION_CHARS} characters"
        )));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(AppError::BadRequest(
            "description must not contain control characters".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

// ---------------------------------------------------------------------------
// 읽기
// ---------------------------------------------------------------------------

type ModuleRow = (
    ModuleId,
    WorkspaceId,
    String,
    String,
    String,
    bool,
    Vec<String>,
    String,
    String,
    String,
);

const MODULE_SELECT: &str =
    "SELECT m.id, m.workspace_id, m.kind, m.name, k.key, m.archived_at IS NOT NULL,
        coalesce((SELECT array_agg(o.key ORDER BY o.created_at) FROM module_keys o
                  WHERE o.module_id = m.id AND o.retired_at IS NOT NULL), '{}'),
        m.icon, m.description, app_access('module', m.id)
 FROM modules m JOIN module_keys k ON k.module_id = m.id AND k.retired_at IS NULL";

fn to_module(
    (id, workspace_id, kind, name, key, archived, previous_keys, icon, description, access): ModuleRow,
) -> Result<Module, AppError> {
    let kind = ModuleKind::parse(&kind)
        .ok_or_else(|| AppError::Config(format!("unknown module kind in database: {kind}")))?;
    Ok(Module {
        id,
        workspace_id,
        kind,
        name,
        key,
        previous_keys,
        archived,
        icon,
        description,
        access: Grade::parse_present(&access)?,
    })
}

async fn load_module(tx: &mut UserTx, id: ModuleId) -> Result<Module, AppError> {
    let row: Option<ModuleRow> = sqlx::query_as(&format!("{MODULE_SELECT} WHERE m.id = $1"))
        .bind(id)
        .fetch_optional(tx.conn())
        .await?;
    to_module(row.ok_or(AppError::NotFound)?)
}

type ItemRow = (
    ItemId,
    ModuleId,
    WorkspaceId,
    Option<String>,
    String,
    String,
    String,
    String,
    bool,
    OffsetDateTime,
    String,
    Option<Date>,
    Option<Date>,
    bool,
    i32,
);

const ITEM_SELECT: &str =
    "SELECT i.id, i.module_id, i.workspace_id, app_item_reference(i.id), i.title,
        i.description, i.visibility, app_access('item', i.id), i.archived_at IS NOT NULL, i.updated_at,
        i.status, i.starts_on, i.ends_on, i.is_template, i.version
 FROM items i";

fn to_item(
    (
        id,
        module_id,
        workspace_id,
        reference,
        title,
        description,
        visibility,
        access,
        archived,
        updated_at,
        status,
        starts_on,
        ends_on,
        is_template,
        version,
    ): ItemRow,
) -> Result<Item, AppError> {
    let access = Access::parse_present(&access)?;
    Ok(Item {
        id,
        module_id,
        workspace_id,
        reference,
        title,
        description,
        visibility,
        access,
        archived,
        updated_at,
        status,
        starts_on,
        ends_on,
        is_template,
        version,
    })
}

/// 항목을 읽는다. 볼 수 없으면(RLS) 404.
async fn load_item(tx: &mut UserTx, id: ItemId) -> Result<Item, AppError> {
    let row: Option<ItemRow> = sqlx::query_as(&format!("{ITEM_SELECT} WHERE i.id = $1"))
        .bind(id)
        .fetch_optional(tx.conn())
        .await?;
    to_item(row.ok_or(AppError::NotFound)?)
}

/// 워크스페이스의 모듈 목록(멤버).
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/modules",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Module]), (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn list_modules(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<Vec<Module>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    crate::access::require_visible(&mut tx, crate::access::Kind::Workspace, ws).await?;
    let rows: Vec<ModuleRow> = sqlx::query_as(&format!(
        "{MODULE_SELECT} WHERE m.workspace_id = $1 ORDER BY m.created_at, m.id"
    ))
    .bind(ws)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(to_module)
        .collect::<Result<_, _>>()
        .map(Json)
}

/// 모듈 하나(멤버).
#[utoipa::path(get, path = "/api/modules/{module_id}",
    params(("module_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Module), (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn get_module(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ModuleId>,
) -> Result<Json<Module>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let module = load_module(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(module))
}

// ---------------------------------------------------------------------------
// 모듈 관리(관리자)
// ---------------------------------------------------------------------------

/// 모듈 생성(관리자). 키가 쓰였거나 예전에 쓰였으면 409.
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/modules",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = CreateModule,
    responses((status = 201, body = Module), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "modules")]
pub async fn create_module(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<CreateModule>,
) -> Result<(StatusCode, Json<Module>), AppError> {
    let name = validate_title(&input.name, state.config.limits.max_title_chars, "name")?;
    let key = normalize_key(&input.key)?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let id: ModuleId = sqlx::query_scalar("SELECT app_create_module($1, $2, $3, $4, $5, $6)")
            .bind(ws)
            .bind(input.request_id)
            .bind(input.kind.as_str())
            .bind(&name)
            .bind(&key)
            .bind(new_ids(2))
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        let module = load_module(&mut tx, id).await?;
        tx.commit().await?;
        Ok(module)
    };
    match run().await {
        Ok(m) => Ok((StatusCode::CREATED, Json(m))),
        // 같은 요청이나 같은 키가 동시에 들어왔다. 다시 부르면 같은 모듈 또는 409를 받는다.
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 모듈 이름·키 변경(관리자). 옛 키는 예전 키로 남아 다른 모듈이 쓰지 못한다.
#[utoipa::path(put, path = "/api/modules/{module_id}",
    params(("module_id" = String, Path, format = Uuid)), request_body = UpdateModule,
    responses((status = 200, body = Module), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "modules")]
pub async fn update_module(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ModuleId>,
    Json(input): Json<UpdateModule>,
) -> Result<Json<Module>, AppError> {
    let name = validate_title(&input.name, state.config.limits.max_title_chars, "name")?;
    let key = normalize_key(&input.key)?;
    let mut tx = state.db.begin_as(user).await?;
    let result = sqlx::query("SELECT app_update_module($1, $2, $3, $4)")
        .bind(id)
        .bind(&name)
        .bind(&key)
        .bind(new_ids(2))
        .execute(tx.conn())
        .await;
    match result {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            return Err(AppError::Conflict(
                "this key is in use or was used before".into(),
            ));
        }
        Err(e) => return Err(AppError::from_db_function(e)),
    }
    let module = load_module(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(module))
}

/// 모듈 보관·해제(관리자). 보관하면 항목과 문서가 읽기 전용이 된다.
#[utoipa::path(put, path = "/api/modules/{module_id}/archived",
    params(("module_id" = String, Path, format = Uuid)), request_body = SetArchived,
    responses((status = 200, body = Module), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn archive_module(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ModuleId>,
    Json(input): Json<SetArchived>,
) -> Result<Json<Module>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_archive_module($1, $2, $3)")
        .bind(id)
        .bind(input.archived)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let module = load_module(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(module))
}

/// 모듈 설명 바꾸기 요청(관리자).
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetModuleDescription {
    /// 빈 문자열이면 지운다.
    pub description: String,
}

/// 모듈 설명 바꾸기(관리자, D-78).
#[utoipa::path(put, path = "/api/modules/{module_id}/description", operation_id = "set_module_description",
    params(("module_id" = String, Path, format = Uuid)), request_body = SetModuleDescription,
    responses((status = 200, body = Module), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn set_description(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ModuleId>,
    Json(input): Json<SetModuleDescription>,
) -> Result<Json<Module>, AppError> {
    let description = validate_description(&input.description)?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_module_description($1, $2)")
        .bind(id)
        .bind(&description)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let module = load_module(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(module))
}

// ---------------------------------------------------------------------------
// 항목
// ---------------------------------------------------------------------------

/// 모듈의 항목 목록. 볼 수 있는 항목만(비공개 항목은 만든 사람·관리자에게만) 나온다.
#[utoipa::path(get, path = "/api/modules/{module_id}/items",
    params(("module_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Item]), (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn list_items(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ModuleId>,
) -> Result<Json<Vec<Item>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    load_module(&mut tx, id).await?;
    let rows: Vec<ItemRow> = sqlx::query_as(&format!(
        "{ITEM_SELECT} WHERE i.module_id = $1 ORDER BY i.created_at, i.id"
    ))
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(to_item)
        .collect::<Result<_, _>>()
        .map(Json)
}

/// 항목 생성(작성자 이상). 모듈 번호열에서 번호를 받는다. 같은 `request_id`는 같은 항목을 돌려준다.
#[utoipa::path(post, path = "/api/modules/{module_id}/items",
    params(("module_id" = String, Path, format = Uuid)), request_body = CreateItem,
    responses((status = 201, body = Item), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "modules")]
pub async fn create_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(module): Path<ModuleId>,
    Json(input): Json<CreateItem>,
) -> Result<(StatusCode, Json<Item>), AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let description = validate_description(&input.description)?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let id: ItemId = sqlx::query_scalar("SELECT app_create_item($1, $2, $3, $4, $5)")
            .bind(module)
            .bind(input.request_id)
            .bind(&title)
            .bind(&description)
            .bind(new_ids(6))
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        let item = load_item(&mut tx, id).await?;
        tx.commit().await?;
        Ok(item)
    };
    match run().await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 항목 하나.
#[utoipa::path(get, path = "/api/items/{item_id}",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Item), (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn get_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ItemId>,
) -> Result<Json<Item>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let item = load_item(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(item))
}

/// 409 응답: 판이 지난 항목 정보 저장. 서버 최신본.
#[derive(Debug, Serialize, ToSchema)]
pub struct ItemConflict {
    pub error: &'static str,
    pub current: Item,
}

/// 항목 변경의 409 본문(계약 문서용). 응답은 두 모양 가운데 하나를 그대로 보낸다(이 타입으로 감싸지 않는다).
/// - `ItemConflict`(`error: "conflict"`, `current`): `base_version`이 지남.
/// - `ErrorBody`(`current` 없음): `invalid_state`(보관된 항목, 동시 변경 재시도 등).
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum ItemSaveConflict {
    Stale(Box<ItemConflict>),
    State(ErrorBody),
}

/// 항목 변경. 제목·설명은 쓰기 권한, 공개 범위는 소유 권한이 필요하다. 읽은 판이 지났으면 아무것도 바꾸지 않고 409와
/// 서버 최신본(`ItemConflict`), 보관된 항목은 409 `invalid_state`.
#[utoipa::path(put, path = "/api/items/{item_id}",
    params(("item_id" = String, Path, format = Uuid)), request_body = UpdateItem,
    responses((status = 200, body = Item), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody),
              (status = 409, body = ItemSaveConflict,
               description = "`conflict`면 ItemConflict(서버 최신본 포함), 보관된 항목·동시 변경 재시도면 ErrorBody(`invalid_state`). 어느 쪽도 아무것도 바꾸지 않았다")), tag = "modules")]
pub async fn update_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ItemId>,
    Json(input): Json<UpdateItem>,
) -> Result<Response, AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let description = validate_description(&input.description)?;
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_update_item_ids($1)").bind(id),
    )
    .await?;
    let outcome: String = sqlx::query_scalar("SELECT app_update_item($1, $2, $3, $4, $5, $6)")
        .bind(id)
        .bind(input.base_version)
        .bind(&title)
        .bind(&description)
        .bind(&input.visibility)
        .bind(ids)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let item = load_item(&mut tx, id).await?;
    tx.commit().await?;
    Ok(if outcome == "conflict" {
        (
            StatusCode::CONFLICT,
            Json(ItemConflict {
                error: "conflict",
                current: item,
            }),
        )
            .into_response()
    } else {
        Json(item).into_response()
    })
}

/// 항목 보관·해제(항목 소유 권한자).
#[utoipa::path(put, path = "/api/items/{item_id}/archived",
    params(("item_id" = String, Path, format = Uuid)), request_body = SetArchived,
    responses((status = 200, body = Item), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn archive_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ItemId>,
    Json(input): Json<SetArchived>,
) -> Result<Json<Item>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_archive_item($1, $2, $3)")
        .bind(id)
        .bind(input.archived)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let item = load_item(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(item))
}

/// 항목 아래 문서 목록. 볼 수 있는 문서만 나온다.
#[utoipa::path(get, path = "/api/items/{item_id}/documents",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [DocumentSummary]), (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn list_item_documents(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ItemId>,
) -> Result<Json<Vec<DocumentSummary>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    load_item(&mut tx, id).await?;
    let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM documents WHERE item_id = $1 ORDER BY {TREE_ORDER}"
    ))
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(to_summaries(rows)))
}

/// 항목 아래 문서 생성(항목 쓰기 권한). 모듈 번호열에서 번호를 받는다.
#[utoipa::path(post, path = "/api/items/{item_id}/documents",
    params(("item_id" = String, Path, format = Uuid)), request_body = CreateDocument,
    responses((status = 201, body = Document), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "modules")]
pub async fn create_item_document(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<CreateDocument>,
) -> Result<(StatusCode, Json<Document>), AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let body = initial_body(&mut tx, input.template_id, NewDocumentPlace::Item(item)).await?;
        let id: DocumentId =
            sqlx::query_scalar("SELECT app_create_module_document($1, $2, $3, $4, $5, $6, $7, $8)")
                .bind(item)
                .bind(input.request_id)
                .bind(&title)
                .bind(SCHEMA_VERSION)
                .bind(body)
                .bind(input.parent_id)
                .bind(max_depth(&state.config.limits))
                .bind(new_ids(4))
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
        let (row, access) = load_with_access(&mut tx, id).await?;
        tx.commit().await?;
        Ok(to_document(row, access))
    };
    match run().await {
        Ok(doc) => Ok((StatusCode::CREATED, Json(doc))),
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_normalized_to_uppercase() {
        assert_eq!(normalize_key(" dev ").ok().as_deref(), Some("DEV"));
        assert_eq!(normalize_key("Ops2").ok().as_deref(), Some("OPS2"));
    }

    #[test]
    fn keys_reject_bad_shapes() {
        for bad in ["", "D", "1AB", "DE-V", "ABCDEFGHIJK", "wiki", "가나"] {
            assert!(normalize_key(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn module_kind_round_trip() {
        for kind in [
            ModuleKind::Project,
            ModuleKind::Research,
            ModuleKind::Study,
            ModuleKind::Course,
            ModuleKind::Hobby,
        ] {
            assert_eq!(ModuleKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn description_allows_newlines_only() {
        assert!(validate_description("첫 줄\n둘째 줄").is_ok());
        assert!(validate_description("a\u{0007}b").is_err());
        assert!(validate_description(&"x".repeat(MAX_DESCRIPTION_CHARS + 1)).is_err());
    }
}
