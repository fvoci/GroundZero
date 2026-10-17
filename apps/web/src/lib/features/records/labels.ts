// 기록 종류·칸·고를 값·파일 역할의 표시 이름(D-81). 서버(`crates/server/src/features/records.rs`)는 한국어 이름을
// 그대로 보내고 언어를 모른다. 화면은 안정된 키로 메시지를 찾아 표시 언어로 바꾸고, 모르는 키면 서버 이름을 쓴다.
//
// 메시지 키(`messages/records/{ko,en}.json`, 서버 키를 소문자·[a-z0-9_]로 맞춘다):
//   records_kind_<kind>                       종류 이름
//   records_kind_<kind>_date                  기록 날짜 칸 이름(date_label)
//   records_field_<kind>_<field>              칸 이름
//   records_choice_<kind>_<field>_<value>     고를 값 이름
//   records_role_<kind>_<value>               파일 역할 이름
// 종류·칸 키에 밑줄이 있어도(research_log, log_type) 구분자 위치로 다른 키와 겹치지 않는다. 겹침은 표를
// 만들 때 확인했다. 한국어 값은 서버 이름과 같아야 한다. 서버에 종류·칸·값을 더하면 두 JSON과 아래 표에 함께 더한다.
//
// 메시지를 문자열 키로 `m`에서 바로 찾으면 타입 검사를 우회해야 하므로, 키 → 메시지 함수 표를 명시한다.
// 표의 함수는 부를 때 지금 언어로 글자를 만든다.
import type { ChoiceView, RecordFieldDef, RecordKindView } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';

const table: Readonly<Record<string, () => string>> = {
  records_kind_deliverable: m.records_kind_deliverable,
  records_kind_deliverable_date: m.records_kind_deliverable_date,
  records_field_deliverable_status: m.records_field_deliverable_status,
  records_choice_deliverable_status_planned: m.records_choice_deliverable_status_planned,
  records_choice_deliverable_status_in_progress: m.records_choice_deliverable_status_in_progress,
  records_choice_deliverable_status_done: m.records_choice_deliverable_status_done,
  records_choice_deliverable_status_accepted: m.records_choice_deliverable_status_accepted,
  records_field_deliverable_owner: m.records_field_deliverable_owner,
  records_field_deliverable_criteria: m.records_field_deliverable_criteria,
  records_role_deliverable_other: m.records_role_deliverable_other,
  records_kind_risk: m.records_kind_risk,
  records_kind_risk_date: m.records_kind_risk_date,
  records_field_risk_likelihood: m.records_field_risk_likelihood,
  records_choice_risk_likelihood_low: m.records_choice_risk_likelihood_low,
  records_choice_risk_likelihood_medium: m.records_choice_risk_likelihood_medium,
  records_choice_risk_likelihood_high: m.records_choice_risk_likelihood_high,
  records_field_risk_impact: m.records_field_risk_impact,
  records_choice_risk_impact_low: m.records_choice_risk_impact_low,
  records_choice_risk_impact_medium: m.records_choice_risk_impact_medium,
  records_choice_risk_impact_high: m.records_choice_risk_impact_high,
  records_field_risk_status: m.records_field_risk_status,
  records_choice_risk_status_open: m.records_choice_risk_status_open,
  records_choice_risk_status_mitigating: m.records_choice_risk_status_mitigating,
  records_choice_risk_status_closed: m.records_choice_risk_status_closed,
  records_field_risk_owner: m.records_field_risk_owner,
  records_field_risk_mitigation: m.records_field_risk_mitigation,
  records_kind_issue: m.records_kind_issue,
  records_kind_issue_date: m.records_kind_issue_date,
  records_field_issue_severity: m.records_field_issue_severity,
  records_choice_issue_severity_low: m.records_choice_issue_severity_low,
  records_choice_issue_severity_medium: m.records_choice_issue_severity_medium,
  records_choice_issue_severity_high: m.records_choice_issue_severity_high,
  records_choice_issue_severity_critical: m.records_choice_issue_severity_critical,
  records_field_issue_status: m.records_field_issue_status,
  records_choice_issue_status_open: m.records_choice_issue_status_open,
  records_choice_issue_status_in_progress: m.records_choice_issue_status_in_progress,
  records_choice_issue_status_resolved: m.records_choice_issue_status_resolved,
  records_choice_issue_status_closed: m.records_choice_issue_status_closed,
  records_field_issue_owner: m.records_field_issue_owner,
  records_field_issue_resolution: m.records_field_issue_resolution,
  records_kind_retrospective: m.records_kind_retrospective,
  records_kind_retrospective_date: m.records_kind_retrospective_date,
  records_field_retrospective_went_well: m.records_field_retrospective_went_well,
  records_field_retrospective_to_improve: m.records_field_retrospective_to_improve,
  records_field_retrospective_actions: m.records_field_retrospective_actions,
  records_kind_reference: m.records_kind_reference,
  records_field_reference_authors: m.records_field_reference_authors,
  records_field_reference_year: m.records_field_reference_year,
  records_field_reference_venue: m.records_field_reference_venue,
  records_field_reference_doi: m.records_field_reference_doi,
  records_field_reference_url: m.records_field_reference_url,
  records_field_reference_read_status: m.records_field_reference_read_status,
  records_choice_reference_read_status_unread: m.records_choice_reference_read_status_unread,
  records_choice_reference_read_status_reading: m.records_choice_reference_read_status_reading,
  records_choice_reference_read_status_read: m.records_choice_reference_read_status_read,
  records_choice_reference_read_status_skimmed: m.records_choice_reference_read_status_skimmed,
  records_field_reference_tags: m.records_field_reference_tags,
  records_field_reference_summary: m.records_field_reference_summary,
  records_field_reference_method_note: m.records_field_reference_method_note,
  records_field_reference_findings: m.records_field_reference_findings,
  records_field_reference_limitations: m.records_field_reference_limitations,
  records_field_reference_citation: m.records_field_reference_citation,
  records_field_reference_pages: m.records_field_reference_pages,
  records_field_reference_zotero_key: m.records_field_reference_zotero_key,
  records_field_reference_zotero_link: m.records_field_reference_zotero_link,
  records_field_reference_zotero_collections: m.records_field_reference_zotero_collections,
  records_role_reference_reference: m.records_role_reference_reference,
  records_kind_research_log: m.records_kind_research_log,
  records_kind_research_log_date: m.records_kind_research_log_date,
  records_field_research_log_log_type: m.records_field_research_log_log_type,
  records_choice_research_log_log_type_experiment:
    m.records_choice_research_log_log_type_experiment,
  records_choice_research_log_log_type_interview: m.records_choice_research_log_log_type_interview,
  records_choice_research_log_log_type_lab_meeting:
    m.records_choice_research_log_log_type_lab_meeting,
  records_choice_research_log_log_type_feedback: m.records_choice_research_log_log_type_feedback,
  records_choice_research_log_log_type_decision: m.records_choice_research_log_log_type_decision,
  records_choice_research_log_log_type_design_change:
    m.records_choice_research_log_log_type_design_change,
  records_field_research_log_participants: m.records_field_research_log_participants,
  records_role_research_log_other: m.records_role_research_log_other,
  records_kind_dataset: m.records_kind_dataset,
  records_kind_dataset_date: m.records_kind_dataset_date,
  records_field_dataset_stage: m.records_field_dataset_stage,
  records_choice_dataset_stage_raw: m.records_choice_dataset_stage_raw,
  records_choice_dataset_stage_cleaned: m.records_choice_dataset_stage_cleaned,
  records_choice_dataset_stage_analysis: m.records_choice_dataset_stage_analysis,
  records_field_dataset_source: m.records_field_dataset_source,
  records_field_dataset_location: m.records_field_dataset_location,
  records_field_dataset_data_version: m.records_field_dataset_data_version,
  records_field_dataset_quality: m.records_field_dataset_quality,
  records_field_dataset_access_note: m.records_field_dataset_access_note,
  records_role_dataset_input: m.records_role_dataset_input,
  records_kind_variable: m.records_kind_variable,
  records_field_variable_label: m.records_field_variable_label,
  records_field_variable_var_type: m.records_field_variable_var_type,
  records_choice_variable_var_type_numeric: m.records_choice_variable_var_type_numeric,
  records_choice_variable_var_type_categorical: m.records_choice_variable_var_type_categorical,
  records_choice_variable_var_type_text: m.records_choice_variable_var_type_text,
  records_choice_variable_var_type_date: m.records_choice_variable_var_type_date,
  records_choice_variable_var_type_other: m.records_choice_variable_var_type_other,
  records_field_variable_values: m.records_field_variable_values,
  records_field_variable_measurement: m.records_field_variable_measurement,
  records_field_variable_dataset: m.records_field_variable_dataset,
  records_kind_analysis: m.records_kind_analysis,
  records_kind_analysis_date: m.records_kind_analysis_date,
  records_field_analysis_purpose: m.records_field_analysis_purpose,
  records_field_analysis_owner: m.records_field_analysis_owner,
  records_field_analysis_status: m.records_field_analysis_status,
  records_choice_analysis_status_planned: m.records_choice_analysis_status_planned,
  records_choice_analysis_status_running: m.records_choice_analysis_status_running,
  records_choice_analysis_status_done: m.records_choice_analysis_status_done,
  records_choice_analysis_status_failed: m.records_choice_analysis_status_failed,
  records_field_analysis_settings: m.records_field_analysis_settings,
  records_field_analysis_log: m.records_field_analysis_log,
  records_field_analysis_compute_minutes: m.records_field_analysis_compute_minutes,
  records_field_analysis_results: m.records_field_analysis_results,
  records_field_analysis_compared_with: m.records_field_analysis_compared_with,
  records_field_analysis_comparison: m.records_field_analysis_comparison,
  records_role_analysis_input: m.records_role_analysis_input,
  records_role_analysis_code: m.records_role_analysis_code,
  records_role_analysis_result: m.records_role_analysis_result,
  records_kind_submission: m.records_kind_submission,
  records_kind_submission_date: m.records_kind_submission_date,
  records_field_submission_venue: m.records_field_submission_venue,
  records_field_submission_status: m.records_field_submission_status,
  records_choice_submission_status_preparing: m.records_choice_submission_status_preparing,
  records_choice_submission_status_submitted: m.records_choice_submission_status_submitted,
  records_choice_submission_status_under_review: m.records_choice_submission_status_under_review,
  records_choice_submission_status_revision: m.records_choice_submission_status_revision,
  records_choice_submission_status_accepted: m.records_choice_submission_status_accepted,
  records_choice_submission_status_published: m.records_choice_submission_status_published,
  records_choice_submission_status_rejected: m.records_choice_submission_status_rejected,
  records_field_submission_revision_due: m.records_field_submission_revision_due,
  records_field_submission_coauthors: m.records_field_submission_coauthors,
  records_field_submission_checklist: m.records_field_submission_checklist,
  records_field_submission_irb: m.records_field_submission_irb,
  records_role_submission_other: m.records_role_submission_other,
  records_kind_material: m.records_kind_material,
  records_field_material_material_type: m.records_field_material_material_type,
  records_choice_material_material_type_book: m.records_choice_material_material_type_book,
  records_choice_material_material_type_paper: m.records_choice_material_material_type_paper,
  records_choice_material_material_type_workbook: m.records_choice_material_material_type_workbook,
  records_choice_material_material_type_web: m.records_choice_material_material_type_web,
  records_choice_material_material_type_course: m.records_choice_material_material_type_course,
  records_choice_material_material_type_other: m.records_choice_material_material_type_other,
  records_field_material_course: m.records_field_material_course,
  records_field_material_url: m.records_field_material_url,
  records_field_material_amount: m.records_field_material_amount,
  records_field_material_chapters: m.records_field_material_chapters,
  records_role_material_other: m.records_role_material_other,
  records_kind_meeting: m.records_kind_meeting,
  records_kind_meeting_date: m.records_kind_meeting_date,
  records_field_meeting_presenter: m.records_field_meeting_presenter,
  records_field_meeting_assignment: m.records_field_meeting_assignment,
  records_field_meeting_attendees: m.records_field_meeting_attendees,
  records_role_meeting_other: m.records_role_meeting_other,
  records_kind_exam: m.records_kind_exam,
  records_kind_exam_date: m.records_kind_exam_date,
  records_field_exam_taker: m.records_field_exam_taker,
  records_field_exam_score: m.records_field_exam_score,
  records_field_exam_max_score: m.records_field_exam_max_score,
  records_field_exam_feedback: m.records_field_exam_feedback,
  records_role_exam_other: m.records_role_exam_other,
  records_kind_review: m.records_kind_review,
  records_kind_review_date: m.records_kind_review_date,
  records_field_review_topic: m.records_field_review_topic,
  records_field_review_weakness: m.records_field_review_weakness,
  records_field_review_rounds: m.records_field_review_rounds,
  records_kind_supply: m.records_kind_supply,
  records_field_supply_supply_type: m.records_field_supply_supply_type,
  records_choice_supply_supply_type_material: m.records_choice_supply_supply_type_material,
  records_choice_supply_supply_type_equipment: m.records_choice_supply_supply_type_equipment,
  records_field_supply_state: m.records_field_supply_state,
  records_choice_supply_state_owned: m.records_choice_supply_state_owned,
  records_choice_supply_state_needed: m.records_choice_supply_state_needed,
  records_choice_supply_state_preparing: m.records_choice_supply_state_preparing,
  records_choice_supply_state_ready: m.records_choice_supply_state_ready,
  records_field_supply_quantity: m.records_field_supply_quantity,
  records_field_supply_settings: m.records_field_supply_settings,
  records_kind_equipment_use: m.records_kind_equipment_use,
  records_kind_equipment_use_date: m.records_kind_equipment_use_date,
  records_field_equipment_use_equipment: m.records_field_equipment_use_equipment,
  records_field_equipment_use_settings: m.records_field_equipment_use_settings,
  records_kind_activity: m.records_kind_activity,
  records_kind_activity_date: m.records_kind_activity_date,
  records_field_activity_minutes: m.records_field_activity_minutes,
  records_field_activity_participants: m.records_field_activity_participants,
  records_field_activity_result: m.records_field_activity_result,
  records_field_activity_learned: m.records_field_activity_learned,
  records_role_activity_other: m.records_role_activity_other,
  records_kind_portfolio: m.records_kind_portfolio,
  records_kind_portfolio_date: m.records_kind_portfolio_date,
  records_field_portfolio_retrospective: m.records_field_portfolio_retrospective,
  records_field_portfolio_next_goal: m.records_field_portfolio_next_goal,
  records_role_portfolio_result: m.records_role_portfolio_result,
};

const norm = (s: string): string => s.toLowerCase().replace(/[^a-z0-9_]/g, '_');

function lookup(key: string, fallback: string): string {
  return table[key]?.() ?? fallback;
}

/** 기록 종류 이름. */
export function recordKindLabel(def: RecordKindView): string {
  return lookup(`records_kind_${norm(def.kind)}`, def.label);
}

/** 기록 날짜 칸 이름. 날짜를 받지 않는 종류면 null. */
export function recordDateLabel(def: RecordKindView): string | null {
  return def.date_label === null
    ? null
    : lookup(`records_kind_${norm(def.kind)}_date`, def.date_label);
}

/** 칸 이름. */
export function recordFieldLabel(def: RecordKindView, field: RecordFieldDef): string {
  return lookup(`records_field_${norm(def.kind)}_${norm(field.key)}`, field.label);
}

/** 고를 값 이름. */
export function recordChoiceLabel(
  def: RecordKindView,
  field: RecordFieldDef,
  choice: ChoiceView,
): string {
  return lookup(
    `records_choice_${norm(def.kind)}_${norm(field.key)}_${norm(choice.value)}`,
    choice.label,
  );
}

/** 파일 역할 이름. */
export function recordRoleLabel(def: RecordKindView, role: ChoiceView): string {
  return lookup(`records_role_${norm(def.kind)}_${norm(role.value)}`, role.label);
}
