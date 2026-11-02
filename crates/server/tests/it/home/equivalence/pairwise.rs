//! #291: witness the pairs exercised by frozen JSON comparisons.
use super::{Fixture, Result};
use serde_json::{Value, json};
use std::collections::HashSet;
use uuid::Uuid;

const ZONES: [Option<&str>; 8] = [
    None,
    Some(""),
    Some("  "),
    Some(" UTC "),
    Some("Asia/Seoul"),
    Some("America/New_York"),
    Some("Pacific/Kiritimati"),
    Some("Nowhere/Land"),
];

#[derive(Default)]
pub(super) struct Coverage {
    rows: Vec<[usize; 4]>,
}

impl Coverage {
    pub(super) fn boundary(&mut self, size: usize) {
        // Exact and trimmed UTC exercise the same explicit-zone class.
        self.record([size, 3, 0, 0]);
    }

    fn record(&mut self, row: [usize; 4]) {
        self.rows.push(row);
    }

    pub(super) fn assert_complete(&self) {
        let cardinalities = [8, 8, 4, 2];
        let names = [
            "row boundary",
            "timezone input",
            "organization",
            "saved timezone",
        ];
        for row in &self.rows {
            for (axis, &value) in row.iter().enumerate() {
                assert!(value < cardinalities[axis], "{} out of range", names[axis]);
            }
        }
        let primary: HashSet<_> = self
            .rows
            .iter()
            .filter(|r| r[2] == 0)
            .map(|r| (r[0], r[1]))
            .collect();
        assert_eq!(
            primary.len(),
            64,
            "each row/timezone pair must reach the primary organization"
        );
        for left in 0..4 {
            for right in left + 1..4 {
                let pairs: HashSet<_> = self.rows.iter().map(|r| (r[left], r[right])).collect();
                assert_eq!(
                    pairs.len(),
                    cardinalities[left] * cardinalities[right],
                    "missing pairwise coverage: {} / {}",
                    names[left],
                    names[right]
                );
            }
        }
    }
}

impl Fixture {
    fn pairwise_scopes(&self) -> [Option<Uuid>; 4] {
        [
            Some(self.organizations[0]),
            Some(self.organizations[1]),
            Some(Uuid::now_v7()),
            None,
        ]
    }

    async fn saved_timezone(&mut self, setting: Option<&str>) -> Result<()> {
        sqlx::query("DELETE FROM fvoci.user_settings WHERE user_id=$1")
            .bind(self.owner.id.0)
            .execute(&mut self.ddl)
            .await?;
        if let Some(setting) = setting {
            sqlx::query("INSERT INTO fvoci.user_settings(user_id,time_zone) VALUES($1,$2)")
                .bind(self.owner.id.0)
                .bind(setting)
                .execute(&mut self.ddl)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn boundary_pairs(
        &mut self,
        size: usize,
        coverage: &mut Coverage,
    ) -> Result<()> {
        for (zone_index, tz) in ZONES.into_iter().enumerate() {
            let setting_index = (size + zone_index / 4) % 2;
            self.saved_timezone([None, Some("America/New_York")][setting_index])
                .await?;
            self.compare(Some(self.owner.id.0), Some(self.organizations[0]), tz)
                .await?;
            coverage.record([size, zone_index, 0, setting_index]);
            let scope_index = (size + zone_index) % 4;
            if scope_index != 0 {
                self.compare(
                    Some(self.owner.id.0),
                    self.pairwise_scopes()[scope_index],
                    tz,
                )
                .await?;
                coverage.record([size, zone_index, scope_index, setting_index]);
            }
        }
        self.saved_timezone(None).await
    }

    fn assert_scope_value(
        &self,
        actor: Option<Uuid>,
        org: Option<Uuid>,
        tz: Option<&str>,
        value: &Value,
    ) {
        if actor == Some(self.organization_only.id.0)
            && org == Some(self.organizations[0])
            && tz == Some("Asia/Seoul")
        {
            assert_eq!(value["value"]["pins"], json!([]));
            assert_eq!(value["value"]["up_next"], json!([]));
        }
        if actor.is_none() || org.is_none() {
            assert_eq!(
                value["code"], "P0002",
                "organization rejection precedes timezone validation"
            );
        }
    }

    pub(super) async fn scope_timezone_pairs(&mut self, coverage: &mut Coverage) -> Result<()> {
        for (setting_index, setting) in [None, Some("America/New_York")].into_iter().enumerate() {
            self.saved_timezone(setting).await?;
            for actor in [
                Some(self.owner.id.0),
                Some(self.reader.id.0),
                Some(self.member.id.0),
                Some(self.organization_only.id.0),
                Some(Uuid::now_v7()),
                None,
            ] {
                for (scope_index, org) in self.pairwise_scopes().into_iter().enumerate() {
                    for (zone_index, tz) in ZONES.into_iter().enumerate() {
                        let value = self.compare(actor, org, tz).await?;
                        if actor == Some(self.owner.id.0) {
                            coverage.record([7, zone_index, scope_index, setting_index]);
                        }
                        self.assert_scope_value(actor, org, tz, &value);
                    }
                }
            }
        }
        Ok(())
    }
}
