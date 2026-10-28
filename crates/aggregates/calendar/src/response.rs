//! 범위 필터와 달력 응답 변환.

use crate::events::{EventRow, to_event};
use crate::model::{
    Calendar, CalendarDate, CalendarDateKind, CalendarItem, CalendarTask, EventOccurrence,
    ProjectedTask,
};
use crate::sources::{DateRow, OccurrenceRow, ProjectedRow, TaskRow};
use fvoci_core::error::AppError;
use fvoci_core::ids::ItemId;
use fvoci_core::validate::format_date;
use fvoci_db::UserTx;
use std::collections::HashSet;
use uuid::Uuid;

pub(super) type ItemRow = (ItemId, Option<String>, String);

#[derive(Debug)]
pub(super) struct CalendarRows {
    pub task_rows: Vec<TaskRow>,
    pub projected_rows: Vec<ProjectedRow>,
    pub occurrence_rows: Vec<OccurrenceRow>,
    pub event_rows: Vec<EventRow>,
    pub date_rows: Vec<DateRow>,
}

impl CalendarRows {
    pub fn retain(&mut self, scope: &HashSet<ItemId>) {
        let keep = |item: &ItemId| scope.contains(item);
        self.task_rows.retain(|r| keep(&r.1));
        self.projected_rows.retain(|r| keep(&r.2));
        self.event_rows.retain(|e| keep(&e.item_id));
        let kept: std::collections::HashSet<Uuid> = self.event_rows.iter().map(|e| e.id).collect();
        self.occurrence_rows.retain(|r| kept.contains(&r.0));
        self.date_rows.retain(|r| keep(&r.2));
    }
    pub async fn items(&self, tx: &mut UserTx) -> Result<Vec<ItemRow>, AppError> {
        let mut item_ids: Vec<ItemId> = self.task_rows.iter().map(|r| r.1).collect();
        item_ids.extend(self.date_rows.iter().map(|r| r.2));
        item_ids.extend(self.projected_rows.iter().map(|r| r.2));
        item_ids.extend(self.event_rows.iter().map(|r| r.item_id));
        item_ids.sort_unstable_by_key(|id| id.0);
        item_ids.dedup();
        let items: Vec<(ItemId, Option<String>, String)> = sqlx::query_as(
        "SELECT i.id, app_item_reference(i.id), i.title FROM items i WHERE i.id = ANY($1) ORDER BY i.title, i.id",
    )
    .bind(&item_ids)
    .fetch_all(tx.conn())
    .await?;

        Ok(items)
    }
    pub fn into_calendar(self, items: Vec<ItemRow>) -> Result<Calendar, AppError> {
        let Self {
            task_rows,
            projected_rows,
            occurrence_rows,
            event_rows,
            date_rows,
        } = self;
        let tasks = tasks(task_rows)?;
        Ok(Calendar {
            tasks,
            projected_tasks: projected_rows
                .into_iter()
                .map(
                    |(
                        series_id,
                        root_task_id,
                        item_id,
                        title,
                        milestone,
                        occurrence,
                        start,
                        due,
                    )| {
                        ProjectedTask {
                            series_id,
                            root_task_id,
                            item_id,
                            title,
                            milestone,
                            occurrence: format_date(Some(occurrence)).unwrap_or_default(),
                            start_date: format_date(start),
                            due_date: format_date(due),
                        }
                    },
                )
                .collect(),
            events: event_rows
                .into_iter()
                .map(to_event)
                .collect::<Result<_, _>>()?,
            occurrences: occurrence_rows
                .into_iter()
                .map(
                    |(event_id, occurrence, start_date, end_date, starts_at, ends_at, skipped)| {
                        EventOccurrence {
                            event_id,
                            occurrence: format_date(Some(occurrence)).unwrap_or_default(),
                            start_date: format_date(start_date),
                            end_date: format_date(end_date),
                            starts_at,
                            ends_at,
                            skipped,
                        }
                    },
                )
                .collect(),
            dates: date_rows
                .into_iter()
                .map(|(kind, id, item_id, title, date, personal, done)| {
                    Ok(CalendarDate {
                        kind: CalendarDateKind::parse(&kind).ok_or_else(|| {
                            AppError::Config(format!("unknown calendar date {kind}"))
                        })?,
                        id,
                        item_id,
                        title,
                        date: format_date(Some(date)).unwrap_or_default(),
                        personal,
                        done,
                    })
                })
                .collect::<Result<_, AppError>>()?,
            items: items
                .into_iter()
                .map(|(id, reference, title)| CalendarItem {
                    id,
                    reference,
                    title,
                })
                .collect(),
        })
    }
}

fn tasks(task_rows: Vec<TaskRow>) -> Result<Vec<CalendarTask>, AppError> {
    task_rows
        .into_iter()
        .map(
            |(id, item_id, reference, title, status, status_category, milestone, start, due)| {
                Ok(CalendarTask {
                    id,
                    item_id,
                    reference,
                    title,
                    status,
                    status_category,
                    milestone,
                    start_date: format_date(start),
                    due_date: format_date(due),
                })
            },
        )
        .collect::<Result<_, AppError>>()
}
