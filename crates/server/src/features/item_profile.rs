//! 항목 정보와 항목 멤버 (D-33·D-34·D-36·D-38, D-20 공유). 리더·목표·범위·기간·상태와 모듈 종류별 정보,
//! 비공개 항목을 함께 쓰는 사람·그룹. 권한 판정은 DB의 정의자 함수(`app_access`)가 한다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::{UserTx, is_unique_violation, needed_ids, new_ids};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::modules::ModuleKind;
use crate::http::AppState;
use crate::ids::{ItemId, ModuleId, UserId};

time::serde::format_description!(day, Date, "[year]-[month]-[day]");

const MAX_FIELD: usize = 2000;
const MAX_KEYWORDS: usize = 30;

/// 모듈 종류별 항목 정보. 종류에 없는 칸을 보내면 400.
/// - 연구(D-34): 분야·키워드·연구 질문·가설·모형·방법·표본
/// - 스터디(D-36): 수준·목표 분량
/// - 취미(D-38): 활동 유형
#[derive(Debug, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ItemDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hypothesis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// 목표 분량(예: "교재 3권, 문제 600개").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_type: Option<String>,
}

impl ItemDetails {
    /// 앞뒤 공백을 걷고 빈 칸은 없앤다. 종류에 없는 칸·너무 긴 값은 400.
    fn normalize(self, kind: ModuleKind) -> Result<Self, AppError> {
        let text =
            |v: Option<String>, name: &str, allowed: bool| -> Result<Option<String>, AppError> {
                let v = v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
                match v {
                    Some(_) if !allowed => Err(AppError::BadRequest(format!(
                        "{name} is not a field of this module kind"
                    ))),
                    Some(s) if s.chars().count() > MAX_FIELD => {
                        Err(AppError::BadRequest(format!("{name} is too long")))
                    }
                    other => Ok(other),
                }
            };
        let research = kind == ModuleKind::Research;
        let study = kind == ModuleKind::Study;
        let hobby = kind == ModuleKind::Hobby;
        let mut keywords: Vec<String> = Vec::new();
        for k in self.keywords {
            let k = k.trim().to_owned();
            if k.is_empty() || keywords.contains(&k) {
                continue;
            }
            if k.chars().count() > 100 {
                return Err(AppError::BadRequest("a keyword is too long".into()));
            }
            keywords.push(k);
        }
        if !keywords.is_empty() && !research {
            return Err(AppError::BadRequest(
                "keywords is not a field of this module kind".into(),
            ));
        }
        if keywords.len() > MAX_KEYWORDS {
            return Err(AppError::BadRequest("too many keywords".into()));
        }
        Ok(Self {
            field: text(self.field, "field", research)?,
            keywords,
            question: text(self.question, "question", research)?,
            hypothesis: text(self.hypothesis, "hypothesis", research)?,
            model: text(self.model, "model", research)?,
            method: text(self.method, "method", research)?,
            sample: text(self.sample, "sample", research)?,
            level: text(self.level, "level", study)?,
            target: text(self.target, "target", study)?,
            activity_type: text(self.activity_type, "activity_type", hobby)?,
        })
    }
}

/// 사람(표시 이름).
#[derive(Debug, Serialize, ToSchema)]
pub struct PersonRef {
    pub user_id: UserId,
    pub display_name: String,
}

/// 그룹.
#[derive(Debug, Serialize, ToSchema)]
pub struct GroupRef {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub name: String,
}

/// 항목 정보.
#[derive(Debug, Serialize, ToSchema)]
pub struct ItemProfile {
    /// 리더(표시만, 권한을 더 주지 않는다). 워크스페이스를 떠났으면 없다.
    pub leader: Option<PersonRef>,
    pub goal: String,
    pub scope: String,
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub starts_on: Option<Date>,
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub ends_on: Option<Date>,
    /// `planned`·`active`·`on_hold`·`done`·`cancelled`.
    pub status: String,
    pub details: ItemDetails,
    /// 비공개 항목을 함께 쓰는 사람(직접 지정).
    pub members: Vec<PersonRef>,
    /// 비공개 항목을 함께 쓰는 그룹.
    pub groups: Vec<GroupRef>,
    pub can_edit: bool,
    /// 멤버를 정할 수 있는지(항목 소유 권한자).
    pub can_manage_members: bool,
    /// 이 칸들(리더·목표·범위·기간·상태·종류별 정보)의 판. 고칠 때 연 판을 `base_version`으로 보낸다(D-104). 항목의
    /// 제목·설명·공개 범위의 판(`Item.version`)과는 따로다.
    pub version: i32,
}

/// 항목 정보 바꾸기(쓸 수 있는 사람). `base_version`(연 때의 `version`)이 지났으면 409와 서버 최신본(D-104).
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetItemProfile {
    pub base_version: i32,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub leader_id: Option<UserId>,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub starts_on: Option<Date>,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub ends_on: Option<Date>,
    pub status: String,
    #[serde(default)]
    pub details: ItemDetails,
}

/// 항목 멤버 정하기(항목 소유 권한자). 목록 전체를 보낸다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetItemMembers {
    #[schema(value_type = Vec<String>)]
    pub user_ids: Vec<UserId>,
    #[schema(value_type = Vec<String>)]
    pub group_ids: Vec<Uuid>,
}

/// 모듈 아이콘(관리자).
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetModuleIcon {
    pub icon: String,
}

type ProfileRow = (
    String,
    Option<UserId>,
    Option<String>,
    String,
    String,
    Option<Date>,
    Option<Date>,
    String,
    serde_json::Value,
    String,
    bool,
    i32,
);

/// 409 응답: 판이 지난 항목 정보 저장. 서버 최신본.
#[derive(Debug, Serialize, ToSchema)]
pub struct ItemProfileConflict {
    pub error: &'static str,
    pub current: ItemProfile,
}

/// 항목 정보 바꾸기의 409 본문(계약 문서용). 응답은 두 모양 가운데 하나를 그대로 보낸다(이 타입으로 감싸지 않는다).
/// - `ItemProfileConflict`(`error: "conflict"`, `current`): `base_version`이 지남.
/// - `ErrorBody`(`current` 없음): `invalid_state`(보관된 항목, 동시 변경 재시도 등).
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum ItemProfileSaveConflict {
    Stale(Box<ItemProfileConflict>),
    State(ErrorBody),
}

/// 항목 정보(볼 수 있는 사람).
#[utoipa::path(get, path = "/api/items/{item_id}/profile", operation_id = "get_item_profile",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = ItemProfile), (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn get(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<ItemProfile>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let profile = load(&mut tx, item).await?;
    tx.commit().await?;
    Ok(Json(profile))
}

/// 항목 정보를 읽는다. 볼 수 없으면(RLS) 404.
async fn load(tx: &mut UserTx, item: ItemId) -> Result<ItemProfile, AppError> {
    // 항목 RLS가 볼 수 없는 항목을 거른다. 리더 이름은 그 사람이 아직 워크스페이스 멤버일 때만.
    let row: Option<ProfileRow> = sqlx::query_as(
        "SELECT m.kind, i.leader_id,
                (SELECT u.display_name FROM users u JOIN workspace_members wm
                  ON wm.user_id = u.id AND wm.workspace_id = i.workspace_id WHERE u.id = i.leader_id),
                i.goal, i.scope, i.starts_on, i.ends_on, i.status, i.details,
                app_access('item', i.id), i.archived_at IS NOT NULL OR m.archived_at IS NOT NULL,
                i.profile_version
         FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = $1",
    )
    .bind(item)
    .fetch_optional(tx.conn())
    .await?;
    let (
        _,
        leader_id,
        leader_name,
        goal,
        scope,
        starts_on,
        ends_on,
        status,
        details,
        access,
        archived,
        version,
    ) = row.ok_or(AppError::NotFound)?;
    let members: Vec<(UserId, String)> = sqlx::query_as(
        "SELECT u.id, u.display_name FROM item_members im JOIN users u ON u.id = im.user_id
         WHERE im.item_id = $1 ORDER BY u.display_name, u.id",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    let groups: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT g.id, g.name FROM item_members im JOIN groups g ON g.id = im.group_id
         WHERE im.item_id = $1 ORDER BY g.name, g.id",
    )
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    let details: ItemDetails = serde_json::from_value(details).unwrap_or_default();
    let leader = match (leader_id, leader_name) {
        (Some(user_id), Some(display_name)) => Some(PersonRef {
            user_id,
            display_name,
        }),
        _ => None,
    };
    Ok(ItemProfile {
        leader,
        goal,
        scope,
        starts_on,
        ends_on,
        status,
        details,
        members: members
            .into_iter()
            .map(|(user_id, display_name)| PersonRef {
                user_id,
                display_name,
            })
            .collect(),
        groups: groups
            .into_iter()
            .map(|(id, name)| GroupRef { id, name })
            .collect(),
        can_edit: !archived && (access == "owner" || access == "write"),
        can_manage_members: !archived && access == "owner",
        version,
    })
}

/// 항목 정보 바꾸기(쓸 수 있는 사람). 종류에 없는 칸은 400, 리더는 그 항목을 볼 수 있는 사람이어야 한다. 연 판이
/// 지났으면 아무것도 바꾸지 않고 409와 서버 최신본(`ItemProfileConflict`), 보관된 항목은 409 `invalid_state`.
#[utoipa::path(put, path = "/api/items/{item_id}/profile", operation_id = "set_item_profile",
    params(("item_id" = String, Path, format = Uuid)), request_body = SetItemProfile,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ItemProfileSaveConflict,
               description = "`conflict`면 ItemProfileConflict(서버 최신본 포함), 보관된 항목·동시 변경 재시도면 ErrorBody(`invalid_state`). 어느 쪽도 아무것도 바꾸지 않았다")), tag = "modules")]
pub async fn set(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<SetItemProfile>,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let kind: Option<String> = sqlx::query_scalar(
        "SELECT m.kind FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = $1",
    )
    .bind(item)
    .fetch_optional(tx.conn())
    .await?;
    let kind = kind
        .as_deref()
        .and_then(ModuleKind::parse)
        .ok_or(AppError::NotFound)?;
    let details = input.details.normalize(kind)?;
    let details = serde_json::to_value(&details)
        .map_err(|e| AppError::Config(format!("item details: {e}")))?;
    let outcome: String =
        sqlx::query_scalar("SELECT app_set_item_profile($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)")
            .bind(item)
            .bind(input.base_version)
            .bind(input.leader_id)
            .bind(input.goal.trim())
            .bind(input.scope.trim())
            .bind(input.starts_on)
            .bind(input.ends_on)
            .bind(&input.status)
            .bind(details)
            .bind(new_ids(2))
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    if outcome == "conflict" {
        let current = load(&mut tx, item).await?;
        tx.commit().await?;
        return Ok((
            StatusCode::CONFLICT,
            Json(ItemProfileConflict {
                error: "conflict",
                current,
            }),
        )
            .into_response());
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// 항목 멤버 정하기(항목 소유 권한자). 비공개 항목은 멤버만 워크스페이스 역할 안에서 본다(D-20 공유).
#[utoipa::path(put, path = "/api/items/{item_id}/members", operation_id = "set_item_members",
    params(("item_id" = String, Path, format = Uuid)), request_body = SetItemMembers,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "modules")]
pub async fn set_members(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<SetItemMembers>,
) -> Result<StatusCode, AppError> {
    let users: Vec<Uuid> = input.user_ids.iter().map(|u| u.0).collect();
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_set_item_members_ids($1)").bind(item),
    )
    .await?;
    sqlx::query("SELECT app_set_item_members($1, $2, $3, $4)")
        .bind(item)
        .bind(&users)
        .bind(&input.group_ids)
        .bind(ids)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 모듈 아이콘 바꾸기(관리자). 빈 값이면 지운다.
#[utoipa::path(put, path = "/api/modules/{module_id}/icon", operation_id = "set_module_icon",
    params(("module_id" = String, Path, format = Uuid)), request_body = SetModuleIcon,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "modules")]
pub async fn set_module_icon(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(module): Path<ModuleId>,
    Json(input): Json<SetModuleIcon>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_module_icon($1, $2)")
        .bind(module)
        .bind(&input.icon)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_follow_the_module_kind() {
        let research = ItemDetails {
            field: Some(" 교육학 ".into()),
            keywords: vec!["학습".into(), " 학습 ".into(), String::new()],
            ..ItemDetails::default()
        }
        .normalize(ModuleKind::Research)
        .unwrap();
        assert_eq!(research.field.as_deref(), Some("교육학"));
        assert_eq!(research.keywords, ["학습"]);
        let wrong = ItemDetails {
            level: Some("중급".into()),
            ..ItemDetails::default()
        };
        assert!(wrong.normalize(ModuleKind::Hobby).is_err());
        let blank = ItemDetails {
            level: Some("  ".into()),
            ..ItemDetails::default()
        };
        assert_eq!(
            blank.normalize(ModuleKind::Project).unwrap(),
            ItemDetails::default()
        );
    }
}

/// 항목 복제(D-33). 같은 워크스페이스의 같은 종류 모듈 안에 비공개 새 항목을 만든다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CopyItem {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub module_id: ModuleId,
    /// 비우면 원래 제목.
    #[serde(default)]
    pub title: String,
    /// 새로 시작(템플릿 사용): 업무 상태·날짜·체크 표시와 항목 상태·기간을 비운다.
    #[serde(default)]
    pub reset: bool,
}

/// 복제 결과.
#[derive(Debug, Serialize, ToSchema)]
pub struct CopiedItem {
    pub item_id: ItemId,
    /// 복제를 허용하지 않아 건너뛴 문서 수.
    pub skipped_documents: i32,
}

/// 템플릿 표시.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetTemplate {
    pub template: bool,
}

/// 항목 복제(볼 수 있는 사람, 대상 모듈에 항목을 만들 수 있어야 함). 담당자·반복 규칙·기록·첨부·진도는 옮기지
/// 않는다. 복제를 허용하지 않은 문서는 건너뛴다(D-20).
#[utoipa::path(post, path = "/api/items/{item_id}/copy", operation_id = "copy_item",
    params(("item_id" = String, Path, format = Uuid)), request_body = CopyItem,
    responses((status = 201, body = CopiedItem), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "modules")]
pub async fn copy(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<CopyItem>,
) -> Result<(StatusCode, Json<CopiedItem>), AppError> {
    if input.title.trim().chars().count() > state.config.limits.max_title_chars {
        return Err(AppError::BadRequest("the title is too long".into()));
    }
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let ids = needed_ids(
            tx.conn(),
            sqlx::query_scalar("SELECT app_copy_item_ids($1)").bind(item),
        )
        .await?;
        let (item_id, skipped_documents): (ItemId, i32) = sqlx::query_as(
            "SELECT copied_item, skipped_count FROM app_copy_item($1, $2, $3, $4, $5, $6)",
        )
        .bind(item)
        .bind(input.module_id)
        .bind(input.request_id)
        .bind(input.title.trim())
        .bind(input.reset)
        .bind(ids)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
        tx.commit().await?;
        Ok(CopiedItem {
            item_id,
            skipped_documents,
        })
    };
    // 같은 요청 id의 앞 요청(응답을 잃고 다시 보낸 것)이 아직 끝나지 않았으면 고유 제약에서 기다린 뒤 23505로 끝난다.
    // 그때는 한 번 더 돌려 앞 요청이 만든 복제본을 돌려준다(항목 만들기와 같다, 감사 S10. 전에는 500).
    let copied = match run().await {
        Ok(copied) => copied,
        Err(AppError::Database(e)) if is_unique_violation(&e) => run().await?,
        Err(e) => return Err(e),
    };
    Ok((StatusCode::CREATED, Json(copied)))
}

/// 템플릿 표시·해제(쓸 수 있는 사람).
#[utoipa::path(put, path = "/api/items/{item_id}/template", operation_id = "set_item_template",
    params(("item_id" = String, Path, format = Uuid)), request_body = SetTemplate,
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "modules")]
pub async fn set_template(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<SetTemplate>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_item_template($1, $2, $3)")
        .bind(item)
        .bind(input.template)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
