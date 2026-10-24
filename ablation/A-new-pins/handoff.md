# Completed A handoff

PR https://github.com/fvoci/GroundZero/pull/122 was merged by the coordinator as `0e02a02802b09824765bc22f9fc43c1eb7359ca8`. Preserved branch: `fvoci/ablation-scout`, local/remote reviewed head `3137374e24ba622d0101111039c00f274f27ea34`; tracked working tree clean. Compiled variant `11b9d9cd05a15259bd0d2867f82ba1b0ccd31a86`, compiled baseline `e0103ef5fbcb818a66d2156cc7c742d51ea321fc`; subsequent Unicode merge changed neither source, pins nor binaries. Effective worker model: GPT-6.1-Sol.

Adoption approved after coordinator source/invariant inspection and independent recomputation of all ten fresh timing cells. Exact 326-fixture equality and six helper tests pass. Updated-pin local full checks pass (100 Rust / 766 web); local isolated integration passes 377 with S3 included. Current reviewed-head CI passes all six jobs, including check (100 Rust / 798 web) and external integration (376 passing / 21 filtered, S3 excluded), plus separate OIDC and Supabase. Full CodeRabbit review covers `3137374` with no actionable body/inline/outside-diff findings.

Fresh 200-invocation AB/BA matrix: all ten gates PASS, many-node gain 95.01–98.75%, tiny cost 2.85–3.86%, single-huge gain 0.26–0.99%, baseline relative MAD 0.39–2.46%. Window 2026-10-24 12:51:32.337560953–12:51:50.664521024 UTC. All 100 compiler/test checkpoints clear; a pre-existing unrelated Python process using about one core was disclosed. Process-cold measures a batch in a new process, not OS cold caches or isolated first-call latency. No endpoint performance claim or manual browser/navigation validation. CodeRabbit's own Clippy 1.99.0 invocation failed, separate from passing local/CI pinned 1.99.1 Clippy.

Evidence: `result.md`, `measurements.tsv`, `summary.json`, `measurement-window.txt`, `prebuilt-hashes.txt`, `ci-current.json`, `review-current.json`. Safe 28-file allowlist `archive/files.txt` and verified `archive/SHA256SUMS` were archived by the coordinator as evidence commit `64e040c`; original evidence remains at `../ablation-a/` and archived `e17de99`. Both sets of original source/fixtures/binaries/build targets remain local. No worker compiler, benchmark or test process was deliberately retained; all commanded runs completed. Six owned service containers are still running: full IDs, exact names and current host bindings are in `resource-handoff.txt`.

| Owned project | PostgreSQL system ID | PostgreSQL / Meili / S3 host ports |
|---|---|---|
| fvoci-ablation-scout-028b53-test | 7700178703633895463 | 54971 / 54969 / 54970 |
| fvoci-ablation-a-final-67dc1df2 | 7700212565316374567 | 54994 / 54995 / 54993 |

Each server retains main `fvoci` and four intentional scratch DBs; both audits prove all 9 fixture DROPs had an earlier same-session CREATE among 13 creates. Exact full DB names and provenance are preserved in each run's `database-ownership.json`. Original storage is retained intact at `../ablation-a/retained-test-files-original`; final storage remains at `../test-files`. No cleanup, shutdown, volume/backup deletion, reset or history rewrite occurred.

The coordinator explicitly owns all remaining combined-main source/pin reconciliation, combined runtime/CI, any brief remeasurement from retained artifacts, final reports and later authorized worktree/resource cleanup. Completion of this worker scope does not claim that broader combined goal is finished.
