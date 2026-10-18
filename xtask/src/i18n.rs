//! 화면 문구 번역 검사(D-81). `apps/web/project.inlang/settings.json`의 영역별 메시지 파일마다:
//! 모든 언어 파일이 있고 키가 같으며(누락·불일치 없음) 값이 비어 있지 않은지, 키가 영역 이름으로 시작하고 영역
//! 사이에 겹치지 않는지 본다. 컴파일 결과(`src/lib/paraglide/messages/_index.js`)에 모든 키가 있는지도 본다
//! (Paraglide는 형식 플러그인을 못 읽어도 경고만 내고 빈 결과로 성공하기 때문이다).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

const PROJECT: &str = "apps/web/project.inlang/settings.json";
const COMPILED_INDEX: &str = "apps/web/src/lib/paraglide/messages/_index.js";

fn keys(path: &Path) -> Result<BTreeMap<String, Value>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    let Value::Object(map) = value else {
        bail!("{} must be a JSON object", path.display());
    };
    Ok(map.into_iter().filter(|(k, _)| k != "$schema").collect())
}

fn is_empty(value: &Value) -> bool {
    match value {
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => true,
    }
}

/// 메시지 파일 검사. 문제를 모두 모아 한 번에 알린다.
pub fn check(root: &Path) -> Result<()> {
    let settings: Value = serde_json::from_str(
        &fs::read_to_string(root.join(PROJECT)).with_context(|| format!("read {PROJECT}"))?,
    )?;
    let locales: Vec<String> = settings["locales"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let patterns: Vec<String> = match &settings["plugin.inlang.messageFormat"]["pathPattern"] {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    };
    if locales.len() < 2 || patterns.is_empty() {
        bail!("{PROJECT}: expected at least two locales and one pathPattern");
    }
    let web = root.join("apps/web");
    let mut problems: Vec<String> = Vec::new();
    let mut owner: BTreeMap<String, String> = BTreeMap::new();
    for pattern in &patterns {
        // ./messages/{area}/{locale}.json
        let area = pattern
            .trim_start_matches("./messages/")
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        let mut per_locale: Vec<(String, BTreeMap<String, Value>)> = Vec::new();
        for locale in &locales {
            let path = web.join(pattern.replace("{locale}", locale));
            match keys(&path) {
                Ok(map) => per_locale.push((locale.clone(), map)),
                Err(err) => problems.push(format!("{err:#}")),
            }
        }
        let Some((base_locale, base)) = per_locale.first() else {
            continue;
        };
        let base_keys: BTreeSet<&String> = base.keys().collect();
        for (locale, map) in &per_locale[1..] {
            let these: BTreeSet<&String> = map.keys().collect();
            for missing in base_keys.difference(&these) {
                problems.push(format!(
                    "{area}: `{missing}` is in {base_locale} but not in {locale}"
                ));
            }
            for extra in these.difference(&base_keys) {
                problems.push(format!(
                    "{area}: `{extra}` is in {locale} but not in {base_locale}"
                ));
            }
        }
        for (locale, map) in &per_locale {
            for (key, value) in map {
                if is_empty(value) {
                    problems.push(format!("{area}/{locale}: `{key}` is empty"));
                }
            }
        }
        for key in base.keys() {
            if !key.starts_with(&format!("{area}_")) {
                problems.push(format!("{area}: `{key}` must start with `{area}_`"));
            }
            if let Some(other) = owner.insert(key.clone(), area.clone()) {
                problems.push(format!("`{key}` is defined in both {other} and {area}"));
            }
        }
    }
    // 컴파일 결과에 모든 키가 있어야 한다.
    let index = fs::read_to_string(root.join(COMPILED_INDEX)).with_context(|| {
        format!("read {COMPILED_INDEX}; run `pnpm --filter @fvoci/web run i18n`")
    })?;
    for key in owner.keys() {
        if !index.contains(&format!("./{key}.js")) {
            problems.push(format!("`{key}` is missing from the compiled messages"));
        }
    }
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("  {p}");
        }
        bail!("{} translation problem(s)", problems.len());
    }
    println!(
        "{} message keys in {} areas, {} languages",
        owner.len(),
        patterns.len(),
        locales.len()
    );
    Ok(())
}
