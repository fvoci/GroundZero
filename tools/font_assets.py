"""D-86 font inputs and independent verification of the published assets."""

import hashlib
import io
from pathlib import Path
import re
import struct
import subprocess
import urllib.request

from fontTools.ttLib import TTFont

if not __debug__:
    raise RuntimeError("Font verification requires Python assertions (no -O/PYTHONOPTIMIZE)")

BASE = "d7b8f67b1d6dad63f10ce0f94803dc0e9f5e984c"
FONT_DIR = Path("apps/web/src/assets/fonts")
ORIGINALS = {
    "NotoSansKR-Regular.otf": "69975a0ac8472717870aefeab0a4d52739308d90856b9955313b2ad5e0148d68",
    "NotoSansKR-Bold.otf": "5a6ceb287ed2fc6cfc6213144ebea68cbd94b20fc9eb873d8486493bf02d9bda",
    "NotoSerifKR-Regular.otf": "5ea012e15cb7eacc1f680aee1703f3b164791b1443ea3e52b65080cca5d179cf",
    "NotoSerifKR-Bold.otf": "263408609ef416d597718ea91055379867d3e7086fa32f049996995d4e715e55",
    "D2Coding-Regular.ttf": "d5f2b6bf5b1826cfbf3734c70582f0cd42ea2e1d6c2636ae681b612bc9c5682f",
    "D2Coding-Bold.ttf": "14a0cfa4d96fbd976bc4b1a6ef81e51df353a1689d5273ebcd908c857a4f2752",
}
# Common punctuation/symbol blocks are explicit, stable generation inputs.
COMMON_BLOCKS = (
    (0x20, 0x024F), (0x0300, 0x036F), (0x2000, 0x206F), (0x20A0, 0x23FF),
    (0x2460, 0x27BF), (0x3000, 0x303F), (0x3130, 0x318F), (0xFF00, 0xFFEF),
)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def source_bytes(name):
    """Read the pinned Git input, also in shallow CI checkouts; always check O-37."""
    cache = Path("target/font-tools-sources") / name
    if cache.exists():
        data = cache.read_bytes()
    else:
        result = subprocess.run(
            ["git", "show", f"{BASE}:{FONT_DIR.as_posix()}/{name}"],
            capture_output=True, check=False,
        )
        if result.returncode == 0:
            data = result.stdout
        else:
            url = f"https://raw.githubusercontent.com/fvoci/GroundZero/{BASE}/{FONT_DIR.as_posix()}/{name}"
            with urllib.request.urlopen(url, timeout=60) as response:
                data = response.read()
        assert sha256(data) == ORIGINALS[name], f"O-37 source hash: {name}"
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_bytes(data)
    assert sha256(data) == ORIGINALS[name], f"O-37 source hash: {name}"
    return data


def ks_hangul():
    values = {ord(bytes((row, cell)).decode("euc_kr"))
              for row in range(0xB0, 0xC9) for cell in range(0xA1, 0xFF)}
    assert len(values) == 2350
    assert all(0xAC00 <= c <= 0xD7A3 for c in values)
    return values


def unicode_points(font):
    return {point for table in font["cmap"].tables if table.isUnicode()
            for point in table.cmap}


def common_points(font):
    common = ks_hangul()
    assert common <= unicode_points(font), "all KS X 1001 syllables must exist"
    for start, end in COMMON_BLOCKS:
        common.update(range(start, end + 1))
    return common & unicode_points(font)


def name_metadata(font):
    return sorted((record.nameID, record.platformID, record.platEncID, record.langID,
                   record.toBytes()) for record in font["name"].names
                  if record.nameID in (0, 13, 14))


def variation_sequences(font):
    return {(base, selector): glyph
            for table in font["cmap"].tables if table.format == 14
            for selector, entries in table.uvsDict.items() for base, glyph in entries}


def assert_serif_tables(before, after):
    """Only remove DSIG, set bit 11 and recalculate checkSumAdjustment."""
    assert set(after) == set(before) - {"DSIG"}, "unexpected table removal/addition"
    for tag in after:
        if tag != "head":
            assert before[tag] == after[tag], f"unexpected {tag} change"
            continue
        expected = bytearray(before[tag])
        expected[8:12] = after[tag][8:12]
        expected[16] |= 0x08
        assert bytes(expected) == after[tag], "head changed outside the approved bits"


def assert_woff2(font, data):
    assert data[:4] == b"wOF2"
    assert struct.unpack_from(">IIIII", data, 28) == (0, 0, 0, 0, 0), "WOFF metadata/private"
    assert "DSIG" not in font, "DSIG must be removed"
    assert struct.unpack_from(">H", font.reader["head"], 16)[0] & 0x800, "head bit 11"
    # Reconstruct SFNT in WOFF2 table order and verify the recalculated checksum.
    from fontTools.ttLib.sfnt import calcChecksum
    from fontTools.ttLib import getSearchRange
    tags = sorted(font.reader.keys())
    count = len(tags)
    search, selector, shift = getSearchRange(count, 16)
    header = struct.pack(">4sHHHH", font.sfntVersion.encode("latin1"), count, search, selector, shift)
    offset = 12 + 16 * count
    directory = bytearray(header)
    total = 0
    for tag in tags:
        table = bytearray(font.reader[tag])
        if tag == "head":
            table[8:12] = b"\0" * 4
        checksum = calcChecksum(bytes(table))
        directory.extend(struct.pack(">4sIII", tag.encode("ascii"), checksum, offset, len(table)))
        offset += (len(table) + 3) & ~3
        total += checksum
    adjustment = (0xB1B0AFBA - total - calcChecksum(bytes(directory))) & 0xFFFFFFFF
    assert struct.unpack_from(">I", font.reader["head"], 8)[0] == adjustment, "head checksum"


def verify_css(css, font_dir=FONT_DIR):
    faces = re.findall(r"@font-face\s*\{([^}]+)\}", css)
    assert len(faces) == 4, "exactly two Sans pieces per weight"
    seen = set()
    for face in faces:
        file_match = re.search(r"NotoSansKR-(Regular|Bold)-(common|rest)\.woff2", face)
        assert file_match, "unexpected font source"
        name = file_match[0]
        assert name not in seen, "duplicate font face"
        seen.add(name)
        weight = 400 if file_match[1] == "Regular" else 700
        assert re.search(rf"font-weight:\s*{weight}\s*;", face), "font weight mismatch"
        ranges = re.search(r"unicode-range:\s*([^;]+);", face)
        assert ranges, "missing unicode-range"
        points = set()
        for span in ranges[1].split(","):
            endpoints = span.strip().removeprefix("U+").split("-")
            start, end = int(endpoints[0], 16), int(endpoints[-1], 16)
            assert start <= end <= 0x10FFFF
            points.update(range(start, end + 1))
        with TTFont(font_dir / name) as font:
            assert points == unicode_points(font), f"CSS/cmap mismatch: {name}"


def verify_assets(font_dir=FONT_DIR):
    for weight in ("Regular", "Bold"):
        name = f"NotoSansKR-{weight}.otf"
        with TTFont(io.BytesIO(source_bytes(name))) as original:
            common = TTFont(font_dir / f"NotoSansKR-{weight}-common.woff2")
            rest = TTFont(font_dir / f"NotoSansKR-{weight}-rest.woff2")
            a, b = unicode_points(common), unicode_points(rest)
            assert a == common_points(original), "common cmap differs"
            assert b == unicode_points(original) - a, "rest cmap differs"
            assert not a & b, "cmap pieces overlap"
            assert a | b == unicode_points(original), "cmap gap"
            av, bv = variation_sequences(common), variation_sequences(rest)
            assert not av.keys() & bv.keys(), "variation sequence overlap"
            assert av | bv == variation_sequences(original), "variation sequence gap/change"
            for piece in (common, rest):
                assert name_metadata(piece) == name_metadata(original), "name ID 0/13/14 changed"
                assert all(glyph == original.getBestCmap()[point]
                           for point, glyph in piece.getBestCmap().items()), "cmap glyph changed"
                assert_woff2(piece, Path(piece.reader.file.name).read_bytes())
                piece.close()
        name = f"NotoSerifKR-{weight}.otf"
        with TTFont(io.BytesIO(source_bytes(name)), lazy=True) as original:
            output = font_dir / f"NotoSerifKR-{weight}.woff2"
            with TTFont(output, lazy=True) as converted:
                assert_serif_tables({t: original.reader[t] for t in original.reader.keys()},
                                    {t: converted.reader[t] for t in converted.reader.keys()})
                assert_woff2(converted, output.read_bytes())
        name = f"D2Coding-{weight}.ttf"
        assert (font_dir / name).read_bytes() == source_bytes(name), "D2Coding must stay original"


if __name__ == "__main__":
    verify_assets()
    print("PASS D-86: source hashes, cmap partitions, name metadata, Serif tables, WOFF2 checksums and D2 originals")
