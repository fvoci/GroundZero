use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use ttf_parser::{Face, GlyphId};

use super::woff2::{bytes, u16_at, u32_at};

type Names = Vec<(u16, u16, u16, u16, Vec<u8>)>;
type Variations = BTreeMap<(u32, u32), Option<u16>>;

#[derive(Clone)]
pub(super) struct Coverage {
    pub points: BTreeMap<u32, u16>,
    pub variations: Variations,
    pub names: Names,
}

fn cid(face: &Face<'_>, glyph: GlyphId) -> Result<u16> {
    face.tables()
        .cff
        .context("CFF table")?
        .glyph_cid(glyph)
        .context("CFF glyph CID")
}

pub(super) fn read(data: &[u8]) -> Result<Coverage> {
    let face = Face::parse(data, 0).context("parse CFF font")?;
    let cmap = face.tables().cmap.context("cmap table")?;
    let mut points = BTreeSet::new();
    for subtable in cmap.subtables {
        if subtable.is_unicode() {
            subtable.codepoints(|point| {
                points.insert(point);
            });
        }
    }
    let points = points
        .into_iter()
        .map(|point| {
            let character = char::from_u32(point).context("invalid Unicode scalar in cmap")?;
            let glyph = face.glyph_index(character).context("missing cmap glyph")?;
            Ok((point, cid(&face, glyph)?))
        })
        .collect::<Result<_>>()?;
    let mut names: Names = face
        .names()
        .into_iter()
        .filter(|name| [0, 13, 14].contains(&name.name_id))
        .map(|name| {
            (
                name.name_id,
                name.platform_id as u16,
                name.encoding_id,
                name.language_id,
                name.name.to_vec(),
            )
        })
        .collect();
    names.sort();
    for id in [0, 13, 14] {
        ensure!(
            names.iter().any(|name| name.0 == id),
            "missing name ID {id}"
        );
    }
    let cmap_data = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"cmap"))
        .context("raw cmap")?;
    Ok(Coverage {
        points,
        names,
        variations: variations(&face, cmap_data)?,
    })
}

fn u24_at(data: &[u8], offset: usize) -> Result<u32> {
    let value = bytes(data, offset, 3)?;
    Ok(u32::from_be_bytes([0, value[0], value[1], value[2]]))
}

fn variations(face: &Face<'_>, cmap: &[u8]) -> Result<Variations> {
    let mut result = Variations::new();
    for index in 0..usize::from(u16_at(cmap, 2)?) {
        let offset = u32_at(cmap, 4 + index * 8 + 4)? as usize;
        if u16_at(cmap, offset)? != 14 {
            continue;
        }
        let table = bytes(cmap, offset, u32_at(cmap, offset + 2)? as usize)?;
        let count = u32_at(table, 6)? as usize;
        bytes(
            table,
            10,
            count.checked_mul(11).context("UVS count overflow")?,
        )?;
        for record in 0..count {
            let start = 10 + record * 11;
            let selector = u24_at(table, start)?;
            let defaults = u32_at(table, start + 3)? as usize;
            if defaults != 0 {
                let count = u32_at(table, defaults)? as usize;
                bytes(
                    table,
                    defaults + 4,
                    count.checked_mul(4).context("UVS count overflow")?,
                )?;
                for index in 0..count {
                    let start = defaults + 4 + index * 4;
                    let first = u24_at(table, start)?;
                    let last = first + u32::from(bytes(table, start + 3, 1)?[0]);
                    for point in first..=last {
                        ensure!(
                            result.insert((point, selector), None).is_none(),
                            "duplicate UVS"
                        );
                    }
                }
            }
            let nondefaults = u32_at(table, start + 7)? as usize;
            if nondefaults != 0 {
                let count = u32_at(table, nondefaults)? as usize;
                bytes(
                    table,
                    nondefaults + 4,
                    count.checked_mul(5).context("UVS count overflow")?,
                )?;
                for index in 0..count {
                    let start = nondefaults + 4 + index * 5;
                    let point = u24_at(table, start)?;
                    let glyph = GlyphId(u16_at(table, start + 3)?);
                    ensure!(
                        result
                            .insert((point, selector), Some(cid(face, glyph)?))
                            .is_none(),
                        "duplicate UVS"
                    );
                }
            }
        }
    }
    Ok(result)
}

pub(super) fn ks_hangul() -> Result<BTreeSet<u32>> {
    let mut points = BTreeSet::new();
    for row in 0xB0..=0xC8 {
        for cell in 0xA1..=0xFE {
            let pair = [row, cell];
            let decoded = encoding_rs::EUC_KR
                .decode_without_bom_handling_and_without_replacement(&pair)
                .context("KS X 1001 decoding")?;
            let mut chars = decoded.chars();
            let point = chars.next().context("KS X 1001 character")? as u32;
            ensure!(
                chars.next().is_none() && (0xAC00..=0xD7A3).contains(&point),
                "KS X 1001 Hangul"
            );
            points.insert(point);
        }
    }
    ensure!(points.len() == 2350, "KS X 1001 must have 2350 syllables");
    Ok(points)
}

pub(super) fn common_points(source: &Coverage) -> Result<BTreeSet<u32>> {
    let mut points = ks_hangul()?;
    ensure!(
        points.iter().all(|p| source.points.contains_key(p)),
        "source missing KS X 1001 Hangul"
    );
    for (first, last) in [
        (0x20, 0x024F),
        (0x0300, 0x036F),
        (0x2000, 0x206F),
        (0x20A0, 0x23FF),
        (0x2460, 0x27BF),
        (0x3000, 0x303F),
        (0x3130, 0x318F),
        (0xFF00, 0xFFEF),
    ] {
        points.extend((first..=last).filter(|p| source.points.contains_key(p)));
    }
    Ok(points)
}

pub(super) fn verify(source: &Coverage, common: &Coverage, rest: &Coverage) -> Result<()> {
    let expected = common_points(source)?;
    ensure!(
        common.points.keys().copied().collect::<BTreeSet<_>>() == expected,
        "common cmap differs from KS X 1001 + symbols"
    );
    ensure!(
        rest.points.keys().all(|p| !common.points.contains_key(p)),
        "overlapping cmap"
    );
    let mut union = common.points.clone();
    union.extend(rest.points.clone());
    ensure!(
        union == source.points,
        "cmap union or glyph CID differs from original"
    );
    for subset in [common, rest] {
        ensure!(subset.names == source.names, "name IDs 0/13/14 changed");
        ensure!(
            subset
                .variations
                .keys()
                .all(|(p, _)| subset.points.contains_key(p)),
            "UVS outside subset cmap"
        );
    }
    ensure!(
        rest.variations
            .keys()
            .all(|key| !common.variations.contains_key(key)),
        "overlapping UVS"
    );
    let mut variations = common.variations.clone();
    variations.extend(rest.variations.clone());
    ensure!(
        variations == source.variations,
        "UVS union or glyph CID differs from original"
    );
    Ok(())
}
