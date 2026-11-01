//! Mandatory D-86 verification, independent of the derivative hash manifest.
mod coverage;
mod css;
mod parser;
#[cfg(test)]
mod tests;
mod woff2;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, ensure};

const FONT_DIR: &str = "apps/web/src/assets/fonts";

pub fn run(root: &Path) -> Result<()> {
    let sources = crate::notices::font_sources(root)?;
    let mut cmaps = BTreeMap::new();
    for weight in ["Regular", "Bold"] {
        let serif = format!("NotoSerifKR-{weight}");
        let decoded = asset(root, &format!("{serif}.woff2"))?;
        verify_serif(&sources[format!("{serif}.otf").as_str()], &decoded)
            .with_context(|| serif.clone())?;
        let sans = format!("NotoSansKR-{weight}");
        let source = coverage::read(&sources[format!("{sans}.otf").as_str()])?;
        let common = coverage::read(&asset(root, &format!("{sans}-common.woff2"))?)?;
        let rest = coverage::read(&asset(root, &format!("{sans}-rest.woff2"))?)?;
        coverage::verify(&source, &common, &rest).with_context(|| sans.clone())?;
        cmaps.insert(
            format!("{sans}-common.woff2"),
            common.points.keys().copied().collect(),
        );
        cmaps.insert(
            format!("{sans}-rest.woff2"),
            rest.points.keys().copied().collect(),
        );
        let coding = format!("D2Coding-{weight}.ttf");
        ensure!(
            std::fs::read(root.join(FONT_DIR).join(&coding))? == sources[coding.as_str()],
            "D2Coding original changed: {coding}"
        );
    }
    css::verify(
        &std::fs::read_to_string(root.join("apps/web/src/styles/font-ranges.css"))?,
        &cmaps,
    )?;
    println!("D-86 font structure, cmap/UVS, names and original bytes verified");
    Ok(())
}

fn asset(root: &Path, name: &str) -> Result<Vec<u8>> {
    let data = std::fs::read(root.join(FONT_DIR).join(name))
        .with_context(|| format!("read {FONT_DIR}/{name}"))?;
    woff2::decode(&data).with_context(|| name.to_owned())
}

fn verify_serif(before: &[u8], after: &[u8]) -> Result<()> {
    verify_serif_tables(&woff2::raw_tables(before)?, &woff2::raw_tables(after)?)
}

fn verify_serif_tables(before: &woff2::Tables, after: &woff2::Tables) -> Result<()> {
    let mut expected: BTreeMap<_, _> = before
        .iter()
        .filter(|(tag, _)| *tag != b"DSIG")
        .map(|(tag, data)| (*tag, data.clone()))
        .collect();
    let original_head = expected.get_mut(b"head").context("original head")?;
    let new_head = after.get(b"head").context("derived head")?;
    original_head
        .get_mut(8..12)
        .context("short original head")?
        .copy_from_slice(woff2::bytes(new_head, 8, 4)?);
    *original_head.get_mut(16).context("short original head")? |= 8;
    ensure!(
        &expected == after,
        "Serif table bytes changed outside DSIG/head approved boundary"
    );
    Ok(())
}
