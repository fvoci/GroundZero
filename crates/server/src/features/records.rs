//! 항목 기록 (D-33·D-34·D-35·D-36·D-38). 모듈 종류마다 정한 기록 종류와 칸을 이 파일 한곳에서 정의한다. 서버는
//! 이 정의로 칸을 확인하고, 화면은 `GET /api/record-kinds`로 같은 정의를 받아 입력 칸을 그린다. 권한은 항목을
//! 그대로 따른다(DB 정의자 함수). 기록에는 그 항목 첨부의 특정 버전을 연결한다(분석 당시 버전 고정, D-35).

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::{Date, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::{UserTx, new_ids};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::modules::ModuleKind;
use crate::http::AppState;
use crate::ids::{ItemId, UserId};

time::serde::format_description!(day, Date, "[year]-[month]-[day]");

/// 칸의 형식.
#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// 한 줄 글자(300자).
    Text,
    /// 여러 줄 글자(10000자).
    LongText,
    /// 날짜(YYYY-MM-DD).
    Date,
    /// 0 이상의 수.
    Number,
    /// 정해진 값 중 하나.
    Choice,
    /// http·https 주소.
    Url,
    /// 낱말 여럿.
    Tags,
    /// 이 워크스페이스의 사람 한 명(사용자 id).
    Person,
    /// 이 워크스페이스의 사람 여럿.
    People,
    /// 이 워크스페이스의 강좌(강의 모듈 항목) 하나(D-36 강좌 연결).
    Course,
}

/// 고를 수 있는 값.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChoiceDef {
    pub value: &'static str,
    pub label: &'static str,
}

/// 칸 정의.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FieldDef {
    pub key: &'static str,
    pub label: &'static str,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub choices: Vec<ChoiceDef>,
}

/// 기록 종류 정의.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RecordKindDef {
    pub kind: &'static str,
    pub label: &'static str,
    pub module_kind: ModuleKind,
    /// 기록 날짜 칸의 이름(예: 모임 날짜, 실행일). 없으면 날짜를 받지 않는다.
    pub date_label: Option<&'static str>,
    pub fields: Vec<FieldDef>,
    /// 파일 연결 역할. 비었으면 파일을 연결하지 않는다.
    pub file_roles: Vec<ChoiceDef>,
}

const fn c(value: &'static str, label: &'static str) -> ChoiceDef {
    ChoiceDef { value, label }
}

fn f(key: &'static str, label: &'static str, field_type: FieldType) -> FieldDef {
    FieldDef {
        key,
        label,
        field_type,
        choices: Vec::new(),
    }
}

fn choice(key: &'static str, label: &'static str, choices: &[ChoiceDef]) -> FieldDef {
    FieldDef {
        key,
        label,
        field_type: FieldType::Choice,
        choices: choices.to_vec(),
    }
}

fn kind(
    kind: &'static str,
    label: &'static str,
    module_kind: ModuleKind,
    date_label: Option<&'static str>,
    fields: Vec<FieldDef>,
    file_roles: &[ChoiceDef],
) -> RecordKindDef {
    RecordKindDef {
        kind,
        label,
        module_kind,
        date_label,
        fields,
        file_roles: file_roles.to_vec(),
    }
}

const LEVEL: [ChoiceDef; 3] = [c("low", "낮음"), c("medium", "보통"), c("high", "높음")];
const ATTACH: [ChoiceDef; 1] = [c("other", "첨부")];

/// 모든 기록 종류. 각 종류는 결정 문서의 낱말에 대응한다(X-08: 목록 밖 기능을 더하지 않는다).
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one declarative table of every record kind; splitting it would scatter the schema"
)]
pub fn record_kinds() -> Vec<RecordKindDef> {
    use FieldType::{Course, Date as Day, LongText, Number, People, Person, Tags, Text, Url};
    use ModuleKind::{Hobby, Project, Research, Study};
    vec![
        // D-33 프로젝트: 산출물·완료 기준·위험·문제 기록, 회고.
        kind(
            "deliverable",
            "산출물",
            Project,
            Some("마감일"),
            vec![
                choice(
                    "status",
                    "상태",
                    &[
                        c("planned", "계획"),
                        c("in_progress", "진행 중"),
                        c("done", "완료"),
                        c("accepted", "승인"),
                    ],
                ),
                f("owner", "담당", Person),
                f("criteria", "완료 기준", LongText),
            ],
            &ATTACH,
        ),
        kind(
            "risk",
            "위험",
            Project,
            Some("확인일"),
            vec![
                choice("likelihood", "가능성", &LEVEL),
                choice("impact", "영향", &LEVEL),
                choice(
                    "status",
                    "상태",
                    &[
                        c("open", "열림"),
                        c("mitigating", "대응 중"),
                        c("closed", "닫힘"),
                    ],
                ),
                f("owner", "담당", Person),
                f("mitigation", "대응", LongText),
            ],
            &[],
        ),
        kind(
            "issue",
            "문제",
            Project,
            Some("발생일"),
            vec![
                choice(
                    "severity",
                    "심각도",
                    &[
                        c("low", "낮음"),
                        c("medium", "보통"),
                        c("high", "높음"),
                        c("critical", "긴급"),
                    ],
                ),
                choice(
                    "status",
                    "상태",
                    &[
                        c("open", "열림"),
                        c("in_progress", "처리 중"),
                        c("resolved", "해결"),
                        c("closed", "닫힘"),
                    ],
                ),
                f("owner", "담당", Person),
                f("resolution", "해결 내용", LongText),
            ],
            &[],
        ),
        kind(
            "retrospective",
            "회고",
            Project,
            Some("회고일"),
            vec![
                f("went_well", "잘된 점", LongText),
                f("to_improve", "아쉬운 점", LongText),
                f("actions", "다음에 할 일", LongText),
            ],
            &[],
        ),
        // D-34 연구: 문헌, 실험·인터뷰·랩미팅·피드백·결정·설계 변경 기록, 데이터, 변수 사전·코드북, 분석, 투고.
        kind(
            "reference",
            "문헌",
            Research,
            None,
            vec![
                f("authors", "저자", Text),
                f("year", "연도", Number),
                f("venue", "학술지·학회", Text),
                f("doi", "DOI", Text),
                f("url", "URL", Url),
                choice(
                    "read_status",
                    "읽기 상태",
                    &[
                        c("unread", "읽기 전"),
                        c("reading", "읽는 중"),
                        c("read", "읽음"),
                        c("skimmed", "훑어봄"),
                    ],
                ),
                f("tags", "태그", Tags),
                f("summary", "요약", LongText),
                f("method_note", "연구 방법", LongText),
                f("findings", "주요 결과", LongText),
                f("limitations", "한계", LongText),
                f("citation", "인용", LongText),
                f("pages", "페이지", Text),
                f("zotero_key", "Zotero 키", Text),
                f("zotero_link", "Zotero 원본", Url),
                f("zotero_collections", "Zotero 컬렉션", Tags),
            ],
            &[c("reference", "PDF·원문")],
        ),
        kind(
            "research_log",
            "연구 기록",
            Research,
            Some("날짜"),
            vec![
                choice(
                    "log_type",
                    "종류",
                    &[
                        c("experiment", "실험"),
                        c("interview", "인터뷰"),
                        c("lab_meeting", "랩미팅"),
                        c("feedback", "피드백"),
                        c("decision", "결정"),
                        c("design_change", "설계 변경"),
                    ],
                ),
                f("participants", "참여자", People),
            ],
            &ATTACH,
        ),
        kind(
            "dataset",
            "데이터",
            Research,
            Some("수집일"),
            vec![
                choice(
                    "stage",
                    "구분",
                    &[
                        c("raw", "원자료"),
                        c("cleaned", "정제 자료"),
                        c("analysis", "분석 자료"),
                    ],
                ),
                f("source", "출처", Text),
                f("location", "외부 위치", Text),
                f("data_version", "자료 버전", Text),
                f("quality", "결측·제외·품질", LongText),
                f("access_note", "권한 관리", Text),
            ],
            &[c("input", "자료 파일")],
        ),
        kind(
            "variable",
            "변수",
            Research,
            None,
            vec![
                f("label", "설명", Text),
                choice(
                    "var_type",
                    "형식",
                    &[
                        c("numeric", "숫자"),
                        c("categorical", "범주"),
                        c("text", "글자"),
                        c("date", "날짜"),
                        c("other", "기타"),
                    ],
                ),
                f("values", "값·코드", LongText),
                f("measurement", "측정", LongText),
                f("dataset", "데이터", Text),
            ],
            &[],
        ),
        kind(
            "analysis",
            "분석",
            Research,
            Some("실행일"),
            vec![
                f("purpose", "목적", LongText),
                f("owner", "담당", Person),
                choice(
                    "status",
                    "상태",
                    &[
                        c("planned", "계획"),
                        c("running", "실행 중"),
                        c("done", "완료"),
                        c("failed", "실패"),
                    ],
                ),
                f("settings", "설정", LongText),
                f("log", "로그·오류", LongText),
                f("compute_minutes", "컴퓨터 실행 시간(분)", Number),
                f("results", "결과", LongText),
                f("compared_with", "비교한 이전 실행", Text),
                f("comparison", "재실행 결과 비교", LongText),
            ],
            &[
                c("input", "입력 데이터"),
                c("code", "코드"),
                c("result", "결과 표·그림"),
            ],
        ),
        kind(
            "submission",
            "투고",
            Research,
            Some("제출 예정일"),
            vec![
                f("venue", "투고 대상·학술대회", Text),
                choice(
                    "status",
                    "상태",
                    &[
                        c("preparing", "준비"),
                        c("submitted", "투고"),
                        c("under_review", "심사"),
                        c("revision", "수정"),
                        c("accepted", "게재 확정"),
                        c("published", "게재"),
                        c("rejected", "거절"),
                    ],
                ),
                f("revision_due", "수정 마감일", Day),
                f("coauthors", "공동저자", People),
                f("checklist", "공동저자 검토·제출 체크리스트", LongText),
                f("irb", "IRB 자료·일정", LongText),
            ],
            &[c("other", "제출본·심사 의견·답변서")],
        ),
        // D-36 스터디: 자료(강좌 연결), 모임·발표·출석, 시험·점수·피드백, 복습·취약 영역.
        kind(
            "material",
            "자료",
            Study,
            None,
            vec![
                choice(
                    "material_type",
                    "종류",
                    &[
                        c("book", "책"),
                        c("paper", "논문"),
                        c("workbook", "문제집"),
                        c("web", "웹 자료"),
                        c("course", "강좌"),
                        c("other", "기타"),
                    ],
                ),
                f("course", "연결한 강좌", Course),
                f("url", "주소", Url),
                f("amount", "분량(페이지·문제 수)", Text),
                f("chapters", "교육과정·챕터·선행 학습", LongText),
            ],
            &ATTACH,
        ),
        kind(
            "meeting",
            "모임",
            Study,
            Some("모임 날짜"),
            vec![
                f("presenter", "진행자·발표자", Person),
                f("assignment", "준비 과제", LongText),
                f("attendees", "출석", People),
            ],
            &ATTACH,
        ),
        kind(
            "exam",
            "시험",
            Study,
            Some("시험일"),
            vec![
                f("taker", "응시자", Person),
                f("score", "점수", Number),
                f("max_score", "만점", Number),
                f("feedback", "피드백", LongText),
            ],
            &ATTACH,
        ),
        kind(
            "review",
            "복습",
            Study,
            Some("다음 복습일"),
            vec![
                f("topic", "주제", Text),
                f("weakness", "취약 영역", LongText),
                f("rounds", "반복 횟수", Number),
            ],
            &[],
        ),
        // D-38 취미: 재료·장비(보유·필요·준비), 장비 설정·사용 기록, 활동(날짜·시간·참여자·결과), 결과물·회고.
        kind(
            "supply",
            "재료·장비",
            Hobby,
            None,
            vec![
                choice(
                    "supply_type",
                    "종류",
                    &[c("material", "재료"), c("equipment", "장비")],
                ),
                choice(
                    "state",
                    "상태",
                    &[
                        c("owned", "보유"),
                        c("needed", "필요"),
                        c("preparing", "준비 중"),
                        c("ready", "준비됨"),
                    ],
                ),
                f("quantity", "수량", Text),
                f("settings", "장비 설정", LongText),
            ],
            &[],
        ),
        kind(
            "equipment_use",
            "장비 사용",
            Hobby,
            Some("사용일"),
            vec![
                f("equipment", "장비", Text),
                f("settings", "설정", LongText),
            ],
            &[],
        ),
        kind(
            "activity",
            "활동",
            Hobby,
            Some("활동일"),
            vec![
                f("minutes", "시간(분)", Number),
                f("participants", "참여자", People),
                f("result", "결과", LongText),
                f("learned", "과정·실패·배운 점", LongText),
            ],
            &[c("other", "사진·영상")],
        ),
        kind(
            "portfolio",
            "결과물",
            Hobby,
            Some("완성일"),
            vec![
                f("retrospective", "회고", LongText),
                f("next_goal", "다음 목표", LongText),
            ],
            &[c("result", "결과물 사진·영상")],
        ),
    ]
}

/// 기록 종류 정의 전체(화면이 입력 칸을 그린다).
#[utoipa::path(get, path = "/api/record-kinds", operation_id = "record_kinds",
    responses((status = 200, body = [RecordKindDef])), tag = "records")]
pub async fn kinds(_user: CurrentUser) -> Json<Vec<RecordKindDef>> {
    Json(record_kinds())
}

/// 연결한 파일 버전.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecordFile {
    pub role: String,
    /// 파일이 영구 삭제되었으면 없다(이름·버전·SHA-256은 남는다).
    #[schema(value_type = Option<String>, format = Uuid)]
    pub file_id: Option<Uuid>,
    pub file_name: String,
    pub version: i32,
    pub sha256: String,
    pub size: i64,
    /// 그 버전을 지금도 열 수 있는지.
    pub available: bool,
}

/// 항목 기록.
#[derive(Debug, Serialize, ToSchema)]
pub struct Record {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub occurred_on: Option<Date>,
    pub body: String,
    #[schema(value_type = Object)]
    pub fields: Value,
    pub files: Vec<RecordFile>,
    pub created_by: UserId,
    pub created_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    pub version: i32,
    pub can_edit: bool,
    pub can_delete: bool,
}

/// 연결할 파일 버전. 버전이 없으면 지금 버전.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct RecordFileInput {
    #[schema(value_type = String, format = Uuid)]
    pub file_id: Uuid,
    pub version: Option<i32>,
    pub role: String,
}

/// 기록 만들기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct NewRecord {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub kind: String,
    pub title: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub occurred_on: Option<Date>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub fields: Map<String, Value>,
    #[serde(default)]
    pub files: Vec<RecordFileInput>,
}

/// 기록 고치기. `version`은 읽은 판.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EditRecord {
    pub version: i32,
    pub title: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub occurred_on: Option<Date>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub fields: Map<String, Value>,
    #[serde(default)]
    pub files: Vec<RecordFileInput>,
}

/// 새 판.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecordVersion {
    pub version: i32,
}

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

/// 칸을 정의대로 확인·정리한다. 사람·강좌 칸의 id는 따로 모아 DB에서 확인한다.
fn normalize_fields(
    def: &RecordKindDef,
    fields: Map<String, Value>,
    people: &mut Vec<Uuid>,
    courses: &mut Vec<Uuid>,
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
            FieldType::Tags => {
                let list = value
                    .as_array()
                    .ok_or_else(|| bad(format!("{key} must be a list")))?;
                let mut tags: Vec<String> = Vec::new();
                for t in list {
                    if let Some(Value::String(s)) = text_value(t, &key, 100)?
                        && !tags.contains(&s)
                    {
                        tags.push(s);
                    }
                }
                if tags.len() > 50 {
                    return Err(bad(format!("{key} has too many tags")));
                }
                (!tags.is_empty()).then(|| serde_json::json!(tags))
            }
            FieldType::Person => {
                let id = uuid_value(&value, &key)?;
                people.push(id);
                Some(Value::String(id.to_string()))
            }
            FieldType::People => {
                let list = value
                    .as_array()
                    .ok_or_else(|| bad(format!("{key} must be a list")))?;
                let mut ids: Vec<Uuid> = Vec::new();
                for v in list {
                    let id = uuid_value(v, &key)?;
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                if ids.len() > 200 {
                    return Err(bad(format!("{key} has too many people")));
                }
                people.extend(&ids);
                (!ids.is_empty()).then(|| serde_json::json!(ids))
            }
            FieldType::Course => {
                let id = uuid_value(&value, &key)?;
                courses.push(id);
                Some(Value::String(id.to_string()))
            }
        };
        if let Some(v) = normalized {
            out.insert(key, v);
        }
    }
    Ok(Value::Object(out))
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
    let fields = normalize_fields(def, fields, &mut people, &mut courses)?;
    if let Some(kept) = kept {
        let (mut kept_people, mut kept_courses) = (Vec::new(), Vec::new());
        // 저장된 칸이 지금 정의와 맞지 않으면(정의가 바뀜) 남은 id가 없는 것으로 본다.
        if normalize_fields(def, kept.clone(), &mut kept_people, &mut kept_courses).is_ok() {
            people.retain(|p| !kept_people.contains(p));
            courses.retain(|c| !kept_courses.contains(c));
        }
    }
    if !people.is_empty() {
        let found: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT user_id) FROM workspace_members WHERE workspace_id = $1 AND user_id = ANY($2)",
        )
        .bind(workspace)
        .bind(&people)
        .fetch_one(tx.conn())
        .await?;
        let mut unique = people.clone();
        unique.sort_unstable();
        unique.dedup();
        if usize::try_from(found).unwrap_or_default() != unique.len() {
            return Err(bad("people must be members of the workspace"));
        }
    }
    for course in &courses {
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

type RecordRow = (
    Uuid,
    String,
    String,
    Option<Date>,
    String,
    Value,
    UserId,
    Option<String>,
    OffsetDateTime,
    i32,
    String,
    bool,
);

type FileRow = (Uuid, String, Option<Uuid>, String, i32, String, i64, bool);

/// 항목의 기록(볼 수 있는 사람). 종류·날짜 순.
#[utoipa::path(get, path = "/api/items/{item_id}/records", operation_id = "item_records",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Record]), (status = 404, body = ErrorBody)), tag = "records")]
pub async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<Record>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let state_row: Option<(String, bool)> = sqlx::query_as(
        "SELECT app_item_access(i.id), i.archived_at IS NOT NULL OR m.archived_at IS NOT NULL
         FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = $1",
    )
    .bind(item)
    .fetch_optional(tx.conn())
    .await?;
    let (access, archived) = state_row.ok_or(AppError::NotFound)?;
    let rows: Vec<RecordRow> = sqlx::query_as(
        "SELECT r.id, r.kind, r.title, r.occurred_on, r.body, r.fields, r.created_by,
                (SELECT u.display_name FROM users u WHERE u.id = r.created_by), r.updated_at, r.version,
                $2::text, r.created_by = app_user_id()
         FROM item_records r WHERE r.item_id = $1
         ORDER BY r.kind, r.occurred_on DESC NULLS LAST, r.created_at DESC",
    )
    .bind(item)
    .bind(&access)
    .fetch_all(tx.conn())
    .await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.0).collect();
    let files: Vec<FileRow> = sqlx::query_as(
        "SELECT f.record_id, f.role, f.file_id, f.file_name, f.version, f.sha256, f.size,
                f.version_id IS NOT NULL AND EXISTS (SELECT 1 FROM files x WHERE x.id = f.file_id AND x.deleted_at IS NULL)
         FROM record_files f WHERE f.record_id = ANY($1) ORDER BY f.record_id, f.position",
    )
    .bind(&ids)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let mut by_record: BTreeMap<Uuid, Vec<RecordFile>> = BTreeMap::new();
    for (record, role, file_id, file_name, version, sha256, size, available) in files {
        by_record.entry(record).or_default().push(RecordFile {
            role,
            file_id,
            file_name,
            version,
            sha256,
            size,
            available,
        });
    }
    let writable = !archived && (access == "owner" || access == "write");
    Ok(Json(
        rows.into_iter()
            .map(
                |(
                    id,
                    kind,
                    title,
                    occurred_on,
                    body,
                    fields,
                    created_by,
                    name,
                    updated_at,
                    version,
                    access,
                    mine,
                )| {
                    Record {
                        id,
                        kind,
                        title,
                        occurred_on,
                        body,
                        fields,
                        files: by_record.remove(&id).unwrap_or_default(),
                        created_by,
                        created_by_name: name,
                        updated_at,
                        version,
                        can_edit: writable,
                        can_delete: writable && (mine || access == "owner"),
                    }
                },
            )
            .collect(),
    ))
}

/// 기록 만들기(항목에 쓸 수 있는 사람). 같은 요청 id면 같은 기록.
#[utoipa::path(post, path = "/api/items/{item_id}/records", operation_id = "create_record",
    params(("item_id" = String, Path, format = Uuid)), request_body = NewRecord,
    responses((status = 201, body = Object), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "records")]
pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<NewRecord>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let (fields, files) = validate(
        &mut tx,
        item,
        &input.kind,
        &input.title,
        input.fields,
        None,
        &input.files,
    )
    .await?;
    let id: Uuid =
        sqlx::query_scalar("SELECT app_create_record($1, $2, $3, $4, $5, $6, $7, $8, $9)")
            .bind(input.request_id)
            .bind(item)
            .bind(&input.kind)
            .bind(input.title.trim())
            .bind(input.occurred_on)
            .bind(&input.body)
            .bind(fields)
            .bind(files)
            .bind(new_ids(2))
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// 기록 고치기(항목에 쓸 수 있는 사람). 읽은 뒤 바뀌었으면 409.
#[utoipa::path(put, path = "/api/records/{record_id}", operation_id = "update_record",
    params(("record_id" = String, Path, format = Uuid)), request_body = EditRecord,
    responses((status = 200, body = RecordVersion), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "records")]
pub async fn update(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(record): Path<Uuid>,
    Json(input): Json<EditRecord>,
) -> Result<Json<RecordVersion>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let found: Option<(ItemId, String, Value)> =
        sqlx::query_as("SELECT item_id, kind, fields FROM item_records WHERE id = $1")
            .bind(record)
            .fetch_optional(tx.conn())
            .await?;
    let (item, kind, stored) = found.ok_or(AppError::NotFound)?;
    // 그사이 바뀐 기록이면 아래 app_update_record가 판으로 409를 낸다.
    let (fields, files) = validate(
        &mut tx,
        item,
        &kind,
        &input.title,
        input.fields,
        stored.as_object(),
        &input.files,
    )
    .await?;
    let version: i32 = sqlx::query_scalar("SELECT app_update_record($1, $2, $3, $4, $5, $6, $7)")
        .bind(record)
        .bind(input.version)
        .bind(input.title.trim())
        .bind(input.occurred_on)
        .bind(&input.body)
        .bind(fields)
        .bind(files)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(RecordVersion { version }))
}

/// 기록 지우기(만든 사람 또는 항목 소유 권한자).
#[utoipa::path(delete, path = "/api/records/{record_id}", operation_id = "delete_record",
    params(("record_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "records")]
pub async fn delete(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(record): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_record($1, $2)")
        .bind(record)
        .bind(new_ids(1))
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
    fn kinds_are_unique_and_choices_are_set() {
        let kinds = record_kinds();
        let mut names: Vec<&str> = kinds.iter().map(|k| k.kind).collect();
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
        let input: Map<String, Value> = serde_json::from_value(serde_json::json!({
            "authors": " Kim, Lee ", "year": 2024, "url": "https://doi.org/10.1/x",
            "read_status": "reading", "tags": ["동기", "동기", " "], "summary": ""
        }))
        .unwrap();
        let out = normalize_fields(reference, input, &mut people, &mut courses).unwrap();
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
            assert!(normalize_fields(reference, map, &mut people, &mut courses).is_err());
        }
    }
}
