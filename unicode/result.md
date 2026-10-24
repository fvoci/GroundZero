# Unicode boundary result

Baseline: 22ad80ee205332e3ef1ffd8f64f783789955921b. Revalidated after dependency main e0103ef5fbcb818a66d2156cc7c742d51ea321fc at PR120 head83585cfa756f50fbb087c8bd0678f1972560dfae. Merged as 06809206e9f7cdc88e538502d7f6fab3dff60d76.

Confirmed and fixed, without storage normalization or SQL changes:
- Export safeName could split an emoji at its existing 80 UTF-16-unit ceiling and cause encodeURIComponent to throw. Strip only a terminal high surrogate after the existing slice.
- Module key frontend Unicode uppercase could alias non-ASCII input to valid ASCII; trim also differed from Rust. Preserve ASCII identifier policy with ASCII-only uppercase and Unicode White_Space trim.
- Episode CSV used UTF-16 length for a server SQL 300-scalar contract; count code points instead.

No Hangul-only product allowlist or problematic Hangul range was found. Multilingual names/titles intentionally accept Hangul syllables/jamo, Han, Latin and emoji subject to their field contracts. ASCII identifier policies remain ASCII. No NFC/NFKC, grapheme policy or search equivalence was introduced.

Evidence: https://github.com/fvoci/GroundZero/pull/120 . All six jobs at https://github.com/fvoci/GroundZero/actions/runs/40094552932 PASS, including actual router/DB tests in general, OIDC and Supabase direct/session phases. Full web suite 798 tests in139 files; locally32 new+23 existing related tests PASS. Exact-head CodeRabbit summary6382977435 covers all5files, no actionable/inline/outside-diff findings. Generic docstring warning and bot sandbox ESLint/Clippy execution failures are distinct from actual repository check success.

Actual Chromium156 component/export checks PASS with mocked HTTP. Screenshots independently opened and retained in coordinator target/retained-browser/unicode-83585cf; GitHub attachment pending authenticated browser/user waiver. Real OS IME NOT RUN. No UI IME behavior was changed.

Policy questions remain: native maxlength UTF-16 vs server scalar units; canonical-equivalence/initial-consonant search semantics. SearchDialog composition interaction is unconfirmed and was not changed.
