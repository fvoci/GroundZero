import unittest
from unittest.mock import patch

from pathlib import Path

from font_assets import ORIGINALS, assert_serif_tables, ks_hangul, source_bytes, verify_assets, verify_css


class FontAssets(unittest.TestCase):
    def test_ks_x_1001_has_exactly_2350_hangul(self):
        self.assertEqual(len(ks_hangul()), 2350)

    def test_published_assets_obey_d86(self):
        verify_assets()

    def test_css_routes_exactly_the_published_cmaps(self):
        css = Path("apps/web/src/styles/font-ranges.css").read_text()
        verify_css(css)
        with self.assertRaises(AssertionError):
            verify_css(css.replace("font-weight: 400", "font-weight: 700", 1))

    def test_cached_sources_still_require_the_approved_hash(self):
        name = "NotoSansKR-Regular.otf"
        source_bytes(name)
        with patch.dict(ORIGINALS, {name: "0" * 64}):
            with self.assertRaises(AssertionError):
                source_bytes(name)

    def test_serif_boundary_rejects_every_unapproved_head_bit(self):
        head = bytes(54)
        before = {"head": head, "DSIG": b"signature", "name": b"copyright"}
        allowed = bytearray(head)
        allowed[8:12] = b"abcd"
        allowed[16] = 0x08
        assert_serif_tables(before, {"head": bytes(allowed), "name": b"copyright"})
        for offset in range(54):
            for bit in range(8):
                if 8 <= offset < 12 or (offset == 16 and bit == 3):
                    continue
                damaged = bytearray(allowed)
                damaged[offset] ^= 1 << bit
                with self.assertRaises(AssertionError, msg=f"head byte {offset} bit {bit}"):
                    assert_serif_tables(before, {"head": bytes(damaged), "name": b"copyright"})
        with self.assertRaises(AssertionError):
            assert_serif_tables(before, {"head": bytes(allowed), "name": b"changed"})
        with self.assertRaises(AssertionError):
            assert_serif_tables(before, {"head": bytes(allowed)})


if __name__ == "__main__":
    unittest.main()
