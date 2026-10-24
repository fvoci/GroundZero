# C new-pin evidence checkpoint

Verdict: the predeclared local formatting-cost criterion is met under updated dependencies; root independently recomputed both CSVs and accepted the improvement subject to newest-head CI/review. No endpoint-latency, total-allocation-count or meaningful 1MiB speedup claim. Source PR remains Draft: https://github.com/fvoci/GroundZero/pull/121

## Exact contexts

- Actual compiled/tested/prebuilt head: `ba6ac88e34b1227108298c2442a825f1d056e3a4`, normally merging dependency main `e0103ef5fbcb818a66d2156cc7c742d51ea321fc` into C source commit `51b240b`.
- Current head during timing: `6d2d1a469a8b3f1e8172a5d653ecdc4d573d402b`, after normal merge of independent Unicode main `06809206`.
- Latest pushed source head at this checkpoint: `c1da0cb95bb55d8addb414ba95b8ea52fb1f63ac`, after normal merge of adopted A main `0e02a02802b09824765bc22f9fc43c1eb7359ca8`.
- Cache source, Cargo/toolchain/pnpm/package pins and harness source/binary/sha2 rlib hashes remained unchanged through both independent merges. No rebuild or test relabeling was performed; latest-head CI covers integration.

Rust 1.99.1; direct sha2 0.11.1 (0.10.9 also remains transitively); Node 24.22.1, existing Corepack 0.36.0 and pnpm 12.15.0. `updated-identity.txt`, `updated-pins.txt`, `updated-harness.sha256` and `new-pin-unchanged-provenance.txt` pin exact versions, source, lock and compiled artifacts. Binary fingerprints are provenance only; binaries/caches are excluded from archival selection.

Complete source snapshot `new-pin-cache.rs` and `new-pin-Cargo.lock` are copied without modification; server manifest/toolchain snapshots accompany them. This is the same minimal formatter/regression change: first 16 SHA-256 bytes, same user/domain inputs, exactly 32 lowercase hex digits with quotes, written into one output buffer. No permission, transaction, precondition, error, audit/event or DB logic changed. Pure returned-byte equivalence preserves caller comparisons/response fields; this benchmark did not measure DB effects.

## Reproduction and protocol

`updated-protocol.md` was recorded before these timings. `updated-bench.rs` is the original harness copied without changing functions, synthetic inputs, warmup or sample plan. Exact old/new whole-function source matching against updated main/candidate passed (`updated-source-match.txt`). Four fixed vectors and 3,072 deterministic binary/user pairs passed under the new compiled sha2 (`updated-equivalence.log`); the committed golden/binary regression also passed, explicitly checking zero-byte padding.

```sh
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/ablation-c-final/benchmark-build cargo build -p sha2@0.11.1 --release --locked -j2
c_sha2_libraries=(target/ablation-c-final/benchmark-build/release/deps/libsha2-*.rlib)
test "${#c_sha2_libraries[@]}" -eq 1
rustc --edition 2024 -C opt-level=3 -L dependency=target/ablation-c-final/benchmark-build/release/deps --extern "sha2=${c_sha2_libraries[0]}" target/ablation-c-final/updated-bench.rs -o target/ablation-c-final/updated-bench
target/ablation-c-final/updated-bench --verify-only
# Only in an explicitly granted quiet slot, two sequential invocations:
taskset -c 31 target/ablation-c-final/updated-bench > target/ablation-c-final/reproduction-1.csv
taskset -c 31 target/ablation-c-final/updated-bench > target/ablation-c-final/reproduction-2.csv
node target/ablation-c-final/updated-summarize.mjs target/ablation-c-final/reproduction-1.csv target/ablation-c-final/reproduction-2.csv
```

The Rust harness black-boxes inputs/outputs and includes String destruction. Three fixed synthetic 16-byte users rotate over empty, 256-byte and 1MiB cyclic bodies. Hex-only uses precomputed digests. Each variant warms for three batches, then 31 paired rounds alternate AB/BA. Batch sizes: hex/empty 20,000, small 10,000, large 100 operations. Separate first-call observations are not a statistically valid cold-start comparison; hashing/allocator state may already be warm. No outliers were discarded.

Original threshold unchanged: >=10% hex-only median reduction and >=90% paired improvements in both runs. This does not establish endpoint or large-body improvements.

## Accepted updated window

UTC: **2026-10-24 12:53:47–12:53:57**, after explicit root quiet release following A's completed measurements, with E paused. Exactly two prebuilt CPU31 invocations completed, no repeats. Each raw CSV contains one header plus four cases of one first-call and 31 warm rows (129 lines).

No competing process among monitored compiler/test names was detected at 200ms intervals. Root scheduling and process/load snapshots are evidence of the controlled worker window, not dedicated hardware or zero activity. An unrelated python3 process remains in snapshots (ps lifetime CPU ~129–130%); host 1m load was 5.10 before / 4.85 after. Shared-host scheduling/allocator noise remains possible. Run 2's baseline tails are wider; all samples remain included.

Raw files: `updated-run-1.csv`, `updated-run-2.csv`; `updated-timing-summary.txt` contains p10/p50/p90, p95/MAD, paired ratios, faster counts and separate first-call observations. Quantile index is floor((n-1)*p); MAD is unscaled median absolute deviation.

Times are nanoseconds/operation; brackets contain p95 and MAD. Ratio is median paired candidate/baseline.

| Case | Run 1 baseline | Run 1 candidate | Ratio | Run 2 baseline | Run 2 candidate | Ratio |
|---|---:|---:|---:|---:|---:|---:|
| Hex only | 573.750 [586.395, 3.990] | 36.254 [38.161, 0.573] | 0.06329 | 558.457 [698.499, 5.176] | 35.894 [38.066, 0.604] | 0.06345 |
| Full ETag, empty | 621.121 [677.148, 3.359] | 85.013 [87.506, 0.499] | 0.13640 | 627.390 [1046.022, 4.698] | 85.135 [92.295, 0.733] | 0.13557 |
| Full ETag, 256B | 738.873 [751.321, 5.133] | 209.171 [214.017, 1.605] | 0.28285 | 750.315 [816.788, 9.629] | 211.447 [216.603, 2.052] | 0.28183 |
| Full ETag, 1MiB | 453012.390 [460902.690, 1739.620] | 452228.370 [461796.300, 1715.920] | 0.99696 | 454889.610 [475417.580, 2490.140] | 453775.780 [459278.960, 1892.240] | 0.99769 |

Hex, empty and 256B improve in 31/31 pairs in both runs. Hex-only median savings are ~0.52–0.54 microseconds and well beyond observed paired noise, meeting the criterion. Large-body faster counts are only 23/31 and 21/31; ratio p10–p90 spans 0.99247–1.00367 and 0.97671–1.00546, crossing 1, and median differences are below individual-time MAD. No meaningful 1MiB improvement is established.

Original Rust1.99.0/sha2 0.10.9 measurements are separate historical evidence in commit `d19f947600874feb2a84270461ddbf43bddcd1ed` under `ablation/C`; these new runs are not a cross-version causal comparison and do not overwrite that archive.

## Validation and review scope

At actual compiled head `ba6ac88`, with `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 VITEST_MAX_WORKERS=2`:

- Full `cargo xtask check`: PASS, exit 0 — 99 Rust unit tests, 766 frontend tests / 138 files, formatting, Clippy, API generation, lint/types/i18n, notices, advisories/bans/sources and Rust/pnpm licenses. Safe stage/count excerpt: `new-pin-check-summary.txt`.
- Standard `cargo xtask test --keep`: PASS, exit 0 — 376 passed / 0 failed / 18 filtered, including all 3 cache API tests. Safe API excerpt and exact totals: `new-pin-api-summary.txt`, `updated-integration-summary.json`.
- Cache tests retain authorization-before-304, user changes, revoked 404/logout 401, weak/strong comparisons, concurrent 200/412 saves, unchanged losing body, replay-before-precondition and base-revision 409 behavior.
- Local OIDC/Supabase: NOT RUN; no real CDN test. CI at `ba6ac88` run 40094631353 passed all six jobs including OIDC and Supabase direct/session-pooler.

Actual CI runtime proof at both `ba6ac88` and `6d2d1a4` licenses jobs shows image `sha256:055178adbb91af6ec677fc26e56ea5f1bdb72ac9b50ed2c077688e8ed2f8946f`, Rust 1.99.1, Node 24.22.1, Corepack 0.36.0, pnpm 12.15.0 (`updated-ci-runtime.txt`, `latest-ci-runtime.txt`). Current pushed head `c1da0cb` CI run **40096229732** is in progress at this checkpoint; no older green status is represented as its final result.

Full CodeRabbit review at `ba6ac88` processed the one-file diff with no actionable findings; no inline/outside-diff findings or formal review submissions. Incremental merges were skipped, and the full request for `6d2d1a4` was aborted when the head changed. One full review was requested for current `c1da0cb`: https://github.com/fvoci/GroundZero/pull/121#issuecomment-6384556727. Its completion remains pending. The bot's docstring warning is a non-project threshold with a reasoned prior disposition; no cosmetic source edits or rule relaxation were made.

## Resource preservation and remaining work

Own retained project `fvoci-ablation-etag-bf3b8f-test`, PG17 endpoint `127.0.0.1:54953`, belongs to Orca run `run_fa40a3b51085` and original C task `task_1a76e81bd4b2` / dispatch `ctx_809d9193a46e`; finalization task `task_3d0f0ca46eb1`, active dispatch `ctx_3eddc0cc153d`. Full PostgreSQL container identity/config hash matched before and after updated tests (`updated-resource-preflight.txt`, `updated-resource-after.txt`), preserving the same service instance. No manual DB/volume/backups or resource deletion occurred. `--keep` retains services, while normal fixtures may clean only their successfully created full UUID name through the same owned admin connection; no prefix-only cleanup was used. Runtime fixture names are not logged by existing helpers, an evidence limit already reported to root.

The latest main merges and metric synthesis performed no further heavy builds or timing. Source PR stays Draft; root owns final source/pin reconciliation, evidence archival, Ready/merge and final main CI. Worker settlement is explicitly held while latest-head CI/review and possible all-code reconciliation finish.
