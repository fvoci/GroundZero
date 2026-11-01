use super::*;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
}

#[test]
fn approved_assets_pass_structural_verification() -> Result<()> {
    run(root())
}

#[test]
fn serif_rejects_every_unapproved_head_bit_and_other_table_changes() -> Result<()> {
    let sources = crate::notices::font_sources(root())?;
    let before = woff2::raw_tables(&sources["NotoSerifKR-Regular.otf"])?;
    let after = woff2::raw_tables(&asset(root(), "NotoSerifKR-Regular.woff2")?)?;
    verify_serif_tables(&before, &after)?;
    for offset in 0..after[b"head"].len() {
        for bit in 0..8 {
            if (8..12).contains(&offset) || (offset == 16 && bit == 3) {
                continue;
            }
            let mut changed = after.clone();
            changed.get_mut(b"head").context("head")?[offset] ^= 1 << bit;
            assert!(
                verify_serif_tables(&before, &changed).is_err(),
                "head byte {offset} bit {bit}"
            );
        }
    }
    let mut changed = after;
    changed.get_mut(b"name").context("name")?[0] ^= 1;
    assert!(verify_serif_tables(&before, &changed).is_err());
    changed.remove(b"name");
    assert!(verify_serif_tables(&before, &changed).is_err());
    Ok(())
}

#[test]
fn sans_rejects_cmap_gaps_overlap_glyph_changes_uvs_and_name_changes() -> Result<()> {
    let sources = crate::notices::font_sources(root())?;
    let source = coverage::read(&sources["NotoSansKR-Regular.otf"])?;
    let common = coverage::read(&asset(root(), "NotoSansKR-Regular-common.woff2")?)?;
    let rest = coverage::read(&asset(root(), "NotoSansKR-Regular-rest.woff2")?)?;
    coverage::verify(&source, &common, &rest)?;
    let point = *rest.points.keys().next().context("rest point")?;
    let mut changed = rest.clone();
    changed.points.remove(&point);
    assert!(coverage::verify(&source, &common, &changed).is_err());
    let mut changed = common.clone();
    changed.points.insert(point, rest.points[&point]);
    assert!(coverage::verify(&source, &changed, &rest).is_err());
    let mut changed = rest.clone();
    *changed.points.get_mut(&point).context("glyph")? ^= 1;
    assert!(coverage::verify(&source, &common, &changed).is_err());
    let mut changed = rest.clone();
    let variation = *changed.variations.keys().next().context("UVS")?;
    changed.variations.remove(&variation);
    assert!(coverage::verify(&source, &common, &changed).is_err());
    for id in [0, 13, 14] {
        let mut changed = common.clone();
        changed
            .names
            .iter_mut()
            .find(|n| n.0 == id)
            .context("name ID")?
            .4
            .push(0);
        assert!(
            coverage::verify(&source, &changed, &rest).is_err(),
            "name ID {id}"
        );
    }
    assert_eq!(coverage::ks_hangul()?.len(), 2350);
    let mut cmaps = BTreeMap::new();
    for weight in ["Regular", "Bold"] {
        cmaps.insert(
            format!("NotoSansKR-{weight}-common.woff2"),
            common.points.keys().copied().collect(),
        );
        cmaps.insert(
            format!("NotoSansKR-{weight}-rest.woff2"),
            rest.points.keys().copied().collect(),
        );
    }
    let css = std::fs::read_to_string(root().join("apps/web/src/styles/font-ranges.css"))?;
    css::verify(&css, &cmaps)?;
    assert!(
        css::verify(
            &css.replacen("font-weight: 400", "font-weight: 700", 1),
            &cmaps
        )
        .is_err()
    );
    assert!(css::verify(&css.replacen("U+20-7E", "U+21-7E", 1), &cmaps).is_err());
    Ok(())
}

#[test]
fn woff2_rejects_truncation_transforms_metadata_and_checksum_changes() -> Result<()> {
    let data = std::fs::read(root().join(FONT_DIR).join("NotoSerifKR-Regular.woff2"))?;
    for length in [0, 8, 47, data.len() - 1] {
        assert!(woff2::decode(&data[..length]).is_err(), "length {length}");
    }
    for offset in [28, 48] {
        let mut changed = data.clone();
        changed[offset] |= if offset == 48 { 0x40 } else { 1 };
        assert!(woff2::decode(&changed).is_err(), "offset {offset}");
    }
    let mut sfnt = asset(root(), "NotoSerifKR-Regular.woff2")?;
    woff2::verify_checksum(&sfnt)?;
    let head = ttf_parser::RawFace::parse(&sfnt, 0)?
        .table_records
        .into_iter()
        .find(|record| record.tag == ttf_parser::Tag::from_bytes(b"head"))
        .context("head record")?
        .offset as usize;
    sfnt[head + 8] ^= 1;
    assert!(woff2::verify_checksum(&sfnt).is_err());
    Ok(())
}
