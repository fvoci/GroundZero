# Experiment C: quoted ETag hexadecimal formatting

Status: local formatting performance hypothesis supported; adoption remains conditional on final checks and the coordinator's updated-main validation. No endpoint latency claim. Only `crates/server/src/http/cache.rs` is changed; no manifest, lock, toolchain, SQL, permission or transaction changes.

## Baseline and protocol

Baseline HEAD: `22ad80ee205332e3ef1ffd8f64f783789955921b` (clean before this experiment). Rust/cargo 1.99.0; x86_64 AMD Ryzen 9 5950X, 32 logical CPUs. Exact versions, file hashes and initial source are in `identity.txt` and `baseline-source.txt`.

Cargo.lock SHA-256: `afacc07bfd285d2079e5ef770e62e2d3aafd011f8aeb31dff0de63a9314a0d49`.
rust-toolchain.toml SHA-256: `72f7dd8c9fdf9c1fcd8eb8bf9326d4bc66833e4ebf4bc2932636e2b85efcb7af`.

The pre-change hypothesis, error invariants and acceptance criterion were recorded in `plan.md` before timing or changing the production function. Accept local formatting improvement only if its median improves at least 10% and at least 90% of paired rounds improve in both runs. This is not a criterion for endpoint or large-body improvement.

The baseline creates a temporary formatted String for each of 16 digest bytes, collects an intermediate hexadecimal String, then quotes it. The candidate emits two ASCII digits per byte directly into one capacity-34 String. The prefix `fvoci-representation-1\0`, user bytes, NUL separator, body bytes, SHA-256, first 16 digest bytes, lowercase digits and quotes are identical. Allocation changes are inferred from source; total allocations were not instrumented.

## Measurement

The optimized standalone harness in `bench.rs` contains the exact old/new production functions, differing only in name (`source-match.txt`). It uses this worktree's locked release sha2 0.10.9; no other worker artifacts. It black-boxes input/output and includes String destruction. Identical synthetic bodies: empty, 256 bytes and 1 MiB, rotating three distinct 16-byte users. Formatting-only samples use precomputed digests. Each case warms each variant for three batches, then records 31 pairs in alternating AB/BA order. Batch sizes are 20,000 / 20,000 / 10,000 / 100 operations. CPU affinity: logical CPU 31.

An initial window at 09:57:26 UTC was **NOT VALID**: the monitor found cargo PID 4098098 at approximately 09:57:32 UTC and terminated sampling. `run-1.csv` (129 lines), `run-2.csv` (66 lines) and `contamination.txt` are preserved and excluded entirely.

Accepted window: 2026-10-24 10:02:55–10:03:04 UTC, explicitly granted after all workers acknowledged idle. No competing process among the monitored compiler/test names was detected by a 200ms monitor; process snapshots and load averages are in `clean-*`. This is shared hardware, not a dedicated idle machine: unrelated background python3 activity appears in snapshots, so residual scheduling/host noise remains possible despite CPU31 affinity and low paired formatting MAD. Raw accepted files: `run-3.csv`, `run-4.csv`; stderr files are empty. Historical load averages were approximately 1.3–1.4 (1m), 3.7 (5m), 4.1 (15m); these are not instantaneous active build counts. The coordinator's later accidental pnpm-format interval, 10:03:49.629–10:03:52.890 UTC, does not overlap this accepted window.

All times below are nanoseconds per operation. Brackets are p95, MAD (unscaled); ratios are median paired candidate/baseline.

| Case | Run 3 baseline | Run 3 candidate | Ratio | Run 4 baseline | Run 4 candidate | Ratio |
|---|---:|---:|---:|---:|---:|---:|
| Hex only | 550.950 [563.829, 3.980] | 35.387 [36.852, 0.480] | 0.06363 | 550.327 [559.023, 2.366] | 35.104 [35.837, 0.383] | 0.06367 |
| Full ETag, empty | 597.378 [609.976, 2.613] | 81.980 [85.134, 0.836] | 0.13723 | 608.960 [630.335, 2.732] | 82.532 [84.518, 0.773] | 0.13499 |
| Full ETag, 256B | 720.791 [734.342, 5.601] | 201.076 [206.024, 1.088] | 0.27838 | 737.882 [744.654, 3.382] | 199.776 [204.656, 1.581] | 0.26995 |
| Full ETag, 1MiB | 446012.700 [451440.180, 3130.540] | 445438.500 [451871.080, 3013.340] | 1.00043 | 446846.510 [455487.640, 2603.630] | 446295.200 [457283.650, 4812.370] | 0.99855 |

Hex/empty/256B improve in 31/31 pairs in each run. Large-body candidate improves in only 14/31 and 18/31; paired ratio p10–p90 spans 0.9874–1.0110 and 0.9831–1.0123. Thus the ~0.5 microsecond formatting benefit is well beyond observed hex noise; large-body total improvement is not established. This supports the local formatting optimization, not a maintenance-only rationale, and rejects a meaningful 1MiB speed claim.

`summary.txt` includes p10/p50/p90 and first-call samples; `robust-summary.jsonl` includes p50/p95/MAD. Quantiles use sorted index floor((n-1)*p). First-call samples are separate, single observations in baseline-then-candidate order; they are not a statistically valid cold-start comparison (hash/allocator state may already be warm). No outliers were discarded from accepted runs.

## Behavior and side-effect boundary

`documents::representation` serializes a Document to response bytes, then calls `strong_etag`. Its users are document GET, document_reply (save/restore success), and conflict_reply (409/412). GET authenticates, checks access and completes its read transaction before conditional comparison. Save validates input, checks permission, locks rows and checks replay before If-Match, then revision/no-op/mutation decisions. These callers and all error/transaction/audit/event code remain byte-for-byte untouched.

`strong_etag` is pure: no DB, audit, event, network, clock or mutable application state. Exact returned-byte equivalence preserves the ETag header/body fields and comparison operands; body serialization, response bytes, branch outcomes, errors and DB effects are unchanged at this boundary. This is a source/byte-equivalence argument, not a claim that this microbenchmark measured DB effects.

Before the candidate change, original production code plus regression checks passed 7 cache unit tests (`baseline-clean-test.log`, no warnings). The seventh was a temporary mocked policy test dropped after coordinator review; it did not prove real permission ordering. The retained regression checks four independent fixed vectors and 257 binary body/user splits against the original encoding, explicitly requiring a zero digest byte to exercise padding. Existing same/different-user and weak/strong/wildcard/absent/empty-header tests remain.

Standalone verification: PASS four fixed vectors and 3,072 deterministic binary body/user pairs for both whole functions and formatting-only functions (`equivalence.log`). Fixed vector digests were independently checked with OpenSSL. Existing `tests/cache.rs` checks actual response bytes/compression, 304 after access, revoked 404/logout 401, user switches, concurrent save 200/412, unchanged losing body, replay-before-precondition and 409 semantics.

Final candidate targeted cache unit tests: **PASS 6/6** (`variant-test.log`). `rustfmt --check` and `git diff --check`: **PASS**. Exact measured-to-production function source comparison: **PASS**, and the final source checksum remained unchanged through integration execution.

`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo xtask test --keep`: **PASS**, exit 0; 376 passed, 0 failed, 0 ignored, 18 filtered across 48 test summaries (including doc-test summaries). This includes **all 3 cache API tests**, plus the remaining standard workspace tests (`integration-test.log`, `integration-summary.json`). OIDC and Supabase were explicitly NOT RUN. The standard runtime used this worktree's isolated PostgreSQL 17, Meilisearch and SeaweedFS project `fvoci-ablation-etag-bf3b8f-test`; services remain retained (`retained-services.jsonl`), and no down/volume deletion was invoked.

`cargo clippy -p fvoci-server --lib --locked -j2 -- -D warnings`: **PASS**, exit 0 (`clippy.log`). Full `cargo xtask check`, OIDC, Supabase, real CDN, and updated-main/updated-sha2 remeasurement are NOT RUN for this fixed-baseline experiment.

## Reproduction and remaining work

Build harness without modifying manifests:

```sh
cargo build -p sha2 --release --locked -j2
rustc --edition 2024 -C opt-level=3 -L dependency=target/release/deps --extern sha2=$(printf '%s' target/release/deps/libsha2-*.rlib) target/ablation-c/bench.rs -o target/ablation-c/bench
target/ablation-c/bench --verify-only
# Only in an explicitly granted quiet window; preserve separate outputs:
taskset -c 31 target/ablation-c/bench > target/ablation-c/reproduction.csv
node target/ablation-c/summarize.mjs target/ablation-c/reproduction.csv
```

Actual final checks:

```sh
cargo test -p fvoci-server --lib http::cache --locked -j2
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo xtask test --keep
cargo clippy -p fvoci-server --lib --locked -j2 -- -D warnings
rustfmt --edition 2024 --check crates/server/src/http/cache.rs
git diff --check
```

`final.patch` contains the complete 47-addition/2-removal source change. `variant-source.sha256` pins the source tested, and `evidence.sha256` pins the principal evidence artifacts. HEAD remains the fixed baseline, with the reviewed candidate uncommitted in its experiment worktree; no history rewrite occurred.

Final code intentionally uses the measured fixed-loop implementation; the existing hex crate alternative was considered but would be a different, unmeasured candidate and would retain an intermediate allocation. No new dependency or formatting error path is needed.

No PR has been created. Before adoption/PR, the coordinator must inspect evidence, complete API/integration/CI requirements, and repeat the locked tests and quiet measurement after the planned dependency merge onto updated main. A future Draft PR and @coderabbitai review are requested; no Ready/merge/tag/deploy action belongs to this experiment.
