//! Observe global consumers without altering platform triggers or publications.

use super::fingerprint;
use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

pub(super) const SQL:&str="WITH RECURSIVE handlers(oid) AS (
 SELECT evtfoid FROM pg_event_trigger
 UNION SELECT d.refobjid FROM pg_depend d JOIN handlers h ON d.objid=h.oid
 WHERE d.classid='pg_proc'::regclass AND d.refclassid='pg_proc'::regclass
) SELECT jsonb_build_object(
 'events',(SELECT coalesce(jsonb_agg(jsonb_build_object('definition',to_jsonb(e),
  'handler',to_jsonb(p),'schema',n.nspname,'result',pg_get_function_result(p.oid),
  'trusted_owner',e.evtowner=p.proowner AND r.rolname IN ('supabase_admin','postgres') AND (r.rolsuper OR r.rolcreaterole),
  'extension',(SELECT extname FROM pg_depend d JOIN pg_extension x ON x.oid=d.refobjid
   WHERE d.classid='pg_proc'::regclass AND d.objid=p.oid AND d.refclassid='pg_extension'::regclass AND d.deptype='e')) ORDER BY e.oid),'[]')
  FROM pg_event_trigger e JOIN pg_proc p ON p.oid=e.evtfoid JOIN pg_namespace n ON n.oid=p.pronamespace JOIN pg_roles r ON r.oid=e.evtowner),
 'functions',(SELECT coalesce(jsonb_agg(to_jsonb(p) ORDER BY p.oid),'[]') FROM pg_proc p JOIN handlers h ON h.oid=p.oid),
 'namespaces',(SELECT coalesce(jsonb_agg(to_jsonb(n) ORDER BY n.oid),'[]') FROM pg_namespace n WHERE oid IN
  (SELECT pronamespace FROM pg_proc JOIN handlers USING(oid) UNION SELECT pnnspid FROM pg_publication_namespace
   UNION SELECT c.relnamespace FROM pg_publication_rel pr JOIN pg_class c ON c.oid=pr.prrelid)),
 'roles',(SELECT coalesce(jsonb_agg(to_jsonb(r)-'rolpassword' ORDER BY oid),'[]') FROM pg_roles r WHERE oid IN
  (SELECT evtowner FROM pg_event_trigger UNION SELECT proowner FROM pg_proc JOIN handlers USING(oid) UNION SELECT pubowner FROM pg_publication)),
 'dependencies',(SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY to_jsonb(d)::text),'[]') FROM pg_depend d WHERE
  (d.classid='pg_proc'::regclass AND d.objid IN (SELECT oid FROM handlers)) OR d.classid='pg_event_trigger'::regclass),
 'extensions',(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.oid),'[]') FROM pg_extension x WHERE EXISTS
  (SELECT FROM pg_depend d JOIN handlers h ON h.oid=d.objid WHERE d.classid='pg_proc'::regclass AND d.refclassid='pg_extension'::regclass AND d.refobjid=x.oid AND d.deptype='e')),
 'comments',(SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY to_jsonb(d)::text),'[]') FROM pg_description d WHERE
  (d.classoid='pg_proc'::regclass AND d.objoid IN (SELECT oid FROM handlers)) OR d.classoid IN ('pg_event_trigger'::regclass,'pg_publication'::regclass)),
 'publications',(SELECT coalesce(jsonb_agg(jsonb_build_object('definition',to_jsonb(p),'includes_product',p.puballtables
  OR EXISTS(SELECT FROM pg_publication_tables t WHERE t.pubname=p.pubname AND t.schemaname='fvoci')
  OR EXISTS(SELECT FROM pg_publication_namespace pn JOIN pg_namespace n ON n.oid=pn.pnnspid WHERE pn.pnpubid=p.oid AND n.nspname='fvoci')
  OR EXISTS(SELECT FROM pg_publication_rel pr JOIN pg_class c ON c.oid=pr.prrelid WHERE pr.prpubid=p.oid AND c.relnamespace=to_regnamespace('fvoci'))) ORDER BY p.oid),'[]') FROM pg_publication p),
 'publication_relations',(SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY r.oid),'[]') FROM pg_publication_rel r),
 'publication_namespaces',(SELECT coalesce(jsonb_agg(to_jsonb(n) ORDER BY n.oid),'[]') FROM pg_publication_namespace n),
 'publication_tables',(SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]') FROM pg_publication_tables t))";

fn managed(event: &Value) -> bool {
    let e = &event["definition"];
    if event["trusted_owner"] != true
        || event["handler"]["pronargs"] != 0
        || event["result"] != "event_trigger"
    {
        return false;
    }
    let Some(name) = e["evtname"].as_str() else {
        return false;
    };
    let (schema, handler, kind, tags, extension) = match name {
        "pgrst_ddl_watch" => (
            "extensions",
            "pgrst_ddl_watch",
            "ddl_command_end",
            None,
            None,
        ),
        "pgrst_drop_watch" => ("extensions", "pgrst_drop_watch", "sql_drop", None, None),
        "issue_pg_cron_access" => (
            "extensions",
            "grant_pg_cron_access",
            "ddl_command_end",
            Some("CREATE EXTENSION"),
            None,
        ),
        "issue_pg_graphql_access" => (
            "extensions",
            "grant_pg_graphql_access",
            "ddl_command_end",
            Some("CREATE EXTENSION"),
            None,
        ),
        "issue_pg_net_access" => (
            "extensions",
            "grant_pg_net_access",
            "ddl_command_end",
            Some("CREATE EXTENSION"),
            None,
        ),
        "issue_graphql_placeholder" => (
            "extensions",
            "set_graphql_placeholder",
            "sql_drop",
            Some("DROP EXTENSION"),
            None,
        ),
        "graphql_watch_ddl" => (
            "graphql",
            "increment_schema_version",
            "ddl_command_end",
            None,
            Some("pg_graphql"),
        ),
        "graphql_watch_drop" => (
            "graphql",
            "increment_schema_version",
            "sql_drop",
            None,
            Some("pg_graphql"),
        ),
        _ => return false,
    };
    event["schema"] == schema
        && event["handler"]["proname"] == handler
        && e["evtevent"] == kind
        && e["evttags"] == tags.map_or(Value::Null, |t| json!([t]))
        && extension.is_none_or(|x| event["extension"] == x)
}

pub(super) fn evaluate(raw: &Value) -> Result<(Value, Map<String, Value>)> {
    let mut blocked_events = 0;
    let mut events = Vec::new();
    for event in raw["events"].as_array().context("missing_global_events")? {
        let e = &event["definition"];
        let decision = if e["evtenabled"] == "D" {
            "disabled"
        } else if managed(event) {
            "managed"
        } else {
            blocked_events += 1;
            "blocked"
        };
        events.push(json!({"name":e["evtname"],"event":e["evtevent"],"enabled":e["evtenabled"],"tags":e["evttags"],
   "handler":format!("{}.{}",event["schema"].as_str().unwrap_or_default(),event["handler"]["proname"].as_str().unwrap_or_default()),
   "decision":decision,"definition_fingerprint":fingerprint(event)}));
    }
    let mut blocked_publications = 0;
    let mut publications = Vec::new();
    for p in raw["publications"]
        .as_array()
        .context("missing_publications")?
    {
        if p["includes_product"] == true {
            blocked_publications += 1;
        }
        publications.push(json!({"name":p["definition"]["pubname"],"all_tables":p["definition"]["puballtables"],"includes_product":p["includes_product"]}));
    }
    Ok((
        json!({"event_triggers":events,"publications":publications}),
        Map::from_iter([
            (
                "unrecognized_event_triggers".to_owned(),
                json!(blocked_events),
            ),
            (
                "product_publications".to_owned(),
                json!(blocked_publications),
            ),
        ]),
    ))
}
