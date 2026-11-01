//! Read the approved CFF fonts using the maintained Fontations parser.
use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use read_fonts::ps::cff::{charset::Charset, dict, v1::Index};
use read_fonts::tables::cmap::{CmapSubtable, PlatformId};
use read_fonts::{FontRead, FontRef, TableProvider, types::Tag};

use super::coverage::{self, Coverage};
use super::woff2::{self, Tables};

pub(super) fn read(data: &[u8]) -> Result<Coverage> {
    let font = FontRef::new(data).context("parse SFNT font")?;
    let cids = cid_map(&font)?;
    let cmap = font.table_data(Tag::new(b"cmap")).context("raw cmap")?;
    Ok(Coverage {
        points: points(&font, &cids)?,
        names: names(&font)?,
        variations: coverage::variations(&cids, cmap.as_bytes())?,
    })
}

// CID-keyed CFF charsets use the same binary format as SID charsets. The ROS
// operator establishes CID semantics; keep raw CID values without resolving names.
fn cid_map(font: &FontRef<'_>) -> Result<Vec<u16>> {
    let cff = font.cff().context("CFF table")?;
    let mut charset_offset = None;
    let mut charstrings_offset = None;
    let mut cid_keyed = false;
    for entry in dict::entries(cff.top_dicts().get(0)?, None) {
        match entry.context("CFF top DICT")? {
            dict::Entry::Charset(offset) => charset_offset = Some(offset),
            dict::Entry::CharstringsOffset(offset) => charstrings_offset = Some(offset),
            dict::Entry::Ros { .. } => cid_keyed = true,
            _ => {}
        }
    }
    ensure!(cid_keyed, "expected CID-keyed CFF font");
    let offset = charset_offset.context("CFF charset offset")?;
    ensure!(offset > 2, "CID font must use a custom charset");
    let charstrings = Index::read(
        cff.offset_data()
            .split_off(charstrings_offset.context("CFF charstrings offset")?)
            .context("CFF charstrings bounds")?,
    )?;
    let count = font.maxp()?.num_glyphs();
    ensure!(
        charstrings.count() == count,
        "CFF/maxp glyph count mismatch"
    );
    let charset = Charset::new(cff.offset_data(), offset, u32::from(count))?;
    let mut cids = Vec::with_capacity(usize::from(count));
    // Bulk iteration walks CFF charset ranges once, rather than scanning them per glyph.
    for (glyph, cid) in charset.iter() {
        ensure!(
            glyph.to_u32() as usize == cids.len(),
            "CFF charset glyph order"
        );
        cids.push(cid.to_u16());
    }
    ensure!(cids.len() == usize::from(count), "truncated CFF charset");
    Ok(cids)
}

fn points(font: &FontRef<'_>, cids: &[u16]) -> Result<BTreeMap<u32, u16>> {
    let cmap = font.cmap().context("cmap table")?;
    let records = cmap.encoding_records();
    ensure!(
        records.len() == usize::from(cmap.num_tables()),
        "truncated cmap records"
    );
    let mut points = BTreeMap::new();
    for record in records {
        let platform = record.platform_id();
        if platform != PlatformId::Unicode
            && !(platform == PlatformId::Windows && [1, 10].contains(&record.encoding_id()))
        {
            continue;
        }
        let subtable = record
            .subtable(cmap.offset_data())
            .context("Unicode cmap subtable")?;
        match &subtable {
            CmapSubtable::Format4(table) => validate_bmp(table)?,
            CmapSubtable::Format12(table) => validate_full(table)?,
            CmapSubtable::Format14(_) => continue,
            _ => anyhow::bail!("unsupported Unicode cmap format in approved font"),
        }
        for (point, glyph) in subtable.iter() {
            // Format 4's terminal 0xffff segment maps to .notdef, not a character.
            if point == 0xFFFF && glyph.to_u32() == 0 {
                continue;
            }
            ensure!(
                char::from_u32(point).is_some(),
                "invalid Unicode scalar in cmap"
            );
            ensure!(glyph.to_u32() != 0, "cmap maps a character to .notdef");
            let cid = *cids
                .get(glyph.to_u32() as usize)
                .context("cmap glyph CID")?;
            if let Some(previous) = points.insert(point, cid) {
                ensure!(previous == cid, "conflicting Unicode cmap glyphs");
            }
        }
    }
    ensure!(!points.is_empty(), "empty Unicode cmap");
    Ok(points)
}

fn validate_bmp(table: &read_fonts::tables::cmap::Cmap4<'_>) -> Result<()> {
    let count = usize::from(table.seg_count_x2() / 2);
    ensure!(
        table.seg_count_x2().is_multiple_of(2) && count > 0,
        "invalid cmap4 segment count"
    );
    ensure!(
        table.start_code().len() == count
            && table.end_code().len() == count
            && table.id_delta().len() == count
            && table.id_range_offsets().len() == count,
        "truncated cmap4 segments"
    );
    let mut previous = None;
    for (start, end) in table.start_code().iter().zip(table.end_code()) {
        let (start, end) = (start.get(), end.get());
        ensure!(
            start <= end && previous.is_none_or(|p| p < start),
            "invalid cmap4 segment bounds"
        );
        previous = Some(end);
    }
    Ok(())
}

fn validate_full(table: &read_fonts::tables::cmap::Cmap12<'_>) -> Result<()> {
    ensure!(
        table.groups().len() == table.num_groups() as usize,
        "truncated cmap12 groups"
    );
    let mut previous = None;
    for group in table.groups() {
        let (start, end) = (group.start_char_code(), group.end_char_code());
        ensure!(
            start <= end && end <= 0x10FFFF && previous.is_none_or(|p| p < start),
            "invalid cmap12 group bounds"
        );
        previous = Some(end);
    }
    Ok(())
}

fn names(font: &FontRef<'_>) -> Result<coverage::Names> {
    let table = font.name().context("name table")?;
    ensure!(
        table.name_record().len() == usize::from(table.count()),
        "truncated name records"
    );
    let storage = table.string_data();
    let mut names = Vec::new();
    for name in table.name_record() {
        let id = name.name_id().to_u16();
        if ![0, 13, 14].contains(&id) {
            continue;
        }
        let raw = woff2::bytes(
            storage.as_bytes(),
            name.string_offset().to_u32() as usize,
            usize::from(name.length()),
        )?;
        names.push((
            id,
            name.platform_id(),
            name.encoding_id(),
            name.language_id(),
            raw.to_vec(),
        ));
    }
    names.sort();
    for id in [0, 13, 14] {
        ensure!(names.iter().any(|n| n.0 == id), "missing name ID {id}");
    }
    Ok(names)
}

pub(super) fn tables(data: &[u8]) -> Result<Tables> {
    let font = FontRef::new(data).context("parse SFNT table directory")?;
    let directory = font.table_directory();
    ensure!(
        directory.table_records().len() == usize::from(directory.num_tables()),
        "truncated SFNT directory"
    );
    let mut tables = Tables::new();
    for record in directory.table_records() {
        let tag = record.tag().to_be_bytes();
        let raw = woff2::bytes(data, record.offset() as usize, record.length() as usize)?;
        ensure!(
            tables.insert(tag, raw.to_vec()).is_none(),
            "duplicate SFNT table"
        );
    }
    Ok(tables)
}

#[cfg(test)]
pub(super) fn table_offset(data: &[u8], tag: &[u8; 4]) -> Result<usize> {
    let font = FontRef::new(data)?;
    let record = font
        .table_directory()
        .table_records()
        .iter()
        .find(|record| record.tag() == Tag::new(tag))
        .context("SFNT table record")?;
    Ok(record.offset() as usize)
}
