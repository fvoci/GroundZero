//! Decode only the untransformed CFF WOFF2 files permitted by D-86.
use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{Context, Result, ensure};

pub(super) type Tables = BTreeMap<[u8; 4], Vec<u8>>;
// WOFF2 table-tag indices, W3C Recommendation §4.1 (https://www.w3.org/TR/WOFF2/).
const TAGS: &[[u8; 4]] = &[
    *b"cmap", *b"head", *b"hhea", *b"hmtx", *b"maxp", *b"name", *b"OS/2", *b"post", *b"cvt ",
    *b"fpgm", *b"glyf", *b"loca", *b"prep", *b"CFF ", *b"VORG", *b"EBDT", *b"EBLC", *b"gasp",
    *b"hdmx", *b"kern", *b"LTSH", *b"PCLT", *b"VDMX", *b"vhea", *b"vmtx", *b"BASE", *b"GDEF",
    *b"GPOS", *b"GSUB", *b"EBSC", *b"JSTF", *b"MATH", *b"CBDT", *b"CBLC", *b"COLR", *b"CPAL",
    *b"SVG ", *b"sbix", *b"acnt", *b"avar", *b"bdat", *b"bloc", *b"bsln", *b"cvar", *b"fdsc",
    *b"feat", *b"fmtx", *b"fvar", *b"gvar", *b"hsty", *b"just", *b"lcar", *b"mort", *b"morx",
    *b"opbd", *b"prop", *b"trak", *b"Zapf", *b"Silf", *b"Glat", *b"Gloc", *b"Feat", *b"Sill",
];
// Well above the approved inputs; bound allocations even for corrupt artifacts.
const LIMIT: usize = 64 * 1024 * 1024;
const CHECKSUM_MAGIC: u32 = 0xB1B0_AFBA;

pub(super) fn bytes(data: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    data.get(offset..offset.checked_add(length).context("offset overflow")?)
        .context("truncated font")
}

pub(super) fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(bytes(data, offset, 2)?.try_into()?))
}

pub(super) fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(bytes(data, offset, 4)?.try_into()?))
}

fn base128(data: &[u8], offset: &mut usize) -> Result<usize> {
    let mut value = 0_u32;
    for index in 0..5 {
        let byte = bytes(data, *offset, 1)?[0];
        *offset += 1;
        ensure!(index != 0 || byte != 0x80, "noncanonical UIntBase128");
        ensure!(value & 0xFE00_0000 == 0, "UIntBase128 overflow");
        value = (value << 7) | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok(value as usize);
        }
    }
    anyhow::bail!("overlong UIntBase128")
}

pub(super) fn decode(data: &[u8]) -> Result<Vec<u8>> {
    ensure!(bytes(data, 0, 8)? == b"wOF2OTTO", "expected CFF WOFF2");
    ensure!(u32_at(data, 8)? as usize == data.len(), "WOFF2 length");
    ensure!(u16_at(data, 14)? == 0, "WOFF2 reserved field");
    ensure!(
        bytes(data, 28, 20)?.iter().all(|b| *b == 0),
        "WOFF2 metadata/private data"
    );
    let count = usize::from(u16_at(data, 12)?);
    ensure!((1..=63).contains(&count), "WOFF2 table count");
    let mut offset = 48;
    let mut directory = Vec::new();
    let mut total = 0;
    for _ in 0..count {
        let flags = bytes(data, offset, 1)?[0];
        offset += 1;
        ensure!(flags >> 6 == 0, "unsupported WOFF2 table transform");
        let tag = if flags & 63 == 63 {
            let tag = bytes(data, offset, 4)?.try_into()?;
            offset += 4;
            tag
        } else {
            TAGS[usize::from(flags & 63)]
        };
        ensure!(tag != *b"glyf" && tag != *b"loca", "expected CFF tables");
        let length = base128(data, &mut offset)?;
        total += length;
        ensure!(total <= LIMIT, "WOFF2 decoded size limit");
        directory.push((tag, length));
    }
    let compressed = bytes(data, offset, u32_at(data, 20)? as usize)?;
    let tail = &data[offset + compressed.len()..];
    ensure!(
        tail.len() <= 3 && tail.iter().all(|b| *b == 0),
        "WOFF2 trailing data"
    );
    let mut decoded = Vec::new();
    brotli_decompressor::Decompressor::new(compressed, 4096)
        .take((total + 1) as u64)
        .read_to_end(&mut decoded)
        .context("decode WOFF2 Brotli stream")?;
    ensure!(decoded.len() == total, "WOFF2 decoded length");
    let mut tables = Tables::new();
    let mut cursor = 0;
    for (tag, length) in directory {
        ensure!(
            tables
                .insert(tag, bytes(&decoded, cursor, length)?.to_vec())
                .is_none(),
            "duplicate WOFF2 table"
        );
        cursor += length;
    }
    ensure!(tables.contains_key(b"CFF "), "missing CFF table");
    ensure!(!tables.contains_key(b"DSIG"), "DSIG must be removed");
    ensure!(
        u16_at(tables.get(b"head").context("head table")?, 16)? & 0x800 != 0,
        "head bit 11 must be set"
    );
    let sfnt = encode(&tables)?;
    ensure!(
        sfnt.len() == u32_at(data, 16)? as usize,
        "WOFF2 totalSfntSize"
    );
    verify_checksum(&sfnt)?;
    Ok(sfnt)
}

pub(super) fn verify_checksum(sfnt: &[u8]) -> Result<()> {
    ensure!(
        checksum(sfnt) == CHECKSUM_MAGIC,
        "reconstructed SFNT checksum"
    );
    Ok(())
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn encode(tables: &Tables) -> Result<Vec<u8>> {
    let count = u16::try_from(tables.len())?;
    let selector = u16::try_from(count.ilog2())?;
    let search = (1 << selector) * 16;
    let mut output = b"OTTO".to_vec();
    for word in [count, search, selector, count * 16 - search] {
        output.extend_from_slice(&word.to_be_bytes());
    }
    let mut offset = 12 + 16 * tables.len();
    for (tag, table) in tables {
        let mut checked = table.clone();
        if tag == b"head" {
            checked.get_mut(8..12).context("short head")?.fill(0);
        }
        output.extend_from_slice(tag);
        for word in [
            checksum(&checked),
            u32::try_from(offset)?,
            u32::try_from(table.len())?,
        ] {
            output.extend_from_slice(&word.to_be_bytes());
        }
        offset += (table.len() + 3) & !3;
    }
    for table in tables.values() {
        output.extend_from_slice(table);
        output.resize((output.len() + 3) & !3, 0);
    }
    Ok(output)
}

pub(super) fn raw_tables(data: &[u8]) -> Result<Tables> {
    super::parser::tables(data)
}
