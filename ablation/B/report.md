# Ablation B: document serialized-size buffer

Final first-wave classification: **REJECT**, under the predeclared CPU
regression criterion, confirmed by the coordinator after reviewing this report,
the actual source and all three summaries. The coordinator requested no further
refinement and no live DB tests for this rejected candidate. DB, OIDC and
Supabase execution remain **NOT RUN**. No product commit or PR has been made;
the experiment branch, working-tree candidate, new regression file and raw
evidence are preserved without rewriting history or deleting results.

## Pinned experiment

- Base and current HEAD at setup: `22ad80ee205332e3ef1ffd8f64f783789955921b`.
- Branch: `fvoci/ablation-body-size`; initially clean.
- Rust: `rustc 1.99.0 (b940084d7 2026-09-28)`; Cargo 1.99.0.
- Cargo.lock SHA256: `afacc07bfd285d2079e5ef770e62e2d3aafd011f8aeb31dff0de63a9314a0d49`.
- Toolchain file SHA256: `72f7dd8c9fdf9c1fcd8eb8bf9326d4bc66833e4ebf4bc2932636e2b85efcb7af`.
- Original documents.rs SHA256: `eaf06666159cb69024f45f9ef0164fec85fbf973bc1368d4bb099b9258134ff9`.
- Hypothesis, invariants, criteria fixed before changes: `plan.md`.
- No updated dependency sources, commits, build outputs, services or volumes used.

## Change

Only `documents::validate_body` replaces `serde_json::to_vec(body).len()` with
`serde_json::to_writer(&mut ByteCount, body)` and uses the resulting byte count.
The local writer discards bytes, counts every write, reports the entire slice as
written, and has a no-op flush. Checked arithmetic refuses impossible-to-count
overflow through the existing serialization-error mapping rather than wrapping.
There is no early exit on exceeding the configured byte limit.

The pinned serde_json 1.0.154 implementation of `to_vec` delegates to `to_writer`
after allocating a Vec with capacity 128 (`src/ser.rs:2213`). The serializer and
compact JSON representation therefore remain the same. The removed allocation
is just this whole-body byte buffer; Value, reference traversal stacks, error
strings, and caller allocations still exist.

## Behavior checks so far

| Check | Result | Raw log |
| --- | --- | --- |
| New tests, original product code | PASS, 3/3 | baseline-test.log |
| Same tests, candidate | PASS, 3/3 | variant-test.log |
| Full server library unit tests, candidate | PASS, 83/83 | variant-unit.log |
| Scoped server library/new test Clippy, `-D warnings` | PASS | clippy.log |
| `cargo fmt --all --check` | PASS | fmt.log |
| Full `cargo xtask check` | NOT RUN | bounded resource authorization |
| DB integration, OIDC, Supabase | NOT RUN | no service authorization yet |

Baseline tests executed before editing product code. Commands used the private
`CARGO_TARGET_DIR=target/ablation-body-size/build`, `--locked`, and `-j2`:

```
cargo test --locked -j2 -p fvoci-server --test ablation_body_size
cargo test --locked -j2 -p fvoci-server --lib
cargo clippy --locked -j2 -p fvoci-server --lib --test ablation_body_size -- -D warnings
cargo fmt --all --check
```

The regression file verifies exact error messages and inclusive byte boundaries
for Unicode, escaping/control characters, integer extremes, finite fractional
numbers, bool/null/arrays/objects, large strings and 128 nested arrays. It checks
that input Values remain equal to their copies, invalid root type precedes size,
size precedes invalid image/link/reference nodes, reverse array traversal error
precedence remains intact, and the reference count is inclusive and checked
before validating an excess malformed reference.

## Caller and persistence boundary

- `documents::save`: ExpectedUser extraction still establishes identity first;
  title/body validation occurs before opening `UserTx`. Invalid bodies never
  reach locks, access checks or writes. On success, existing access checks,
  request-id replay, If-Match, base revision, unchanged-content detection and
  realtime restrictions still run before `write_revision`. That function still
  updates the body/revision, appends revision history, calls rename audit and
  records references in the same transaction.
- `revisions::restore_in_db`: historical body is fetched in the user's RLS
  transaction; title/body validation still precedes
  `app_restore_document_revision`. Validation failure drops the transaction.
  Restore identity/access/version/idempotence and checkpoint behavior remain
  caller/DB responsibilities.
- `Room::snapshot_with`: invalid body still sets `failed_checks`, emits
  `collab.invalid_document`, and returns false before writer selection or
  `write_snapshot`. Accepted bodies retain the existing author/access/lineage
  checks and snapshot/revision/persisted-ack logic.
- `check_update`: scratch CRDT decoding, lineage/state size and shape checks
  still precede body validation. The same BadRequest message becomes
  `Verdict::Invalid`; body acceptance still gates applying/broadcasting changes.

No input, RLS, tenant, authority, version, idempotence, atomicity, history,
reference retention, audit, notification or draft/save-preservation policy was
changed. Equal pure-helper decisions preserve these branch choices by source
inspection; helper tests are **not** evidence that live DB/event/audit execution
passed. Existing focused integration coverage to request includes templates
body rejection, revision restore/history/audit and request-id rules, conditional
save atomicity, references/mentions, expected-user protection, and live snapshot/
restore preservation. Any local integration must use `--keep` and preserve all
synthetic resources under the newest coordinator instruction.

## Performance protocol

`bench.rs` imports verbatim baseline and candidate validation/helper source with
minimal Limits/BadRequest types. It measures the pure validation boundary, not
HTTP, DB, end-to-end save latency, or aggregate application RSS. Both variants
use the same release-built pinned serde_json and uuid libraries. Build recipe:
`bash target/ablation-body-size/build-bench.sh` (no new manifest/dependencies).

Eight deterministic inputs: minimal, short prose, escaped Unicode, 1000 small
nodes, exact default 2,000,000-byte body, 2,000,001-byte rejected body, depth 128,
and 100 references. Before timing, each fixture compares complete outcomes for
five byte limits and three reference limits. Each run uses 41 AB/BA alternating
pairs with identical preconstructed Values and 64 untimed warmups. Batch
wall-clock nanoseconds per operation are a CPU-latency proxy; medians, p95, min,
max, MAD and paired ratios distinguish effects from noise.

Separate fresh processes measure the first validator call after constructing
inputs. These include first-call/page-fault effects but are **not** controlled
OS or hardware cold-cache trials. Main-run first calls after parity checks are
reported separately and must not be labelled cold. `Vec::capacity` is the
removed peak payload-buffer proxy, not total allocation count or heap peak.

Prebuilt binary SHA256 before measurement:
`73ca074112cc4a3f06e0dfd8c3fb6585af2d68b3c5cbec56a9a218955e6c224a`.

## First candidate measurements

Explicit coordinator slot: 2026-10-24 10:05:14–10:05:35 UTC, 21 seconds, CPU 0
affinity. Three complete runs, each with 41 AB/BA pairs for every fixture;
192 separate process-first samples (12 per fixture/variant). No truncation.
Process snapshots once per second found no rustc/cargo/npm/pnpm/node/build
process. This is a quiet coordinated local run, not controlled dedicated
hardware. Load average changed from 2.15/3.02/3.81 to 2.62/3.06/3.81.

Raw outputs: `warm-{1,2,3}.csv`, `first-validator.csv`,
`measurement-environment.txt`, `measurement-processes.log`. Full distribution
statistics: `summary-{1,2,3}.json`, `summary-first.json`. Every matrix run also
passed 120 full-result/string comparisons across byte/reference boundaries.
The executable and source hashes recorded above apply to these measurements.

| Fixture (serialized bytes) | Removed Vec capacity | Paired median latency change, runs 1 / 2 / 3 |
| --- | ---: | ---: |
| Minimal (14) | 128 | -10.68% / -9.39% / -10.42% |
| Short prose (485) | 866 | -38.64% / -37.59% / -38.47% |
| Escaped Unicode (139,288) | 262,144 | **+20.48% / +17.98% / +20.30%** |
| 1000 nodes (68,026) | 131,072 | -3.12% / -2.67% / -2.26% |
| Exact default limit (2,000,000) | 3,999,970 | -52.48% / -52.17% / -52.65% |
| Rejected above limit (2,000,001) | 3,999,972 | -52.61% / -52.37% / -52.59% |
| 128 nested arrays (285) | 512 | **+23.64% / +22.64% / +23.90%** |
| 100 references (8,926) | 16,384 | -5.72% / -6.41% / -6.44% |

Negative means faster. At run 3, the 2MB accepted case measured medians
907,537 ns baseline / 429,476 ns candidate; escaped Unicode 70,770 / 85,227 ns;
deep arrays 918 / 1,134 ns. Unicode run-3 MAD was 0.46% / 0.30%, deep run-3 MAD
1.25% / 1.25%. These regressions persist across all three runs and are much
larger than the within-run variation in runs 2 and 3; they cannot be dismissed
as blanket flakiness. The first run had higher deep/reference dispersion and
the second run had higher Unicode dispersion; all raw samples remain present.

Process-first medians included: 2MB accepted 1,931,776 / 478,937 ns; escaped
Unicode 71,501 / 84,561 ns; deep 3,080 / 2,700 ns. The deep first-call direction
differs from its warmed direction. Do not conflate these distributions with
controlled cold caches or infer all first-call behavior from warmed results.

The buffer-elimination objective succeeds, but the straightforward writer does
not pass the agreed CPU acceptance gate. A possible `Write::write_all` override
was proposed as a follow-up hypothesis, not implemented or measured. The
coordinator declined refinement in this wave; the negative result is final for
this candidate. No unrelated optimization, live DB/service operation, PR,
merge, tag or deployment was performed.
