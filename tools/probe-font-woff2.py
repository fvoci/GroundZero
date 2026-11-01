"""D-86 strict round-trip witness. Candidates never replace approved product assets.

FontTools' null transforms avoid glyf/loca/hmtx changes. WOFF2 still requires
DSIG removal and head bit 11 (W3C WOFF2 section 5), so compare every raw table,
including head and DSIG, without exemptions or normalizing the expected data.
"""

import hashlib
import importlib.metadata
import io
import json
from pathlib import Path
import struct
import subprocess

from fontTools.ttLib import TTFont, woff2


BASE = "d7b8f67b1d6dad63f10ce0f94803dc0e9f5e984c"
FONT_DIR = "apps/web/src/assets/fonts"
# O-37 approved source hashes; do not derive expectations from the input.
ORIGINALS = {
    "NotoSansKR-Regular.otf": "69975a0ac8472717870aefeab0a4d52739308d90856b9955313b2ad5e0148d68",
    "NotoSansKR-Bold.otf": "5a6ceb287ed2fc6cfc6213144ebea68cbd94b20fc9eb873d8486493bf02d9bda",
    "NotoSerifKR-Regular.otf": "5ea012e15cb7eacc1f680aee1703f3b164791b1443ea3e52b65080cca5d179cf",
    "NotoSerifKR-Bold.otf": "263408609ef416d597718ea91055379867d3e7086fa32f049996995d4e715e55",
    "D2Coding-Regular.ttf": "d5f2b6bf5b1826cfbf3734c70582f0cd42ea2e1d6c2636ae681b612bc9c5682f",
    "D2Coding-Bold.ttf": "14a0cfa4d96fbd976bc4b1a6ef81e51df353a1689d5273ebcd908c857a4f2752",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def compare(original, restored):
    """Compare raw table bytes: no checksum, flag or signature exceptions."""
    before = original.reader
    after = restored.reader
    return {
        "missing_tables": sorted(set(before.keys()) - set(after.keys())),
        "added_tables": sorted(set(after.keys()) - set(before.keys())),
        "changed_tables": {
            tag: {
                "original_sha256": digest(before[tag]),
                "restored_sha256": digest(after[tag]),
                "original_bytes": len(before[tag]),
                "restored_bytes": len(after[tag]),
                "different_offsets": [
                    i for i, (a, b) in enumerate(zip(before[tag], after[tag])) if a != b
                ],
            }
            for tag in sorted(set(before.keys()) & set(after.keys()))
            if before[tag] != after[tag]
        },
    }


def main():
    for package, version in (("fonttools", "4.60.1"), ("Brotli", "1.2.0")):
        assert importlib.metadata.version(package) == version, package
    output = Path("target/font-evidence/woff2-probe")
    output.mkdir(parents=True, exist_ok=True)
    results = []
    for name, approved_hash in ORIGINALS.items():
        data = subprocess.check_output(["git", "show", f"{BASE}:{FONT_DIR}/{name}"])
        assert digest(data) == approved_hash, f"O-37 input changed: {name}"
        candidate = output / (Path(name).stem + ".woff2")
        woff2.compress(io.BytesIO(data), candidate, transform_tables=set())
        original = TTFont(io.BytesIO(data), lazy=True)
        restored = TTFont(candidate, lazy=True)
        comparison = compare(original, restored)
        flags = lambda font: struct.unpack_from(">H", font.reader["head"], 16)[0]
        # WOFF2 header: metaOffset, metaLength, metaOrigLength, privOffset, privLength.
        header = struct.unpack_from(">IIIII", candidate.read_bytes(), 28)
        assert header == (0, 0, 0, 0, 0), "WOFF metadata/private block must be absent"
        result = {
            "original": name,
            "original_sha256": approved_hash,
            "original_bytes": len(data),
            "candidate": candidate.name,
            "candidate_sha256": digest(candidate.read_bytes()),
            "candidate_bytes": candidate.stat().st_size,
            "head_flags_before": hex(flags(original)),
            "head_flags_after": hex(flags(restored)),
            "metadata_and_private_blocks": list(header),
            **comparison,
        }
        result["result"] = "FAIL" if any(comparison.values()) else "PASS"
        results.append(result)
        print(f'{result["result"]} {name}: '
              f'missing={result["missing_tables"]}, '
              f'changed={list(result["changed_tables"])}', flush=True)
        original.close()
        restored.close()
    report = {
        "base": BASE,
        "tools": {name: importlib.metadata.version(name) for name in ("fonttools", "Brotli")},
        "transform_tables": [],
        "results": results,
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    raise SystemExit(1 if any(r["result"] == "FAIL" for r in results) else 0)


if __name__ == "__main__":
    main()
