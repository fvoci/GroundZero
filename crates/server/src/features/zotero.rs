//! Zotero 읽기 연동 (D-34 "Zotero는 서지 정보·컬렉션 읽기와 원본 링크를 지원한다", X-05 PDF·주석 가져오기와 양방향 쓰기
//! 제외). 사용자가 가져올 때마다 Zotero API 키를 보내고, 서버는 그 요청 동안만 머리글로 Zotero Web API(v3)에 쓰며 저장하거나
//! 기록하지 않는다. 가져온 서지는 연구 항목의 문헌 기록(records)이 되고, Zotero 키로 맞춰 다시 가져오면 서지 칸만 새로 하고
//! 읽기 상태·인용·메모 같은 내 칸은 그대로 둔다. 계약: D-34(docs/decisions/modules.md#d-34)

use std::collections::HashMap;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::features::records::{RecordFileInput, validate};
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::ItemId;
use fvoci_db::UserTx;
use fvoci_http::session::CurrentUser;

/// 한 번에 가져오는 최대 문헌 수.
const MAX_ITEMS: usize = 2000;
const PAGE: usize = 100;
/// Zotero에서 오는 칸 중 서지(가져올 때마다 새로 쓰는 것).
const BIBLIOGRAPHIC: [&str; 8] = [
    "authors",
    "year",
    "venue",
    "doi",
    "url",
    "pages",
    "zotero_key",
    "zotero_link",
];

/// 어느 Zotero 라이브러리인지. 키는 저장하지 않는다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ZoteroLibrary {
    /// Zotero 설정의 API 키(읽기 권한). 이 요청에만 쓴다.
    pub api_key: String,
    /// `user` 또는 `group`.
    pub library_type: String,
    /// 사용자 ID 또는 그룹 ID(숫자).
    pub library_id: String,
}

/// 컬렉션.
#[derive(Debug, Serialize, ToSchema)]
pub struct ZoteroCollection {
    pub key: String,
    pub name: String,
}

/// 가져오기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ZoteroImport {
    #[serde(flatten)]
    pub library: ZoteroLibrary,
    /// 이 컬렉션만. 없으면 라이브러리의 최상위 문헌 전부.
    #[serde(default)]
    pub collection_key: Option<String>,
}

/// 가져온 결과.
#[derive(Debug, Serialize, ToSchema)]
pub struct ZoteroImported {
    pub created: i32,
    pub updated: i32,
    /// 첨부·메모처럼 문헌이 아닌 것과 제목 없는 것.
    pub skipped: i32,
}

struct Library {
    prefix: String,
    key: String,
}

fn library(input: &ZoteroLibrary) -> Result<Library, AppError> {
    let kind = match input.library_type.as_str() {
        "user" => "users",
        "group" => "groups",
        _ => {
            return Err(AppError::BadRequest(
                "library_type must be user or group".into(),
            ));
        }
    };
    let id = input.library_id.trim();
    if id.is_empty() || id.len() > 20 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::BadRequest("library_id must be a number".into()));
    }
    let key = input.api_key.trim();
    if key.is_empty() || key.len() > 64 || !key.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(AppError::BadRequest(
            "the Zotero API key looks malformed".into(),
        ));
    }
    Ok(Library {
        prefix: format!("{kind}/{id}"),
        key: key.to_owned(),
    })
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 16 && key.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Zotero 목록 한 종류를 끝까지(최대 `limit`개) 읽는다.
async fn fetch_all(
    state: &AppState,
    lib: &Library,
    path: &str,
    limit: usize,
) -> Result<Vec<Value>, AppError> {
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Config(format!("http client: {e}")))?;
    let mut out = Vec::new();
    loop {
        let mut url = state
            .config
            .zotero_api_url
            .join(&format!("{}/{path}", lib.prefix))
            .map_err(|e| AppError::Config(format!("zotero url: {e}")))?;
        url.query_pairs_mut()
            .append_pair("format", "json")
            .append_pair("limit", &PAGE.to_string())
            .append_pair("start", &out.len().to_string());
        // 키는 머리글로만 보낸다(주소·기록에 남지 않게).
        let response = http
            .get(url)
            .header("Zotero-API-Version", "3")
            .header("Zotero-API-Key", &lib.key)
            .send()
            .await
            .map_err(|_| AppError::Upstream("could not reach Zotero".into()))?;
        match response.status().as_u16() {
            200 => {}
            403 => {
                return Err(AppError::BadRequest(
                    "Zotero refused the key for this library".into(),
                ));
            }
            404 => {
                return Err(AppError::BadRequest(
                    "no such Zotero library or collection".into(),
                ));
            }
            429 | 503 => return Err(AppError::Upstream("Zotero is busy; try again later".into())),
            code => return Err(AppError::Upstream(format!("Zotero answered {code}"))),
        }
        let page: Vec<Value> = response
            .json()
            .await
            .map_err(|_| AppError::Upstream("Zotero sent an unexpected answer".into()))?;
        let done = page.len() < PAGE;
        out.extend(page);
        if done || out.len() >= limit {
            out.truncate(limit);
            return Ok(out);
        }
    }
}

async fn collections(state: &AppState, lib: &Library) -> Result<Vec<ZoteroCollection>, AppError> {
    let rows = fetch_all(state, lib, "collections", 1000).await?;
    Ok(rows
        .iter()
        .filter_map(|c| {
            let key = c.get("key")?.as_str()?.to_owned();
            let name = c.pointer("/data/name")?.as_str()?.trim().to_owned();
            (valid_key(&key) && !name.is_empty()).then_some(ZoteroCollection { key, name })
        })
        .collect())
}

/// 내 Zotero 라이브러리의 컬렉션(로그인한 사람). 키는 저장하지 않는다.
#[utoipa::path(post, path = "/api/zotero/collections", operation_id = "zotero_collections",
    request_body = ZoteroLibrary,
    responses((status = 200, body = [ZoteroCollection]), (status = 400, body = ErrorBody),
              (status = 502, body = ErrorBody)), tag = "records")]
pub async fn list_collections(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
    Json(input): Json<ZoteroLibrary>,
) -> Result<Json<Vec<ZoteroCollection>>, AppError> {
    let lib = library(&input)?;
    Ok(Json(collections(&state, &lib).await?))
}

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Zotero 문헌 하나 → (제목, 서지 칸). 문헌이 아니면 None.
fn reference_fields(
    item: &Value,
    names: &HashMap<String, String>,
) -> Option<(String, Map<String, Value>)> {
    let data = item.get("data")?;
    let kind = data.get("itemType")?.as_str()?;
    if matches!(kind, "attachment" | "note" | "annotation") {
        return None;
    }
    let title: String = text(data.get("title"))?.chars().take(300).collect();
    let key = item.get("key")?.as_str()?.to_owned();
    if !valid_key(&key) {
        return None;
    }
    let mut f = Map::new();
    let authors: Vec<String> = data
        .get("creators")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|c| {
                    text(c.get("name")).or_else(|| {
                        let last = text(c.get("lastName"))?;
                        Some(match text(c.get("firstName")) {
                            Some(first) => format!("{last}, {first}"),
                            None => last,
                        })
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if !authors.is_empty() {
        f.insert(
            "authors".into(),
            json!(authors.join("; ").chars().take(300).collect::<String>()),
        );
    }
    if let Some(date) = text(data.get("date")) {
        let digits: Vec<char> = date.chars().collect();
        if let Some(year) = digits
            .windows(4)
            .find(|w| w.iter().all(char::is_ascii_digit))
            .and_then(|w| w.iter().collect::<String>().parse::<u32>().ok())
        {
            f.insert("year".into(), json!(year));
        }
    }
    let venue = [
        "publicationTitle",
        "proceedingsTitle",
        "bookTitle",
        "conferenceName",
        "publisher",
    ]
    .iter()
    .find_map(|k| text(data.get(*k)));
    if let Some(v) = venue {
        f.insert(
            "venue".into(),
            json!(v.chars().take(300).collect::<String>()),
        );
    }
    if let Some(doi) = text(data.get("DOI")) {
        f.insert(
            "doi".into(),
            json!(doi.chars().take(300).collect::<String>()),
        );
    }
    if let Some(url) = text(data.get("url")).filter(|u| {
        let l = u.to_ascii_lowercase();
        (l.starts_with("http://") || l.starts_with("https://"))
            && !u.chars().any(char::is_whitespace)
    }) {
        f.insert("url".into(), json!(url));
    }
    if let Some(pages) = text(data.get("pages")) {
        f.insert(
            "pages".into(),
            json!(pages.chars().take(300).collect::<String>()),
        );
    }
    if let Some(summary) = text(data.get("abstractNote")) {
        f.insert(
            "summary".into(),
            json!(summary.chars().take(10_000).collect::<String>()),
        );
    }
    let tags: Vec<String> = data
        .get("tags")
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .filter_map(|x| text(x.get("tag")))
                .map(|s| s.chars().take(100).collect())
                .take(50)
                .collect()
        })
        .unwrap_or_default();
    if !tags.is_empty() {
        f.insert("tags".into(), json!(tags));
    }
    let in_collections: Vec<String> = data
        .get("collections")
        .and_then(Value::as_array)
        .map(|c| {
            c.iter()
                .filter_map(Value::as_str)
                .filter_map(|k| names.get(k).cloned())
                .take(50)
                .collect()
        })
        .unwrap_or_default();
    if !in_collections.is_empty() {
        f.insert("zotero_collections".into(), json!(in_collections));
    }
    f.insert("zotero_key".into(), json!(key));
    if let Some(link) = item
        .pointer("/links/alternate/href")
        .and_then(Value::as_str)
        .filter(|l| l.starts_with("https://") || l.starts_with("http://"))
    {
        f.insert("zotero_link".into(), json!(link));
    }
    Some((title, f))
}

/// 이미 가져온 같은 문헌: 내 칸은 두고 서지 칸만 바꾼다. 요약은 비어 있을 때만, 태그는 합친다.
fn merged(existing: &Map<String, Value>, incoming: Map<String, Value>) -> Map<String, Value> {
    let mut out = existing.clone();
    for key in BIBLIOGRAPHIC.iter().chain(["zotero_collections"].iter()) {
        out.remove(*key);
    }
    for (k, v) in incoming {
        match k.as_str() {
            "summary" if existing.get("summary").is_some() => {}
            "tags" => {
                let mut tags: Vec<Value> = existing
                    .get("tags")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for t in v.as_array().cloned().unwrap_or_default() {
                    if !tags.contains(&t) {
                        tags.push(t);
                    }
                }
                tags.truncate(50);
                out.insert(k, Value::Array(tags));
            }
            _ => {
                out.insert(k, v);
            }
        }
    }
    out
}

async fn existing_files(tx: &mut UserTx, record: Uuid) -> Result<Vec<RecordFileInput>, AppError> {
    let rows: Vec<(Uuid, i32, String)> = sqlx::query_as(
        "SELECT file_id, version, role FROM record_files
         WHERE record_id = $1 AND file_id IS NOT NULL ORDER BY position",
    )
    .bind(record)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(|(file_id, version, role)| RecordFileInput {
            file_id,
            version: Some(version),
            role,
        })
        .collect())
}

/// Zotero에서 문헌 가져오기(연구 항목에 쓸 수 있는 사람). 같은 Zotero 문헌은 새로 만들지 않고 서지만 고친다.
#[utoipa::path(post, path = "/api/items/{item_id}/zotero/import", operation_id = "zotero_import",
    params(("item_id" = String, Path, format = Uuid)), request_body = ZoteroImport,
    responses((status = 200, body = ZoteroImported), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 502, body = ErrorBody)),
    tag = "records")]
pub async fn import(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<ZoteroImport>,
) -> Result<Json<ZoteroImported>, AppError> {
    let lib = library(&input.library)?;
    if input
        .collection_key
        .as_deref()
        .is_some_and(|key| !valid_key(key))
    {
        return Err(AppError::BadRequest(
            "collection_key looks malformed".into(),
        ));
    }
    // 바깥에 묻기 전에 그 항목에 쓸 수 있는지 먼저 본다.
    {
        let mut tx = state.db.begin_as(user).await?;
        let kind: Option<(String, String)> = sqlx::query_as(
            "SELECT m.kind, app_access('item', i.id) FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = $1",
        )
        .bind(item)
        .fetch_optional(tx.conn())
        .await?;
        match kind {
            None => return Err(AppError::NotFound),
            Some((k, _)) if k != "research" => {
                return Err(AppError::BadRequest(
                    "Zotero imports go into research items".into(),
                ));
            }
            Some((_, access)) if access != "owner" && access != "write" => {
                return Err(AppError::Forbidden);
            }
            Some(_) => {}
        }
        tx.commit().await?;
    }
    let names: HashMap<String, String> = collections(&state, &lib)
        .await?
        .into_iter()
        .map(|c| (c.key, c.name))
        .collect();
    let path = match input.collection_key.as_deref() {
        Some(key) => format!("collections/{key}/items/top"),
        None => "items/top".to_owned(),
    };
    let items = fetch_all(&state, &lib, &path, MAX_ITEMS).await?;

    let mut tx = state.db.begin_as(user).await?;
    // 같은 항목에 가져오기가 겹치면 차례로(같은 문헌이 두 번 생기지 않게).
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('zotero:' || $1::text, 0))")
        .bind(item)
        .execute(tx.conn())
        .await?;
    let (mut created, mut updated, mut skipped) = (0, 0, 0);
    for entry in &items {
        let Some((title, fields)) = reference_fields(entry, &names) else {
            skipped += 1;
            continue;
        };
        let key = fields
            .get("zotero_key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let found: Option<(Uuid, i32, Value)> = sqlx::query_as(
            "SELECT id, version, fields FROM item_records
             WHERE item_id = $1 AND kind = 'reference' AND fields->>'zotero_key' = $2
             ORDER BY created_at LIMIT 1",
        )
        .bind(item)
        .bind(&key)
        .fetch_optional(tx.conn())
        .await?;
        match found {
            Some((id, version, old)) => {
                let old = old.as_object().cloned().unwrap_or_default();
                let body: Option<String> =
                    sqlx::query_scalar("SELECT body FROM item_records WHERE id = $1")
                        .bind(id)
                        .fetch_optional(tx.conn())
                        .await?;
                let occurred: Option<time::Date> =
                    sqlx::query_scalar("SELECT occurred_on FROM item_records WHERE id = $1")
                        .bind(id)
                        .fetch_one(tx.conn())
                        .await?;
                let files = existing_files(&mut tx, id).await?;
                let (fields, files) = validate(
                    &mut tx,
                    item,
                    "reference",
                    &title,
                    merged(&old, fields),
                    Some(&old),
                    &files,
                )
                .await?;
                sqlx::query_scalar::<_, i32>(
                    "SELECT app_update_record($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(id)
                .bind(version)
                .bind(&title)
                .bind(occurred)
                .bind(body.unwrap_or_default())
                .bind(fields)
                .bind(files)
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
                updated += 1;
            }
            None => {
                let (fields, files) =
                    validate(&mut tx, item, "reference", &title, fields, None, &[]).await?;
                sqlx::query_scalar::<_, Uuid>(
                    "SELECT app_create_record($1, $2, 'reference', $3, NULL, '', $4, $5, $6)",
                )
                .bind(Uuid::now_v7())
                .bind(item)
                .bind(&title)
                .bind(fields)
                .bind(files)
                .bind(fvoci_db::new_ids(2))
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
                created += 1;
            }
        }
    }
    tx.commit().await?;
    Ok(Json(ZoteroImported {
        created,
        updated,
        skipped,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zotero_items_become_reference_fields() {
        let names = HashMap::from([("COLL1".to_owned(), "동기 연구".to_owned())]);
        let item = json!({ "key": "ABCD1234",
            "links": { "alternate": { "href": "https://www.zotero.org/users/1/items/ABCD1234" } },
            "data": { "itemType": "journalArticle", "title": " 학습 동기와 성취 ",
                "creators": [{ "lastName": "Kim", "firstName": "Minsu" }, { "name": "Study Group" }],
                "date": "March 2021", "publicationTitle": "교육학 연구", "DOI": "10.1/x",
                "url": "javascript:alert(1)", "abstractNote": "요약", "tags": [{ "tag": "동기" }],
                "collections": ["COLL1", "UNKNOWN"] } });
        let (title, f) = reference_fields(&item, &names).unwrap();
        assert_eq!(title, "학습 동기와 성취");
        assert_eq!(f["authors"], "Kim, Minsu; Study Group");
        assert_eq!(f["year"], 2021);
        assert_eq!(f["venue"], "교육학 연구");
        assert!(f.get("url").is_none());
        assert_eq!(f["zotero_collections"], json!(["동기 연구"]));
        assert_eq!(
            f["zotero_link"],
            "https://www.zotero.org/users/1/items/ABCD1234"
        );
        let note = json!({ "key": "N1", "data": { "itemType": "note", "title": "x" } });
        assert!(reference_fields(&note, &names).is_none());

        let mine: Map<String, Value> = serde_json::from_value(json!({
            "read_status": "read", "summary": "내 요약", "tags": ["내 태그"], "authors": "old" }))
        .unwrap();
        let out = merged(&mine, f);
        assert_eq!(out["read_status"], "read");
        assert_eq!(out["summary"], "내 요약");
        assert_eq!(out["authors"], "Kim, Minsu; Study Group");
        assert_eq!(out["tags"], json!(["내 태그", "동기"]));
    }

    #[test]
    fn library_input_is_checked() {
        let ok = ZoteroLibrary {
            api_key: "abcDEF123".into(),
            library_type: "group".into(),
            library_id: "42".into(),
        };
        assert_eq!(library(&ok).unwrap().prefix, "groups/42");
        for (key, kind, id) in [
            ("a b", "user", "1"),
            ("abc", "team", "1"),
            ("abc", "user", "1/../2"),
        ] {
            let bad = ZoteroLibrary {
                api_key: key.into(),
                library_type: kind.into(),
                library_id: id.into(),
            };
            assert!(library(&bad).is_err());
        }
    }
}
