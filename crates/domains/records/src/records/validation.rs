//! 기록 칸 정규화와 참조 검증.

use super::{FieldType, RecordFileInput, RecordKindDef, record_kinds};
use fvoci_core::{error::AppError, ids::ItemId, module::ModuleKind};
use fvoci_db::UserTx;
use serde_json::{Map, Value};
use time::Date;
use uuid::Uuid;

fn bad(msg: impl Into<String>) -> AppError {
    AppError::BadRequest(msg.into())
}

fn text_value(value: &Value, key: &str, max: usize) -> Result<Option<Value>, AppError> {
    let s = value
        .as_str()
        .ok_or_else(|| bad(format!("{key} must be text")))?
        .trim();
    if s.is_empty() {
        return Ok(None);
    }
    if s.chars().count() > max {
        return Err(bad(format!("{key} is too long")));
    }
    Ok(Some(Value::String(s.to_owned())))
}

fn uuid_value(value: &Value, key: &str) -> Result<Uuid, AppError> {
    value
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| bad(format!("{key} must be an id")))
}

fn normalize_tags(value: &Value, key: &str) -> Result<Option<Value>, AppError> {
    let list = value
        .as_array()
        .ok_or_else(|| bad(format!("{key} must be a list")))?;
    let mut tags: Vec<String> = Vec::new();
    for t in list {
        if let Some(Value::String(s)) = text_value(t, key, 100)?
            && !tags.contains(&s)
        {
            tags.push(s);
        }
    }
    if tags.len() > 50 {
        return Err(bad(format!("{key} has too many tags")));
    }
    Ok((!tags.is_empty()).then(|| serde_json::json!(tags)))
}

fn normalize_people(
    value: &Value,
    key: &str,
    people: &mut Vec<Uuid>,
) -> Result<Option<Value>, AppError> {
    let list = value
        .as_array()
        .ok_or_else(|| bad(format!("{key} must be a list")))?;
    let mut ids: Vec<Uuid> = Vec::new();
    for v in list {
        let id = uuid_value(v, key)?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.len() > 200 {
        return Err(bad(format!("{key} has too many people")));
    }
    people.extend(&ids);
    Ok((!ids.is_empty()).then(|| serde_json::json!(ids)))
}

/// 칸을 정의대로 확인·정리한다. 사람·강좌 칸의 id는 따로 모아 DB에서 확인한다.
fn normalize_fields(
    def: &RecordKindDef,
    fields: Map<String, Value>,
    people: &mut Vec<Uuid>,
    courses: &mut Vec<Uuid>,
    episodes: &mut Vec<Uuid>,
) -> Result<Value, AppError> {
    let mut out = Map::new();
    for (key, value) in fields {
        let field = def
            .fields
            .iter()
            .find(|f| f.key == key)
            .ok_or_else(|| bad(format!("{key} is not a field of {}", def.kind)))?;
        if value.is_null() {
            continue;
        }
        let normalized = match field.field_type {
            FieldType::Text => text_value(&value, &key, 300)?,
            FieldType::LongText => text_value(&value, &key, 10_000)?,
            FieldType::Date => {
                let s = value.as_str().unwrap_or_default();
                if s.is_empty() {
                    None
                } else {
                    Date::parse(
                        s,
                        &time::macros::format_description!("[year]-[month]-[day]"),
                    )
                    .map_err(|_| bad(format!("{key} must be YYYY-MM-DD")))?;
                    Some(Value::String(s.to_owned()))
                }
            }
            FieldType::Number => {
                let n = value
                    .as_f64()
                    .filter(|n| n.is_finite() && (0.0..=1_000_000_000.0).contains(n))
                    .ok_or_else(|| bad(format!("{key} must be a number from 0")))?;
                Some(serde_json::json!(n))
            }
            FieldType::Choice => {
                let s = value.as_str().unwrap_or_default();
                if s.is_empty() {
                    None
                } else if field.choices.iter().any(|c| c.value == s) {
                    Some(Value::String(s.to_owned()))
                } else {
                    return Err(bad(format!("{key} has no choice {s}")));
                }
            }
            FieldType::Url => match text_value(&value, &key, 2000)? {
                Some(Value::String(s)) => {
                    let lower = s.to_ascii_lowercase();
                    if !(lower.starts_with("http://") || lower.starts_with("https://"))
                        || s.chars().any(char::is_whitespace)
                    {
                        return Err(bad(format!("{key} must start with http:// or https://")));
                    }
                    Some(Value::String(s))
                }
                other => other,
            },
            FieldType::Tags => normalize_tags(&value, &key)?,
            FieldType::Person => {
                let id = uuid_value(&value, &key)?;
                people.push(id);
                Some(Value::String(id.to_string()))
            }
            FieldType::People => normalize_people(&value, &key, people)?,
            FieldType::Course => {
                let id = uuid_value(&value, &key)?;
                courses.push(id);
                Some(Value::String(id.to_string()))
            }
            FieldType::Episode => {
                let id = uuid_value(&value, &key)?;
                episodes.push(id);
                Some(Value::String(id.to_string()))
            }
        };
        if let Some(v) = normalized {
            out.insert(key, v);
        }
    }
    Ok(Value::Object(out))
}

/// 칸에 든 참조 id(사람·강좌·회차).
type FieldIds = (Vec<Uuid>, Vec<Uuid>, Vec<Uuid>);

/// 칸마다 사람·강좌·회차 id를 모은다. 정의와 맞지 않는 칸은 건너뛴다(그 칸의 확인은 `normalize_fields`가 한다).
fn ids_by_field(
    def: &RecordKindDef,
    fields: &Map<String, Value>,
) -> std::collections::HashMap<String, FieldIds> {
    fields
        .iter()
        .filter_map(|(key, value)| {
            let (mut people, mut courses, mut episodes) = (Vec::new(), Vec::new(), Vec::new());
            let one = Map::from_iter([(key.clone(), value.clone())]);
            normalize_fields(def, one, &mut people, &mut courses, &mut episodes)
                .ok()
                .map(|_| (key.clone(), (people, courses, episodes)))
        })
        .collect()
}

/// 종류·칸·사람·강좌·파일 역할을 확인한다. `kept`는 고치는 기록에 지금 저장된 칸이다. 거기 이미 있는 사람·강좌 id는
/// 다시 확인하지 않고 새로 넣는 id만 확인한다. 나간 멤버나 볼 수 없게 된 강좌가 남은 기록도 다른 칸을 고칠 수 있게
/// 한다(항목 리더와 같은 규칙, 감사 W15). 남은 id로 권한이 넓어지지는 않는다(이미 저장된 값 그대로).
pub(crate) async fn validate(
    tx: &mut UserTx,
    item: ItemId,
    kind_name: &str,
    title: &str,
    fields: Map<String, Value>,
    kept: Option<&Map<String, Value>>,
    files: &[RecordFileInput],
) -> Result<(Value, Value), AppError> {
    let row: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT m.kind, i.workspace_id FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = $1",
    )
    .bind(item)
    .fetch_optional(tx.conn())
    .await?;
    let (module_kind, workspace) = row.ok_or(AppError::NotFound)?;
    let module_kind = ModuleKind::parse(&module_kind).ok_or(AppError::NotFound)?;
    let kinds = record_kinds();
    let def = kinds
        .iter()
        .find(|k| k.kind == kind_name && k.module_kind == module_kind)
        .ok_or_else(|| bad(format!("{kind_name} is not a record kind of this module")))?;
    let length = title.trim().chars().count();
    if !(1..=300).contains(&length) {
        return Err(bad("a record needs a title of 1 to 300 characters"));
    }
    let mut people = Vec::new();
    let mut courses = Vec::new();
    let mut episodes = Vec::new();
    let by_field = kept.map(|_| ids_by_field(def, &fields));
    let fields = normalize_fields(def, fields, &mut people, &mut courses, &mut episodes)?;
    if let (Some(kept), Some(now)) = (kept, by_field) {
        // 같은 칸에 그대로 남은 id만 다시 확인하지 않는다. 다른 칸에 있던 id를 옮겨 넣으면(출석에 남은 나간 사람을
        // 진행자로 등) 새로 넣는 것이라 확인한다(리뷰). 저장된 칸이 지금 정의와 맞지 않으면(정의가 바뀜) 그 칸에 남은
        // id가 없는 것으로 본다.
        let before = ids_by_field(def, kept);
        people.clear();
        courses.clear();
        episodes.clear();
        for (key, (p, c, e)) in now {
            let (kept_people, kept_courses, kept_episodes) =
                before.get(&key).cloned().unwrap_or_default();
            people.extend(p.into_iter().filter(|id| !kept_people.contains(id)));
            courses.extend(c.into_iter().filter(|id| !kept_courses.contains(id)));
            episodes.extend(e.into_iter().filter(|id| !kept_episodes.contains(id)));
        }
    }
    check_references(tx, item, workspace, &people, &courses, &episodes).await?;
    if files.len() > 50 {
        return Err(bad("give at most 50 linked files"));
    }
    for file in files {
        if !def.file_roles.iter().any(|r| r.value == file.role) {
            return Err(bad(format!(
                "{} is not a file role of {kind_name}",
                file.role
            )));
        }
    }
    let files =
        serde_json::to_value(files).map_err(|e| AppError::Config(format!("record files: {e}")))?;
    Ok((fields, files))
}

async fn check_references(
    tx: &mut UserTx,
    item: ItemId,
    workspace: Uuid,
    people: &[Uuid],
    courses: &[Uuid],
    episodes: &[Uuid],
) -> Result<(), AppError> {
    if !people.is_empty() {
        let found: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT user_id) FROM workspace_members WHERE workspace_id = $1 AND user_id = ANY($2)",
        )
        .bind(workspace)
        .bind(people)
        .fetch_one(tx.conn())
        .await?;
        let mut unique = people.to_vec();
        unique.sort_unstable();
        unique.dedup();
        if usize::try_from(found).unwrap_or_default() != unique.len() {
            return Err(bad("people must be members of the workspace"));
        }
    }
    for course in courses {
        // 항목 RLS가 볼 수 없는 강좌를 거른다.
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                            WHERE i.id = $1 AND i.workspace_id = $2 AND m.kind = 'course')",
        )
        .bind(course)
        .bind(workspace)
        .fetch_one(tx.conn())
        .await?;
        if !ok {
            return Err(bad(
                "the course must be a course item you can see in this workspace",
            ));
        }
    }
    if !episodes.is_empty() {
        // 회차는 이 기록이 붙는 강좌(항목)의 회차여야 한다(D-123). 다른 강좌의 회차 id는 거절한다.
        let mut unique = episodes.to_vec();
        unique.sort_unstable();
        unique.dedup();
        let found: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM course_episodes WHERE id = ANY($1) AND item_id = $2",
        )
        .bind(&unique)
        .bind(item)
        .fetch_one(tx.conn())
        .await?;
        if usize::try_from(found).unwrap_or_default() != unique.len() {
            return Err(bad("the episode must be an episode of this course item"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_unique_and_choices_are_set() {
        let kinds = record_kinds();
        // 종류 이름은 모듈 안에서 유일하다(`review`는 스터디·강의 양쪽에 있다, D-36·D-123).
        let mut names: Vec<String> = kinds
            .iter()
            .map(|k| format!("{:?}/{}", k.module_kind, k.kind))
            .collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
        for k in &kinds {
            for field in &k.fields {
                assert_eq!(
                    field.field_type == FieldType::Choice,
                    !field.choices.is_empty(),
                    "{}.{}",
                    k.kind,
                    field.key
                );
            }
        }
    }

    #[test]
    fn fields_are_checked_against_the_kind() {
        let kinds = record_kinds();
        let reference = kinds.iter().find(|k| k.kind == "reference").unwrap();
        let mut people = Vec::new();
        let mut courses = Vec::new();
        let mut episodes = Vec::new();
        let input: Map<String, Value> = serde_json::from_value(serde_json::json!({
            "authors": " Kim, Lee ", "year": 2024, "url": "https://doi.org/10.1/x",
            "read_status": "reading", "tags": ["동기", "동기", " "], "summary": ""
        }))
        .unwrap();
        let out =
            normalize_fields(reference, input, &mut people, &mut courses, &mut episodes).unwrap();
        assert_eq!(out["authors"], "Kim, Lee");
        assert_eq!(out["tags"], serde_json::json!(["동기"]));
        assert!(out.get("summary").is_none());
        for bad_input in [
            serde_json::json!({ "read_status": "maybe" }),
            serde_json::json!({ "url": "javascript:alert(1)" }),
            serde_json::json!({ "year": -1 }),
            serde_json::json!({ "color": "blue" }),
        ] {
            let map: Map<String, Value> = serde_json::from_value(bad_input).unwrap();
            assert!(
                normalize_fields(reference, map, &mut people, &mut courses, &mut episodes).is_err()
            );
        }
    }
}
