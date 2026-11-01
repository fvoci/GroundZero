//! #291: fully cross roles, visibility, archive and trash; keep literal grade witnesses.
use super::{Fixture, Result};
use crate::common::{self, TestUser};
use crate::sql_support::made;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
enum Role {
    Owner,
    OrganizationAdmin,
    SpaceAdmin,
    Member,
    Readonly,
    Direct,
    Group,
    Creator,
    Outside,
    Foreign,
    ServerAdmin,
    JoinedAdmin,
    Removed,
}

impl Role {
    fn workspace(self) -> Option<&'static str> {
        match self {
            Self::Owner | Self::SpaceAdmin | Self::JoinedAdmin => Some("admin"),
            Self::Member | Self::Direct | Self::Group | Self::Creator => Some("member"),
            Self::Readonly => Some("readonly"),
            _ => None,
        }
    }

    fn item_grade(self, position: usize, visibility: &str) -> &'static str {
        match self.workspace() {
            Some("admin") => "owner",
            Some(role) => {
                let creator = matches!(self, Self::Creator) && position == 3;
                let relation = matches!(self, Self::Direct | Self::Readonly) && position == 1
                    || matches!(self, Self::Group) && position == 2;
                if visibility == "private" && !creator && !relation {
                    "none"
                } else if role == "readonly" {
                    "read"
                } else if creator {
                    "owner"
                } else {
                    "write"
                }
            }
            _ => "none",
        }
    }
}

struct Actor {
    role: Role,
    user: TestUser,
}

struct Matrix {
    fixture: Fixture,
    actors: Vec<Actor>,
    module: Uuid,
    items: Vec<Uuid>,
    tasks: Vec<Uuid>,
    events: Vec<Uuid>,
    documents: Vec<Uuid>,
}

impl Matrix {
    async fn document_state(&mut self, mode: &str) -> Result<()> {
        let f = &mut self.fixture;
        sqlx::query("UPDATE fvoci.documents SET visibility=$2 WHERE id=ANY($1)")
            .bind(&self.documents)
            .bind(if mode == "workspace" {
                "workspace"
            } else {
                "private"
            })
            .execute(&mut f.ddl)
            .await?;
        sqlx::query("DELETE FROM fvoci.document_shares WHERE document_id=ANY($1)")
            .bind(&self.documents)
            .execute(&mut f.ddl)
            .await?;
        if matches!(mode, "read" | "write") {
            for doc in &self.documents {
                for actor in &self.actors {
                    sqlx::query("INSERT INTO fvoci.document_shares(id,document_id,organization_id,grantee_user_id,permission,granted_by)
                        VALUES($1,$2,$3,$4,$5,$6)")
                        .bind(Uuid::now_v7()).bind(doc).bind(f.organizations[0]).bind(actor.user.id.0)
                        .bind(mode).bind(f.owner.id.0).execute(&mut f.ddl).await?;
                }
            }
        }
        Ok(())
    }

    async fn document_grades(
        &mut self,
        actor: &Actor,
        visibility: &str,
        archive: &str,
        trash: &str,
        mode: &str,
    ) -> Result<()> {
        for (position, doc) in self.documents.iter().enumerate() {
            let grade = if matches!(trash, "document" | "item" | "module" | "workspace") {
                "none"
            } else if matches!(
                actor.role,
                Role::Owner | Role::SpaceAdmin | Role::JoinedAdmin
            ) {
                "owner"
            } else if matches!(actor.role, Role::Foreign | Role::ServerAdmin) {
                "none"
            } else if mode == "write" && actor.role.workspace() == Some("member") {
                "write"
            } else if matches!(mode, "read" | "write") {
                "read"
            } else if mode == "workspace" {
                match actor.role.item_grade(position, visibility) {
                    "owner" => "write",
                    grade => grade,
                }
            } else {
                "none"
            };
            let grade = if archive != "none" && matches!(grade, "owner" | "write") {
                "read"
            } else {
                grade
            };
            let actual: (String, bool) = sqlx::query_as(
                "SELECT app_access('document',$1),EXISTS(SELECT 1 FROM documents WHERE id=$1)",
            )
            .bind(doc)
            .fetch_one(&mut self.fixture.read)
            .await?;
            assert_eq!(
                actual,
                (grade.to_owned(), grade != "none"),
                "document {:?}/{position}/{visibility}/{archive}/{trash}/{mode}",
                actor.role
            );
        }
        Ok(())
    }

    async fn new() -> Result<Self> {
        let mut fixture = Fixture::new().await?;
        sqlx::query("UPDATE fvoci.items SET archived_at=now()")
            .execute(&mut fixture.ddl)
            .await?;
        sqlx::raw_sql(include_str!(
            "../../../fixtures/home/calendar_reference.sql"
        ))
        .execute(&mut fixture.ddl)
        .await?;
        sqlx::raw_sql(
            "CREATE FUNCTION fvoci.calendar_outcome(ws uuid,reference boolean) RETURNS jsonb
             LANGUAGE plpgsql STABLE SECURITY INVOKER SET search_path=fvoci,pg_temp AS $$ BEGIN
               IF reference THEN RETURN jsonb_build_object('value',calendar_reference(ws,'2026-11-01','2026-11-07','UTC')); END IF;
               RETURN jsonb_build_object('value',app_calendar(ws,'2026-11-01','2026-11-07','UTC'));
             EXCEPTION WHEN OTHERS THEN RETURN jsonb_build_object('code',SQLSTATE,'message',SQLERRM);
             END $$;
             REVOKE ALL ON FUNCTION fvoci.calendar_outcome(uuid,boolean) FROM PUBLIC;
             GRANT EXECUTE ON FUNCTION fvoci.calendar_outcome(uuid,boolean) TO fvoci_app;",
        ).execute(&mut fixture.ddl).await?;
        let mut matrix = Self {
            fixture,
            actors: Vec::new(),
            module: Uuid::nil(),
            items: Vec::new(),
            tasks: Vec::new(),
            events: Vec::new(),
            documents: Vec::new(),
        };
        matrix.actors().await?;
        matrix.contents().await?;
        Ok(matrix)
    }

    async fn actors(&mut self) -> Result<()> {
        let f = &mut self.fixture;
        self.actors.extend([
            Actor {
                role: Role::Owner,
                user: f.owner.clone(),
            },
            Actor {
                role: Role::Member,
                user: f.member.clone(),
            },
            Actor {
                role: Role::Readonly,
                user: f.reader.clone(),
            },
            Actor {
                role: Role::Outside,
                user: f.organization_only.clone(),
            },
        ]);
        for role in [
            Role::OrganizationAdmin,
            Role::SpaceAdmin,
            Role::Direct,
            Role::Group,
            Role::Creator,
            Role::Foreign,
            Role::ServerAdmin,
            Role::JoinedAdmin,
            Role::Removed,
        ] {
            let label = format!("aggregate-{role:?}");
            let user =
                common::login_with_email(&f.state, &label, &common::unique_email(&label), true)
                    .await;
            if !matches!(role, Role::Foreign | Role::ServerAdmin) {
                sqlx::query("INSERT INTO fvoci.organization_members(organization_id,user_id,role) VALUES($1,$2,$3)")
                    .bind(f.organizations[0]).bind(user.id.0)
                    .bind(if matches!(role, Role::OrganizationAdmin | Role::JoinedAdmin) { "admin" } else { "member" })
                    .execute(&mut f.ddl).await?;
            }
            if let Some(space_role) = role.workspace() {
                sqlx::query("INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,$3)")
                    .bind(f.workspace).bind(user.id.0).bind(space_role).execute(&mut f.ddl).await?;
            }
            if matches!(role, Role::ServerAdmin) {
                sqlx::query("INSERT INTO fvoci.server_admins(user_id) VALUES($1)")
                    .bind(user.id.0)
                    .execute(&mut f.ddl)
                    .await?;
            }
            self.actors.push(Actor { role, user });
        }
        Ok(())
    }

    async fn contents(&mut self) -> Result<()> {
        let f = &mut self.fixture;
        self.module = made(
            &f.state,
            &f.owner,
            "SELECT fvoci.app_create_module($1,$2,'project','matrix','MAT',@ids) AS id",
            &[f.workspace, Uuid::now_v7()],
        )
        .await;
        for position in 0..5 {
            let item = made(
                &f.state,
                &f.owner,
                "SELECT fvoci.app_create_item($1,$2,'matrix','',@ids) AS id",
                &[self.module, Uuid::now_v7()],
            )
            .await;
            self.items.push(item);
            if position < 4 {
                for status in ["할 일", "완료"] {
                    let task = made(&f.state, &f.owner,
                        &format!("SELECT fvoci.app_create_task($1,$2,'matrix',NULL,'','{status}','none',NULL,NULL,NULL,'{{}}','{{}}',false,@ids) AS id"),
                        &[item, Uuid::now_v7()]).await;
                    self.tasks.push(task);
                }
                let doc = made(&f.state, &f.owner,
                    "SELECT fvoci.app_create_module_document($1,$2,'matrix',1,'{}',NULL,20,@ids) AS id",
                    &[item, Uuid::now_v7()]).await;
                self.documents.push(doc);
                let event = Uuid::now_v7();
                sqlx::query("INSERT INTO fvoci.events(id,workspace_id,item_id,kind,title,all_day,start_date,end_date,timezone,created_by,updated_by,request_id,recurrence)
                    VALUES($1,$2,$3,'event','matrix',true,'2026-11-01','2026-11-01','UTC',$4,$4,$1,
                    '{\"freq\":\"daily\",\"interval\":1,\"weekdays\":[],\"count\":4,\"until\":null}')")
                    .bind(event).bind(f.workspace).bind(item).bind(f.owner.id.0).execute(&mut f.ddl).await?;
                self.events.push(event);
            }
        }
        self.relations().await?;
        Ok(())
    }

    async fn relations(&mut self) -> Result<()> {
        for actor in &self.actors {
            if matches!(actor.role, Role::Direct | Role::Readonly) {
                sqlx::query(
                    "INSERT INTO fvoci.item_members(item_id,user_id,added_by) VALUES($1,$2,$3)",
                )
                .bind(self.items[1])
                .bind(actor.user.id.0)
                .bind(self.fixture.owner.id.0)
                .execute(&mut self.fixture.ddl)
                .await?;
            }
            if matches!(actor.role, Role::Creator) {
                sqlx::query("UPDATE fvoci.items SET created_by=$2 WHERE id=$1")
                    .bind(self.items[3])
                    .bind(actor.user.id.0)
                    .execute(&mut self.fixture.ddl)
                    .await?;
            }
            if matches!(actor.role, Role::Group) {
                let group = Uuid::now_v7();
                sqlx::query(
                    "INSERT INTO fvoci.groups(id,workspace_id,name) VALUES($1,$2,'matrix')",
                )
                .bind(group)
                .bind(self.fixture.workspace)
                .execute(&mut self.fixture.ddl)
                .await?;
                sqlx::query("INSERT INTO fvoci.group_members(group_id,user_id) VALUES($1,$2)")
                    .bind(group)
                    .bind(actor.user.id.0)
                    .execute(&mut self.fixture.ddl)
                    .await?;
                sqlx::query(
                    "INSERT INTO fvoci.item_members(item_id,group_id,added_by) VALUES($1,$2,$3)",
                )
                .bind(self.items[2])
                .bind(group)
                .bind(self.fixture.owner.id.0)
                .execute(&mut self.fixture.ddl)
                .await?;
            }
        }
        Ok(())
    }

    async fn state(&mut self, visibility: &str, archive: &str, trash: &str) -> Result<()> {
        let f = &mut self.fixture;
        sqlx::query(
            "UPDATE fvoci.items SET visibility=$2, archived_at=CASE WHEN $3 THEN now() END,
            deleted_at=CASE WHEN $4 THEN now() END,updated_at='2026-01-01' WHERE id=ANY($1)",
        )
        .bind(&self.items)
        .bind(visibility)
        .bind(matches!(archive, "item" | "both"))
        .bind(trash == "item")
        .execute(&mut f.ddl)
        .await?;
        sqlx::query(
            "UPDATE fvoci.modules SET archived_at=CASE WHEN $2 THEN now() END,
            deleted_at=CASE WHEN $3 THEN now() END WHERE id=$1",
        )
        .bind(self.module)
        .bind(matches!(archive, "module" | "both"))
        .bind(trash == "module")
        .execute(&mut f.ddl)
        .await?;
        sqlx::query(
            "UPDATE fvoci.workspaces SET deleted_at=CASE WHEN $2 THEN now() END WHERE id=$1",
        )
        .bind(f.workspace)
        .bind(trash == "workspace")
        .execute(&mut f.ddl)
        .await?;
        for (table, values, kind) in [
            ("tasks", &self.tasks, "task"),
            ("events", &self.events, "event"),
            ("documents", &self.documents, "document"),
        ] {
            sqlx::query(&format!(
                "UPDATE fvoci.{table} SET deleted_at=CASE WHEN $2 THEN now() END WHERE id=ANY($1)"
            ))
            .bind(values)
            .bind(trash == kind)
            .execute(&mut f.ddl)
            .await?;
        }
        Ok(())
    }

    async fn compare(
        &mut self,
        actor: &Actor,
        visibility: &str,
        archive: &str,
        trash: &str,
        document_mode: &str,
    ) -> Result<()> {
        let f = &mut self.fixture;
        sqlx::query("UPDATE fvoci.documents SET updated_by=$2 WHERE id=ANY($1)")
            .bind(&self.documents)
            .bind(actor.user.id.0)
            .execute(&mut f.ddl)
            .await?;
        let home = f
            .compare(Some(actor.user.id.0), Some(f.organizations[0]), Some("UTC"))
            .await?;
        for (position, item) in self.items.iter().enumerate() {
            let grade = if matches!(trash, "item" | "module" | "workspace") {
                "none"
            } else {
                actor.role.item_grade(position, visibility)
            };
            let actual: (String, bool) = sqlx::query_as(
                "SELECT app_access('item',$1),EXISTS(SELECT 1 FROM items WHERE id=$1)",
            )
            .bind(item)
            .fetch_one(&mut f.read)
            .await?;
            assert_eq!(
                actual,
                (grade.to_owned(), grade != "none"),
                "{:?}/{position}/{visibility}/{archive}/{trash}",
                actor.role
            );
            if let Some(active) = home["value"]["active_items"].as_array() {
                let row = active.iter().find(|r| r["id"] == item.to_string());
                assert_eq!(row.is_some(), grade != "none" && archive == "none");
                if let Some(row) = row {
                    assert_eq!(row["can_write"], matches!(grade, "owner" | "write"));
                    let count = if position == 4 || trash == "task" {
                        0
                    } else {
                        2
                    };
                    assert_eq!(
                        row["progress"],
                        json!({"kind":"tasks","done":count/2,"total":count})
                    );
                }
            }
        }
        self.document_grades(actor, visibility, archive, trash, document_mode)
            .await?;
        if document_mode == "workspace" {
            self.calendar(actor, visibility, archive, trash).await?;
        }
        Ok(())
    }

    async fn calendar(
        &mut self,
        actor: &Actor,
        visibility: &str,
        archive: &str,
        trash: &str,
    ) -> Result<()> {
        let f = &mut self.fixture;
        let pair: (Value, Value) =
            sqlx::query_as("SELECT calendar_outcome($1,true),calendar_outcome($1,false)")
                .bind(f.workspace)
                .fetch_one(&mut f.read)
                .await?;
        assert_eq!(
            pair.0, pair.1,
            "calendar {:?}/{visibility}/{archive}/{trash}",
            actor.role
        );
        if actor.role.workspace().is_none() || trash == "workspace" {
            assert_eq!(pair.1["code"], "P0002");
        } else {
            let expected = if matches!(trash, "item" | "module" | "event") {
                0
            } else {
                (0..4)
                    .filter(|&i| actor.role.item_grade(i, visibility) != "none")
                    .count()
            };
            let events = pair.1["value"]["events"].as_array().ok_or("events")?;
            let occurrences = pair.1["value"]["occurrences"]
                .as_array()
                .ok_or("occurrences")?;
            assert_eq!(events.len(), expected);
            assert_eq!(
                occurrences.len(),
                expected * 4,
                "all recurrence rows survive"
            );
            for event in events {
                let position = self
                    .items
                    .iter()
                    .position(|id| event["item_id"] == id.to_string())
                    .ok_or("event item")?;
                let grade = actor.role.item_grade(position, visibility);
                let grade = if archive != "none" && matches!(grade, "owner" | "write") {
                    "read"
                } else {
                    grade
                };
                assert_eq!(event["access"], grade);
            }
        }
        Ok(())
    }
}

#[tokio::test]
async fn aggregate_reads_cross_all_roles_visibility_archive_and_trash() -> Result<()> {
    let mut m = Matrix::new().await?;
    let actors: Vec<_> = m
        .actors
        .iter()
        .map(|a| Actor {
            role: a.role,
            user: a.user.clone(),
        })
        .collect();
    let mut cases = 0;
    for visibility in ["workspace", "private"] {
        for archive in ["none", "item", "module", "both"] {
            for trash in [
                "none",
                "task",
                "event",
                "document",
                "item",
                "module",
                "workspace",
            ] {
                m.state(visibility, archive, trash).await?;
                for mode in ["workspace", "private", "read", "write"] {
                    m.document_state(mode).await?;
                    for actor in &actors {
                        m.compare(actor, visibility, archive, trash, mode).await?;
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 2912);
    let anonymous = m
        .fixture
        .compare(None, Some(m.fixture.organizations[0]), Some("UTC"))
        .await?;
    assert_eq!(anonymous["code"], "P0002");
    m.fixture.finish().await
}
