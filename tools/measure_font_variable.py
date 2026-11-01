"""Diagnostic only: compare a new upstream variable source; never ship it."""

import io
import json
from pathlib import Path
import urllib.request

from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont, woff2

from font_assets import sha256, source_bytes

VARIABLE_FONT_COMMIT = "523d033d6cb47f4a80c58a35753646f5c3608a78"  # Sans2.004
VARIABLE_FONT_PATH = "Sans/Variable/OTF/Subset/NotoSansKR-VF.otf"


def main():
    output = Path("target/font-variable")
    output.mkdir(parents=True, exist_ok=True)
    source = output / "NotoSansKR-VF.otf"
    url = f"https://raw.githubusercontent.com/notofonts/noto-cjk/{VARIABLE_FONT_COMMIT}/{VARIABLE_FONT_PATH}"
    if not source.exists():
        with urllib.request.urlopen(url, timeout=60) as response:
            source.write_bytes(response.read())
    variable = output / "NotoSansKR-VF.woff2"
    woff2.compress(source, variable, transform_tables=set())
    fonts = [TTFont(io.BytesIO(source_bytes(f"NotoSansKR-{w}.otf")))
             for w in ("Regular", "Bold")]
    glyph_sets = [font.getGlyphSet() for font in fonts]
    incompatible = []
    for name in fonts[0].getGlyphOrder():
        pens = [RecordingPen(), RecordingPen()]
        for glyphs, pen in zip(glyph_sets, pens):
            glyphs[name].draw(pen)
        signatures = [[(operation, len(arguments)) for operation, arguments in pen.value]
                      for pen in pens]
        if signatures[0] != signatures[1]:
            incompatible.append(name)
    with TTFont(source) as font:
        axes = [{"tag": axis.axisTag, "min": axis.minValue, "default": axis.defaultValue,
                 "max": axis.maxValue} for axis in font["fvar"].axes]
    report = {
        "adopted": False, "reason": "New original outside the six approved inputs",
        "source_commit": VARIABLE_FONT_COMMIT, "source_url": url,
        "source_sha256": sha256(source.read_bytes()), "source_bytes": source.stat().st_size,
        "woff2_sha256": sha256(variable.read_bytes()), "woff2_bytes": variable.stat().st_size,
        "axes": axes, "static_glyphs": len(fonts[0].getGlyphOrder()),
        "incompatible_outline_signatures": len(incompatible),
        "incompatible_examples": incompatible[:10],
    }
    (output / "sizes.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
