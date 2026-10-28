//! 기록 종류와 입력 칸 정의.

use FieldType::{Course, Date as Day, Episode, LongText, Number, People, Person, Tags, Text, Url};
use ModuleKind::{Hobby, Project, Research, Study};
use axum::Json;
use fvoci_core::module::ModuleKind;
use fvoci_http::session::CurrentUser;
use serde::Serialize;
use utoipa::ToSchema;

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
    /// 이 강좌(기록이 붙은 강의 모듈 항목)의 회차 하나(회차 id, D-123 질문·복습).
    Episode,
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
pub fn record_kinds() -> Vec<RecordKindDef> {
    vec![
        project_deliverable(),
        project_risk(),
        project_issue(),
        project_retrospective(),
        research_reference(),
        research_research_log(),
        research_dataset(),
        research_variable(),
        research_analysis(),
        research_submission(),
        study_material(),
        study_meeting(),
        study_exam(),
        study_review(),
        course_question(),
        course_review(),
        hobby_supply(),
        hobby_equipment_use(),
        hobby_activity(),
        hobby_portfolio(),
    ]
}

/// 기록 종류 정의 전체(화면이 입력 칸을 그린다).
#[utoipa::path(get, path = "/api/record-kinds", operation_id = "record_kinds",
    responses((status = 200, body = [RecordKindDef])), tag = "records")]
pub async fn kinds(_user: CurrentUser) -> Json<Vec<RecordKindDef>> {
    Json(record_kinds())
}

fn project_deliverable() -> RecordKindDef {
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
    )
}

fn project_risk() -> RecordKindDef {
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
    )
}

fn project_issue() -> RecordKindDef {
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
    )
}

fn project_retrospective() -> RecordKindDef {
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
    )
}

fn research_reference() -> RecordKindDef {
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
    )
}

fn research_research_log() -> RecordKindDef {
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
    )
}

fn research_dataset() -> RecordKindDef {
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
    )
}

fn research_variable() -> RecordKindDef {
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
    )
}

fn research_analysis() -> RecordKindDef {
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
    )
}

fn research_submission() -> RecordKindDef {
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
    )
}

fn study_material() -> RecordKindDef {
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
    )
}

fn study_meeting() -> RecordKindDef {
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
    )
}

fn study_exam() -> RecordKindDef {
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
    )
}

fn study_review() -> RecordKindDef {
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
    )
}

fn course_question() -> RecordKindDef {
    kind(
        "question",
        "질문",
        ModuleKind::Course,
        Some("질문한 날"),
        vec![
            f("episode", "회차", Episode),
            f("answer", "답변", LongText),
            choice(
                "state",
                "상태",
                &[c("open", "미해결"), c("answered", "답변됨")],
            ),
        ],
        &ATTACH,
    )
}

fn course_review() -> RecordKindDef {
    kind(
        "review",
        "복습",
        ModuleKind::Course,
        Some("다음 복습일"),
        vec![
            f("episode", "회차", Episode),
            f("weakness", "취약 영역", LongText),
            f("rounds", "반복 횟수", Number),
        ],
        &[],
    )
}

fn hobby_supply() -> RecordKindDef {
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
    )
}

fn hobby_equipment_use() -> RecordKindDef {
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
    )
}

fn hobby_activity() -> RecordKindDef {
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
    )
}

fn hobby_portfolio() -> RecordKindDef {
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
    )
}
