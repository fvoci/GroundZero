# Ablation A: safe baseline evidence payload

Paths in `archive/files.txt` are relative to `target/ablation-a/`. Archive only that allowlist; `archive/SHA256SUMS` covers every payload file except itself. Original receipts, full logs, process/environment internals, node_modules, Cargo build outputs, executable binaries, images, and unrelated files are excluded. The allowlist contains synthetic fixtures and result hashes only; no credentials or cookies. Runtime identifiers are omitted from the public database summary; the exact private runtime binding remains local and is bound by SHA-256.

Baseline source: `22ad80ee205332e3ef1ffd8f64f783789955921b`; executable change: `27445253910e4ab620b7f7b596306014e9cb6e28`; documentation-only follow-up: `e7080b951eaade1d8cdc9d7c15730aa3bde59222`. The two archived source modules are the exact measured implementations. `archive/environment.json` contains allowlisted tool versions, build flags, input-pin hashes and measurement timestamps, not a raw environment dump.

Use a new scratch copy of the payload to reproduce: the scripts write new result files, so do not run them over the archived measurements. With Rust 1.99.0 and the locked crates available, from the payload root:

```sh
CARGO_TARGET_DIR=build cargo +1.99.0 build --offline --locked --release --manifest-path harness/Cargo.toml -j2
cp build/release/ablation-a-harness baseline
CARGO_TARGET_DIR=build cargo +1.99.0 build --offline --locked --release --features variant --manifest-path harness/Cargo.toml -j2
cp build/release/ablation-a-harness variant
./baseline golden
./variant verify
./variant golden
CARGO_TARGET_DIR=build cargo +1.99.0 test --offline --locked --release --features variant --manifest-path harness/Cargo.toml -j2
```

The oracle compares complete strings, not just hashes: 326 cases include malformed shapes, every block kind, Unicode, cap boundaries, marker/newline accounting and trim-before-cap overshoot. `baseline-golden.tsv` was recorded before the product edit; `variant-golden.tsv` and `differential.txt` capture equivalence. Module tests passed six tests across the original and variant modules.

Only in a separately assigned quiet window, run `node run-matrix.mjs` then `node analyze.mjs`. The original run retained all 200 samples: ten alternating AB/BA pairs for each of five cells in warm and process-cold modes. Warm performs five untimed calls; process-cold means a fresh process without explicit warmup and reports a batch including its first call, not cold OS caches or isolated first-call latency. `process-monitor.jsonl` records 100 process-name checkpoints with no detected competing compiler/test processes. `measurement-input-hashes.txt` records the original executable hashes; executable files themselves are deliberately excluded.

Many-node paired median improvements were 96–99%; tiny inputs cost 3–4%, under the declared 5% regression ceiling; a single huge text node was effectively unchanged. This is helper-level performance, not an endpoint claim. Baseline relative MAD was below 5% in every cell; no samples were dropped.

`archive/validation-summary.txt` records standard-check PASS (100 Rust and 766 web tests), full isolated integration PASS (377 tests, zero failures, OIDC/Supabase profiles NOT RUN locally), and the documentation follow-up check. Full source and standard service definitions are available at the pinned public repository commit; they are not duplicated here.

Database evidence is observational, not a cleanup script. `archive/database-ddl.tsv` records exact synthetic CREATE/DROP names and sessions on the owned PostgreSQL instance. `archive/fixture-cleanup-source.txt` records the successful CREATE checks and same-name/same-admin-connection cleanup paths. `archive/database-ownership.json` matches all nine drops to earlier creations on the same server/session and records four retained scratch DBs. Main test DB, containers, volumes and backups were retained. Do not infer permission for any further cleanup from this archive.

Initial review/CI: Draft PR https://github.com/fvoci/GroundZero/pull/122; all six CI jobs passed at `e7080b9` in https://github.com/fvoci/GroundZero/actions/runs/40086365960. CodeRabbit reviewed that exact head without actionable findings; its independent Clippy tool failed, while pinned local/CI Clippy passed. These are historical results on the initial pins; updated-main regression and fresh quiet measurement remain a separate gate.
