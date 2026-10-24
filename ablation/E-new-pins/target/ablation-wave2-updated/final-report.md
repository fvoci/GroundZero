# E final updated-pin evidence and resource handoff

E was adopted and merged by coordinator as PR124 commit479b562522f3b2fa1696e6c5828c753fc91124fd. Worker changed only home.rs and reusable tests/home_marks.rs. A later explicit coordinator instruction authorized the test-only ordering follow-up PR126 at4cd44d0d5564100ece7a01759bf73273bc0ceb52, documented separately in ordering-followup.md; no more ablation measurements, product changes, cleanup or candidate exploration. Coordinator owns Ready/merge, combined-runtime checks and final CI after merge.

## Identity and contract

Original pinned experiment base22ad80ee205332e3ef1ffd8f64f783789955921b, Rust1.99.0: immutable baseline report/raw archive under target/ablation-wave2 and root evidence commit3454219. This report supplements it, never replaces original results. Updated exact dependency main e0103ef, local full-check/full-test actual source2ee60be, optimized prebuild actual source4c8551b13a0e7bbd57fbaab8776ce60ba6cab918. Timed updated passes1/2 current08b88c5; passes3/4 currentd69cbd4abf9a354be31836d426fdab355a9e0f88 after independent Unicode/A/C merges. E sources, locks/toolchain/migration and prebuilt harness hashes were verified unchanged through d69; compiled-source identity is not relabeled. environment.txt and harness*.sha256 retain exact identities. Runtime Rust1.99.1, Node24.22.1, Corepack0.36.0, pnpm12.15.0.

One static tagged SELECT replaces the Pin then Favorite queries and stably splits rows. Same UserTx, projection/joins, RLS, app_time_target_label, NULL filtering, target parse, category-local created_at/target_id order, both-marked targets, response and transaction commit. Exactly2->1 marks SQL statements; no schema/index/cache/isolation/lock/write/event/audit changes. This is one removed round trip, not half all target-label work.

READ COMMITTED: old two statements may observe different snapshots. Controlled shared-note revocation produced old Pin before revoke and Favorite omitted after revoke; grouped after revoke omitted both. A grouped read with a snapshot before revoke may return both authorized-at-that-snapshot labels. Stable-input exact output and statement-read-time authorization are preserved; identical interleavings or immediate post-revoke response suppression are not claimed.

## Correctness and standard validation

PASS: independent original-vs-grouped oracle and full Home golden JSON for0/1/100, empty/pin/favorite/both, item/task/document, personal/shared notes, tied ordering, hidden/deleted/archived/renamed targets, owner/admin/reader/outsider/cross-tenant forged stale marks, NULL-user RLS,401/invalid-tz400. Updated raw_current_cost and raw_verify_full_home passed on the ORIGINAL DB. Actual blocked-query cancellation57014, query error42P01, rollback/pool reuse and controlled revocation passed on UPDATED STANDARD DB, avoiding original fixture writes. Expected rows0/2/200 and exact A/B outputs asserted outside all timed blocks in every benchmark pass. Global original counts unchanged: audit470 documents117 events0 items21 marks302 notifications0 tasks204. Read stage adds zero mutations/events/audits.

PASS full bounded cargo xtask check at actual2ee:98Rust unit tests,138web files/766tests, licenses/contracts; PASS separate fresh local cargo xtask test --external-services:376/0 with PG17/Meilisearch/S3. CARGO_BUILD_JOBS2,RUST_TEST_THREADS2,VITEST_MAX_WORKERS2. Local OIDC/Supabase NOT RUN; current-head CI covers them. No redundant full local rebuild merely to label independent merges. Standard DDL audit records13CREATE,9successful same-name/same-session drops and4intentionally retained scratch DBs; initial protected DBs retained, role log/application_name defaults restored, original counts preserved.

Current d69 CI https://github.com/fvoci/GroundZero/actions/runs/40097758968 all6 PASS:check,licenses,integration,oidc,supabase,rhwp. ci-runtime-proof.txt records completed job log digest055178adbb91af6ec677fc26e56ea5f1bdb72ac9b50ed2c077688e8ed2f8946f and all4new runtime versions. CodeRabbit full current-head review receipt6384789328 finished13:34:18UTC; summary explicitly reviewed32e9c77..d69 and both files, no new actionable findings, empty inline threads. Historical2ee outside-inline nit5638281077 correctly observes reusable fixture checks equal-created_at target_id ties but not deliberately conflicting created_at-vs-target_id ordering; coordinator initially froze further source changes, then explicitly authorized separate test-only follow-up PR126 to close this gap; see ordering-followup.md. This was not hidden as no historical finding. Generic16.67% docstring warning and CR's own Clippy1.99.0 tool failure are not repository gate failures; pinned real local/CI checks passed. No check suppression/cosmetic coverage edit retained.

## Updated timing protocol and declared gates

Same unchanged optimized prebuilt binary, exact old/new static SQL, original preserved DB, synthetic personal documents with SAME targets both categories;0/1/100marks per category. Each full pass:80warm ABBA/BAAB blocks (160samples/variant/case),12plan_first blocks (24samples/variant/case). Timing covers typed fetch/filter/split marks stage only; no endpoint/SLO or production-general p95 claim. No samples excluded. Original median gate: a >5% median regression beyond3pairedMAD fails; gains must exceed3pairedMAD. All24cells across4passes satisfy median/noise gates.

Original updated quiet window13:14:55–13:15:45UTC retained untouched passes1/2. n100plan_first p95 was +3.4820% and +11.8065% candidate regression; this triggered one bounded coordinator-authorized characterization, declared in followup-protocol.md BEFORE new runs. Exactly2additional unchanged FULL passes3/4 under explicit quiet grant, not a rerun-until-green loop. Follow-up overall13:33:11–13:34:19UTC; between-pass guard paused on a replay of an already received proceed message (no redirect), acknowledged before pass4; distinct monitor files preserve both runs and pause provenance. No rebuild/profile/case/data/threshold change occurred.

Added gate: pool all4 n100plan_first passes96samples/variant, nearest-rank ceil(.95*N)=92, Bp95<=1.05Ap95 AND both new passes must not each repeat >5% p95 regression; otherwise insufficient/reject and stop. Actual pass3 A71.798424/B56.333305ms(-21.5396%); pass4 A80.314778/B52.888538ms(-34.1484%). Pooled A73.231543/B69.428320ms(-5.19342%) PASS. All original high tails remain in pooled data; N24 per-pass p95 is second-largest, so bounded sampling uncertainty remains. No universal p95 improvement assertion. No more measurement authorized.

| Pass | n/category | Phase | A/B p50 ms | A/B p95 ms | A/B MAD ms | paired delta% / MAD pp |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 0 | plan_first | 1.9593 / 1.2471 | 2.1177 / 1.5736 | 0.0333 / 0.0521 | -36.278 / 1.086 |
| 1 | 0 | warm | 0.8981 / 0.4495 | 1.2809 / 0.7017 | 0.0559 / 0.0322 | -49.416 / 1.882 |
| 1 | 1 | plan_first | 2.9330 / 2.0176 | 4.2697 / 3.7907 | 0.1910 / 0.1246 | -31.320 / 1.953 |
| 1 | 1 | warm | 1.8569 / 1.1976 | 2.2290 / 1.4848 | 0.0837 / 0.0562 | -35.084 / 2.481 |
| 1 | 100 | plan_first | 56.4045 / 47.2131 | 67.0922 / 69.4283 | 0.8444 / 0.6155 | -15.696 / 1.217 |
| 1 | 100 | warm | 65.2810 / 53.3012 | 85.5001 / 62.2678 | 0.7169 / 1.1888 | -19.322 / 1.175 |
| 2 | 0 | plan_first | 1.8290 / 1.1537 | 1.9517 / 1.3220 | 0.0515 / 0.0376 | -36.841 / 1.521 |
| 2 | 0 | warm | 0.8877 / 0.4475 | 1.2958 / 0.6550 | 0.0472 / 0.0331 | -48.781 / 2.868 |
| 2 | 1 | plan_first | 2.9110 / 1.9496 | 3.4909 / 2.5811 | 0.1600 / 0.1317 | -33.384 / 1.859 |
| 2 | 1 | warm | 1.8874 / 1.2069 | 2.4511 / 1.4945 | 0.0913 / 0.0609 | -35.578 / 2.419 |
| 2 | 100 | plan_first | 56.9195 / 47.6175 | 65.2380 / 72.9403 | 0.9803 / 0.8779 | -15.944 / 3.970 |
| 2 | 100 | warm | 65.1078 / 53.2531 | 79.3923 / 71.1891 | 0.7707 / 1.6393 | -18.924 / 1.310 |
| 3 | 0 | plan_first | 1.8340 / 1.1625 | 2.0322 / 1.3106 | 0.0824 / 0.0531 | -37.578 / 1.508 |
| 3 | 0 | warm | 0.8805 / 0.4415 | 1.0191 / 0.4897 | 0.0323 / 0.0186 | -50.204 / 1.991 |
| 3 | 1 | plan_first | 2.7116 / 1.8827 | 3.3343 / 2.6990 | 0.1044 / 0.0481 | -31.025 / 2.354 |
| 3 | 1 | warm | 1.8822 / 1.2028 | 2.9738 / 2.0706 | 0.0883 / 0.0608 | -35.730 / 2.869 |
| 3 | 100 | plan_first | 55.2627 / 46.6703 | 71.7984 / 56.3333 | 0.5919 / 0.4094 | -15.349 / 0.954 |
| 3 | 100 | warm | 63.6603 / 52.0197 | 82.0034 / 68.4176 | 0.8828 / 1.4952 | -18.510 / 1.057 |
| 4 | 0 | plan_first | 1.9770 / 1.2154 | 3.2862 / 2.2265 | 0.1374 / 0.0880 | -37.511 / 3.108 |
| 4 | 0 | warm | 0.9038 / 0.4546 | 1.2703 / 0.5900 | 0.0459 / 0.0228 | -49.339 / 2.105 |
| 4 | 1 | plan_first | 2.8644 / 1.9001 | 5.4051 / 2.9660 | 0.1665 / 0.0633 | -32.177 / 1.590 |
| 4 | 1 | warm | 1.8345 / 1.1636 | 2.1468 / 1.2978 | 0.0652 / 0.0311 | -35.941 / 2.036 |
| 4 | 100 | plan_first | 55.2053 / 46.4839 | 80.3148 / 52.8885 | 0.5207 / 0.2841 | -16.088 / 0.919 |
| 4 | 100 | warm | 63.6929 / 52.1685 | 86.4113 / 68.1605 | 0.8389 / 1.2746 | -18.783 / 0.844 |

## Plans and actual SQL cost

Direct current EXPLAIN(ANALYZE,BUFFERS) files retained; table below sums TWO A plans, versus one B. Direct plans are not a trace of every warm cached/generic execution. Planner units are estimates, not wall time; state/fixture/plan dependence matters. RLS/join scans may be shared by grouped plan, but labeling still applies per returned category row.

| n | A/B total planner cost | A/B actual rows | A/B shared hits | A/B shared reads |
| --- | --- | --- | --- | --- |
| 0 | 68.16/38.36 | 0/0 | 4/2 | 0/0 |
| 1 | 68.16/64.16 | 2/2 | 80/61 | 0/0 |
| 100 | 411.88/226.31 | 200/200 | 8704/5989 | 0/0 |

## Contamination and retained resources

No compiler/linker/cargo/pnpm/npm/git heavy job observed in either follow-up monitor. Four new PostgreSQL PIDs are expected benchmark connections, about1063–1084ticks each; no container start/die in whole follow-up. Approved existing Python PID3454939 accumulated2140ticks across the two monitoring segments, up to103/sample; it was pre-existing, not excluded. New native2.1.310 processes had0measuredticks; cannot attribute old tails to them. Existing service/agent/healthcheck activity remains in raw monitors, no dedicated-hardware claim. Coordinator independently inspected monitors and host idle (~89.6–89.8%) before adoption. First updated window's pre-existing Python5005ticks and last-sample native0ticks remain documented separately in new-process-evidence.json and tail-pass1-2.json. New heavy overlap was not observed; no outlier/sample was discarded.

Keep ALL3own standard Compose projects, no cleanup/volume/backup/prune/DB deletion: original fvoci-ablation-home-marks-f0352d-test (PG system identifier7700176722821517351), old full-suite fvoci-ablation-home-marks-f0352d-test-standard (PG7700194463416586279), updated full-suite fvoci-ablation-home-marks-f0352d-test-updated-standard (PG7700211961989640231). Resource proof lists exact container IDs/images/projects. Original benchmark DB is tmpfs-backed: stopping its PG container loses data, so do not stop. Original benchmark private run-env.sh selects originalDB; updated case-env.sh selects updated-standardDB for fixture-writing race tests. Neither runner, fixtureIDs/cookies nor binaries are archive allowlisted. Actual standard DB names and CREATE/DROP provenance are fully recorded in standard-server-before/after.json and standard-ddl-audit.json in each evidence directory; four scratch DBs per standard suite retained. No other worker resources/Cloud/external tests used.

Safe updated evidence allowlist and SHA256 are archive-files.txt and archive-sha256.txt; baseline allowlists remain unchanged. Everything stays under target until coordinator archives selected evidence. Raw harness source and prebuilt binary remain local for audit; no new framework/dependency added. Current tracked worktree is clean. Worker ends after this evidence handoff; final combined-runtime gate remains coordinator-owned.
