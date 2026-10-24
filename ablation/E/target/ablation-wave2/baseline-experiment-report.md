# Wave 2 E — grouped home marks

Status: correctness proven on pinned baseline; timing awaits explicit coordinator quiet slot. No PR, no exploration beyond E. Product ownership limited to `crates/server/src/features/home.rs`; reusable regression limited to `crates/server/tests/home_marks_ablation.rs`. Raw comparison harness/results are target-only.

## Fixed baseline and environment

- Base `22ad80ee205332e3ef1ffd8f64f783789955921b`, initially clean; no pending dependency commits included.
- Exact lock/toolchain/source/migration/Compose hashes in `baseline-environment.txt`; Rust/Cargo 1.99.0; private `target/ablation-build`; compile jobs=2.
- Standard PG17.9 digest-pinned `deploy/compose.test.yaml`; runtime `SHOW transaction_isolation` = `read committed`.
- Own checkout-derived project `fvoci-ablation-home-marks-f0352d-test`, services PostgreSQL/Meilisearch/SeaweedFS. Services/volumes/data remain retained. Only fresh synthetic users/workspaces/notes/marks in that service's `fvoci` DB; no temporary DBs created, no database/container/volume/backup deletion, no Cloud/external services, no other worker's services or outputs used.
- Read-only proposal input: coordinator-reviewed `ablation-body-size/target/ablation-wave2/proposal.md`. Rejected B source, outputs and services unused. No .codegraph directory. Repository entrypoint, relevant approved decisions/development and Rust/test/orchestration/runtime guidance consulted.

## Change and protected semantics

`marks()` reads category tag plus the unchanged projection, LEFT JOINs and `app_time_target_label` once with `mark IN ('pin','favorite')`. Iteration stably partitions rows into the two output lists, preserving `created_at,target_id` within each, target parsing and NULL-label filtering. Both-marked targets remain in both lists. Same UserTx/RLS/read-time access checks/API DTOs/commit point; no changes to writes, limits, schemas, dependencies, isolation, locks, cache, events or audit behavior.

READ COMMITTED caveat (explicitly accepted in dispatch): old Pin and Favorite statements can see different snapshots. In the controlled shared-personal-note revocation, old Pin read returned an authorized label before revoke, old Favorite read returned NULL afterward, and grouped read after revoke omitted both. One grouped statement sees one snapshot; a revoke after that snapshot may leave both labels in that response, while a later statement excludes both. This is authorized-at-statement-snapshot equivalence on stable inputs, not identical instruction interleavings or immediate post-revoke response suppression. No new snapshot policy was inferred.

## Pre-edit baseline evidence

Pre-edit output and plan capture completed before home.rs changed (saved baseline Home JSON timestamps 10:22:33–34 UTC, product edit afterward).

| Marks per category | Baseline SELECTs | Grouped SELECTs | Raw rows both | Baseline summed hits | Grouped hits | Baseline estimated total costs | Grouped cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 2 | 1 | 0 | 2 | 1 | 33.96 + 33.96 | 33.96 |
| 1 | 2 | 1 | 2 | 74 | 56 | 33.96 + 33.96 | 33.96 |
| 100 | 2 | 1 | 200 | 3844 | 3826 | 34.09 + 34.09 | 38.37 |

All shared reads=0 in captured warm plans. Top nodes Sort; detailed joins, actual/estimated rows, loop counts and buffers in `baseline-n*-*-plan.json` / `grouped-n*-plan.json`. EXPLAIN ANALYZE used TIMING OFF for node instrumentation, before timed benchmarking; its incidental statement timing is not treated as benchmark data. Estimates are planner estimates on small synthetic fixtures, not elapsed-time evidence. At 100/category, physical label/access work is nearly unchanged; exactly one SQL round trip removed, not half all label work.

`postgres-observer.log` confirms baseline execute lines for pin and favorite and one tagged SELECT under fvoci_app (same projection text as helpers); `roundtrip-observer-rerun.log` confirms equal raw output and global before/after counts: audit=236, documents=109, events=0, items=8, marks=243, notifications=0, tasks=69. Later variant observation also left all counts unchanged. This SQL observer exercises the actual statements directly; handler's 2->1 call count also verified by source diff, not a full HTTP wire trace.

## Correctness results

PASS on both baseline and variant:

- Existing `home` integration binary: 3 tests.
- Baseline-oracle matrix: empty, pin-only, favorite-only, both-marked, all three target kinds, personal and shared notes, tied creation timestamps, category-local target-id order, renamed/archived/deleted/hidden targets, owner/admin/reader/outsider and forged cross-tenant/stale/private-note marks, anonymous RLS NULL user, HTTP 401 and invalid timezone 400.
- Stable raw baseline/grouped exact JSON at 0/1/100 each, same row counts and no mark/audit/event/notification/content mutations.
- Exact complete Home JSON comparison against pre-edit saved baseline for unchanged 0/1/100 fixtures (`variant-full-home.log`).
- Controlled cross-user share revocation at a deliberate old-statement boundary; label changes consistent with each statement's snapshot.
- Actual old/grouped marks SELECT errors with SQLSTATE 42P01 (test-local missing search_path), actual blocked marks cancellation via pg_cancel_backend SQLSTATE 57014, dropped UserTx rollback and pool reuse. Lock used only by test fixture to deterministically block/cancel the SELECT; no product locks added.

Initial raw-harness run FAILED on harness-only relative output paths and a prepared multi-command fixture statement; corrected and rerun PASS (`baseline-tests-rerun.log`). First observer FAILED on its admin connection's search_path; corrected session-local path then rerun PASS. These failures did not change product behavior or permissions. Preserve failed logs as honest execution history.

Final tracked regression removes raw harness inclusion and baseline SQL duplication, retaining category/both-target/order/visibility/access/error coverage. Broad temporary oracle/cost/race/metrics code and compiled binary stay in target, outside normal test discovery.

NOT RUN so far: full cargo xtask check, full standard cargo xtask test, PG18/OIDC/Supabase/Cloud, dependency-updated-main validation, CI and CodeRabbit. These must not be inferred from targeted PASS. Final minimal change/raw metrics require coordinator review before any PR; adoption/Ready/merge remain coordinator-owned.

## Timing protocol and gate

Declared before product edit in `baseline-notes.md`: paired ABBA/BAAB, 12 plan-first blocks and 80 warm blocks per 0/1/100 fixture; per variant p50/p95/MAD, paired-block percentage delta/MAD. Common-case regression rejected when repeatable paired median delta >5% and >3x paired-delta MAD; ambiguous data is insufficient, no forced speed win. Server roundtrip and physical cost reduction remain separate from latency claims.

Same synthetic personal-document targets are both pinned and favorited in timing fixtures (0/1/100 each), same retained DB inputs for A and B. Timing scope is optimized-release SQL fetch/decode/filter/stable split stage, excludes UserTx setup/commit, connection establishment, JSON encoding and full home handler. Driver/server prepared statements cleared for plan-first; warm statements reused; plan-first does not mean cold server buffers or new connection. Both SQL texts are static literals, and the grouped literal is asserted byte-for-byte present in product source. Never translate these measurements to end-to-end home performance. No debug latency samples were collected.

Reproduction: `target/ablation-wave2/run-env.sh target/ablation-wave2/harness-release-static-bin raw::raw_benchmark --ignored --nocapture` only in coordinator-granted quiet slot. `node target/ablation-wave2/summarize.mjs target/ablation-wave2/latency.csv` produces metrics. Final optimized static-query binary/source hashes in `release-static-harness.sha256`; raw sources `release-static-test-source.rs` and `release-static-raw-source.rs` show exact queries, fixture setup and test-only synchronization. Restoring archived test source for rebuild is temporary experiment work, not required by final reusable regression. Earlier debug and initial release binaries remain historical correctness evidence, not final timing executables.

Ordering detail: ties on created_at with distinct target_id are tested. As before, if different target kinds somehow share both created_at and target_id, SQL specifies no additional tie-breaker; no new kind/category ordering guarantee is claimed.

Timing uses the saved 0/1/100 fixture identities; correctness setup also created other synthetic identities in this isolated DB after the initial plans. A/B comparisons run back-to-back against identical retained DB state, rather than asserting that global table cardinality never changed during all correctness work. No writes/fixture generation run concurrently with timing.

## Final source checks and optimized readiness

PASS: `CARGO_TARGET_DIR=target/ablation-build CARGO_BUILD_JOBS=2 cargo fmt --all --check`, `cargo clippy --locked -p fvoci-server --lib --test home_marks_ablation -- -D warnings`, final normal targeted tests (`home`: 3, `home_marks_ablation`: 1). Raw harness is absent from final normal test discovery.

Optimized release harness built with the same pinned source/toolchain/locks using `cargo test --release --locked -p fvoci-server --test home_marks_ablation --no-run` and jobs=2: PASS in 3m57s. Preserved as `harness-release-bin`, `release-test-source.rs`, `release-raw-source.rs`, SHA-256 list `release-harness.sha256`. Its nonignored correctness checks (matrix/race/errors/cancel/SQL observer) PASS, full Home saved-baseline comparison PASS (`release-correctness.log`, `release-full-home.log`). No debug latency benchmark was run. Optimized latency awaits the new quiet slot.

Coordinator subsequently removed the original proposal worktree after archival; E has no runtime dependency on that path. Coordinator-provided recovery permalink: https://github.com/fvoci/GroundZero/blob/1d6527921ee3fd654afcb705d82ccefb350194df/ablation/wave2/proposal.md (E did not independently fetch or use rejected B evidence).

## Pre-timing fidelity correction and final environment snapshot

Coordinator review required removing harness-only SQL-string construction before measurement. This was corrected before any latency sample: both baseline and candidate query text are now static; the exact grouped literal is checked against the compiled product source. Cached optimized rebuild PASS in 6.24s (`release-static-build-rerun.log`), final executable `harness-release-static-bin`, sources and SHA-256 in `release-static-harness.sha256`. The first fidelity rebuild FAILED on an owned test-only String/&str conditional; corrected before the passing rebuild, with no product change.

`current-cost.log` PASS and `release-static-full-home.log` PASS recapture stable output and current plans after all fixture generation. `current-cardinality.json` records global counts audit=470/documents=117/events=0/items=21/marks=302/notifications=0/tasks=204 and the same three synthetic users with 0/2/200 raw marks rows respectively. `current-*-plan.json` / `current-plan-summary.jsonl` are the plan evidence linked to final timings; earlier baseline plans remain pre-edit proof. No subsequent fixture writes run in E before measurement. JSON byte counts are diagnostic serialized output sizes, not PostgreSQL wire bytes.

No maintenance win is claimed merely from combining calls: product code adds explicit category splitting. Adoption must rely on actual bounded latency/cost tradeoff without a common-case regression; a null/noisy latency result does not automatically become a maintenance success.

Current post-fixture plan summary linked to upcoming timing (all reads=0):

| Marks/category | Baseline summed hits | Grouped hits | Baseline total cost sum | Grouped cost | Rows A/B |
| --- | --- | --- | --- | --- | --- |
| 0 | 4 | 2 | 68.16 | 38.36 | 0/0 |
| 1 | 80 | 61 | 68.16 | 64.16 | 2/2 |
| 100 | 8704 | 5989 | 411.88 | 226.31 | 200/200 |

The 100/category current plan uses merge/hash joins and scans the visible documents plus RLS-filtered tasks/items once per statement. Old per-category plans each scan documents (100 returned, 17 filtered), tasks (0 returned, 204 filtered) and items (0 returned, 21 filtered); grouped scans these once. Therefore physical shared-join/access work savings are data/plan-dependent and larger here than in initial nested-loop plans; emitted label count remains 200 in both. This is not a claim that all labeling work is halved, nor a timing result.

## Optimized timing results — two independent passes

PASS for the declared local experiment; candidate adoption still belongs to coordinator review and later updated-main validation. Explicit quiet grant was received via `msg_2e26a840bec1`, and measurement ran **2026-10-24 11:09:43–11:10:32 UTC (49 seconds)**. Both optimized static-query passes completed inside the granted window. Quiet-slot release was sent immediately as `msg_8ee737977be6`; no builds, algorithm changes or fixture writes occurred during timing. The outstanding optimized-slot ask `msg_2386686184e6` timed out while the separate status grant arrived; that question is superseded by the grant and completion status, not a new permission request.

Each pass: 80 warm ABBA/BAAB blocks = 160 samples per implementation per fixture; 12 plan-first blocks = 24 samples per implementation per fixture. A is the original two SELECTs; B is grouped SELECT + stable split. Units below are **milliseconds** except paired change/MAD in percentage points. Raw samples and full-precision summaries: `latency-pass1.csv`, `latency-pass2.csv`, `summary-pass1.json`, `summary-pass2.json`.

| Pass | Marks/category | Warm p50 A→B | Warm p95 A→B | Warm MAD A→B | Paired median change | Paired MAD (pp) |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 0 | 0.9195→0.4697 | 1.2536→0.6369 | 0.0608→0.0280 | -48.59% | 2.82 |
| 2 | 0 | 0.9066→0.4568 | 1.0747→0.5591 | 0.0495→0.0258 | -49.22% | 2.30 |
| 1 | 1 | 1.8989→1.2185 | 2.2204→1.4042 | 0.0837→0.0529 | -35.68% | 2.10 |
| 2 | 1 | 1.9177→1.2366 | 2.5955→1.6056 | 0.0766→0.0657 | -36.44% | 2.37 |
| 1 | 100 | 65.3612→53.1539 | 74.9176→59.1306 | 0.8373→1.4424 | -19.32% | 1.46 |
| 2 | 100 | 65.6339→53.3128 | 76.7784→58.4107 | 0.8349→1.3245 | -19.29% | 1.18 |

| Pass | Marks/category | Plan-first p50 A→B | Plan-first p95 A→B | Plan-first MAD A→B | Paired median change | Paired MAD (pp) |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 0 | 1.9298→1.1883 | 2.2084→1.3475 | 0.1038→0.0257 | -36.89% | 1.00 |
| 2 | 0 | 2.1997→1.3606 | 3.2101→1.9519 | 0.2606→0.1524 | -39.19% | 1.89 |
| 1 | 1 | 2.8680→1.9582 | 3.5880→2.5715 | 0.0873→0.0405 | -32.67% | 2.51 |
| 2 | 1 | 2.8617→1.9525 | 3.7111→2.5622 | 0.1157→0.1163 | -33.09% | 2.85 |
| 1 | 100 | 56.0833→48.0578 | 63.4527→51.7868 | 0.5525→1.2170 | -15.18% | 0.76 |
| 2 | 100 | 56.3908→47.2286 | 62.0182→51.0429 | 1.1645→0.5462 | -16.31% | 0.85 |

The common-case regression gate passes in every fixture/phase in both passes: no positive median regression, all gain medians exceed 3× paired MAD. At 100/category B has a higher absolute MAD than A, while its p50/p95 remain lower and the paired gain repeats. This is a bounded marks-stage result on the declared personal-document/both-marked synthetic fixture, **not an end-to-end home latency claim or universal percentage speedup**. Physical query reduction is exactly one client DB roundtrip (2→1), not half all label work. Extra Rust splitting is accepted only for this measured tradeoff; no separate maintenance win is invented.

Immediately before timing, `raw_current_cost` refreshed current plans/cardinality read-only (`quiet-current-cost.log` PASS), same global counts and fixture identities as the earlier capture. Older snapshots remain in `prequiet/`. Plans are the captured direct statement executions; the warm driver/server prepared-statement cache can select generic plans differently from first-plan executions, so do not attribute every warm latency difference to a specific EXPLAIN node or claim these plans are a trace of every warm execution.

## Noise and resource qualification

This was shared-background quiet, not dedicated hardware. `process-monitor.jsonl`, `vmstat.log` and `docker-events.jsonl` preserve all observations. No Cargo/rustc/cc/ld/pnpm/npm build process and no container start/die event were observed. Do not interpret absence of the literal process name `node` as zero Node activity: services appear as `MainThread`; pre-existing MainThread processes used 528 CPU ticks in aggregate, and four new MainThread entries had zero measured CPU ticks. Pre-existing python3 used 4793 ticks, codex processes 1681 ticks, expected PostgreSQL 4346 ticks; agents/daemons and healthcheck processes remained active. vmstat tail showed 87–90% host idle. Small new postgrest/runc/pg_isready/sh processes are retained in the full record; no newly observed heavy build workload was detected, and paired noise is explicitly measured rather than erased. No samples were discarded.

Final source hashes still match `final-source-hashes.txt` (checked after the long wait), locks/toolchain unchanged. Services and all target proof remain retained for coordinator review/archival; PostgreSQL data is tmpfs-backed, so stopping its container loses that synthetic DB state. E performs no cleanup, PR, merge or additional candidate exploration.

## Conditional adoption and ongoing standard validation

Coordinator reviewed the actual product/regression, exact static optimized harness, raw samples and both distribution summaries, then conditionally adopted the pinned-baseline experiment. On that instruction the reusable regression was renamed from `home_marks_ablation.rs` to `home_marks.rs` and committed with only home.rs as `302587ef6f9f05513fb5e997769d4395712c8acc`; no raw artifacts/dependency changes entered the commit. Draft PR https://github.com/fvoci/GroundZero/pull/124 remains Draft. Explicit `@coderabbitai full review` requested body/inline/outside-diff review at that exact head.

PASS: complete `cargo xtask check`, 98 Rust unit tests and 138 web files/766 tests, licenses/contracts. First run's Vitest default worker count exceeded the intended limit and had already finished PASS before an attempted stop; no owned worker remained to terminate. The early status saying this would be an interrupted failure was corrected. The complete standard entry was rerun with CARGO_BUILD_JOBS=2, RUST_TEST_THREADS=2, VITEST_MAX_WORKERS=2 and PASS again (`standard-check-bounded.log`); a process checkpoint recorded two owned Vitest processes. No source/shared check configuration was weakened.

Full standard integration is RUNNING via `cargo xtask test --external-services`, against a second **fresh local owned** checkout-derived project `fvoci-ablation-home-marks-f0352d-test-standard`, using the unchanged standard Compose file and dynamic loopback ports. This external-services flag is the standard CI entry pointed at E's own local PG17/Meili/S3 containers, not Cloud/shared external testing. Both projects remain retained; original benchmark DB/data and storage are untouched. `standard-server-before.json` recorded only fvoci/postgres/template0/template1, empty admin role config, PG17.9 and exact server system_identifier before tests.

DDL provenance is enabled only on the fresh standard server's admin role after bootstrap/migrate and before integration binaries start. Test source lineage was checked: FreshDb/empty_database generates a full UUIDv7 database name, requires successful CREATE, and subsequent DROP uses that returned exact name and same admin connection. Only those disposable test-internal databases are authorized for normal cleanup, not original/existing/uncertain/retained databases or volumes/backups. Observer role defaults will be restored afterward; services/data remain. This continuation is not settled yet: remote CI/CodeRabbit, updated dependency main and a later explicit quiet rerun remain required; Ready/merge remains coordinator-only.
