use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};

pub(super) fn verify(css: &str, cmaps: &BTreeMap<String, BTreeSet<u32>>) -> Result<()> {
    let faces: Vec<_> = css.split("@font-face").skip(1).collect();
    ensure!(faces.len() == 4, "exactly four Sans CSS faces required");
    let mut seen = BTreeSet::new();
    for face in faces {
        let face = face.split('}').next().context("CSS face")?;
        let (name, points) = cmaps
            .iter()
            .find(|(name, _)| face.contains(name.as_str()))
            .context("unexpected CSS font source")?;
        ensure!(seen.insert(name), "duplicate CSS font face");
        let weight = if name.contains("Regular") {
            "400"
        } else {
            "700"
        };
        let declared = face
            .split("font-weight:")
            .nth(1)
            .and_then(|s| s.split(';').next())
            .context("CSS weight")?;
        ensure!(declared.trim() == weight, "CSS font weight mismatch");
        let ranges = face
            .split("unicode-range:")
            .nth(1)
            .and_then(|s| s.split(';').next())
            .context("CSS unicode-range")?;
        let mut actual = BTreeSet::new();
        for range in ranges.split(',') {
            let range = range
                .trim()
                .strip_prefix("U+")
                .context("CSS Unicode prefix")?;
            let endpoints: Vec<_> = range.split('-').collect();
            ensure!((1..=2).contains(&endpoints.len()), "CSS Unicode span");
            let first = u32::from_str_radix(endpoints[0], 16)?;
            let last = u32::from_str_radix(endpoints[endpoints.len() - 1], 16)?;
            ensure!(first <= last && last <= 0x10FFFF, "CSS Unicode bounds");
            actual.extend(first..=last);
        }
        ensure!(&actual == points, "CSS/cmap mismatch: {name}");
    }
    Ok(())
}
