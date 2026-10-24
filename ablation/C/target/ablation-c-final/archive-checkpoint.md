# Ablation C finalization — in progress

Draft PR: https://github.com/fvoci/GroundZero/pull/121

Minimal source commit: `51b240b5a392c24808d0198c4e511b0a6ef2f850`, parent `22ad80ee205332e3ef1ffd8f64f783789955921b`. One file only (`crates/server/src/http/cache.rs`), 47 additions / 2 removals; no raw logs or harness committed. Branch `fvoci/ablation-etag` was pushed normally. No Ready, merge, force push or history rewrite performed by this worker.

The original fixed-baseline experiment is preserved separately under `target/ablation-c`, with its report, raw accepted runs 3/4, excluded contaminated runs 1/2, source equivalence checks and SHA-256 manifest. Its accepted interval is 2026-10-24 10:02:55–10:03:04 UTC. The ~551ns → ~35ns hex-only result is a shared-host microbenchmark; the 1MiB total gain was within noise. Those measurements must not be relabeled as updated-dependency evidence.

## Baseline PR validation

Local source checks, now committed as `51b240b`: cache unit tests 6/6, cache API tests 3/3, standard isolated integration 376 passed / 0 failed, Clippy with warnings denied, rustfmt and diff checks all PASS. See the original report for commands and hashes.

GitHub Actions run https://github.com/fvoci/GroundZero/actions/runs/40085545718 at this head: **PASS**, all six jobs:

- check (`cargo xtask check --skip-licenses`)
- licenses (`cargo xtask licenses`)
- integration (standard external test services)
- oidc (PG18 / Meilisearch / Keycloak)
- supabase (`cargo xtask test --supabase --session-pooler --oidc`, direct and session-pooler)
- rhwp (build, notice comparison and audit)

This CI evidence supplements the original local NOT RUN entries; it does not retroactively change what ran locally.

## Review

Explicit request: https://github.com/fvoci/GroundZero/pull/121#issuecomment-6383039343

CodeRabbit completed automatic review of the full one-file diff from `22ad80e` through `51b240b`, run `d9c85d95-9887-4bc7-a0d0-741eeb5d7729`. Summary coverage explicitly marks `51b240b` as reviewed, one file processed, no actionable comments. There are no inline review threads and no outside-diff findings. Formal review-submission list is empty: the bot published summary-based coverage, which is distinct from a formal approval review.

Summary: https://github.com/fvoci/GroundZero/pull/121#issuecomment-6383043100

Two nonblocking tool/pre-merge warnings were inspected and addressed in one evidence reply: https://github.com/fvoci/GroundZero/pull/121#issuecomment-6383158693

- The 25% docstring coverage warning counts four functions; the public `strong_etag` contract already has Rust documentation, and test names/comments describe the remaining assertions. The repository has no 80% docstring requirement. No documentation-only source expansion or rule relaxation was made.
- The bot's Clippy tool invocation failed to execute. This is not a successful bot tool run. Actual local Clippy with `-D warnings` and the complete CI check job passed.

No valid source finding required a fix. The manual review command also received an already-reviewed response, so no redundant full review was requested at the same head.

## Resource preservation

Own retained project: `fvoci-ablation-etag-bf3b8f-test`, created by the earlier C standard test invocation under Orca run `run_fa40a3b51085`, original task `task_1a76e81bd4b2` / dispatch `ctx_809d9193a46e`, in this same worktree. Current finalization task is `task_3d0f0ca46eb1` / active retry dispatch `ctx_3eddc0cc153d`. PostgreSQL 17 host endpoint at that run: `127.0.0.1:54953`; Meilisearch and SeaweedFS are also isolated. Original `retained-services.jsonl` records container identifiers and mappings. No manual DB cleanup or Compose down/volume deletion occurred here. The worktree, source and target evidence remain preserved for coordinator archival and normal Orca cleanup after final validation.

`--keep` retains service containers; it does not disable normal fixture-owned temporary database cleanup. Source inspection confirms fresh UUIDv7 full names, successful CREATE before returning fixtures, and the same returned name/admin connection at DROP in backup/search/migration fixtures. Existing helpers do not record full runtime names or orchestration run IDs; this evidence limit was reported to the coordinator. No uncertain/reused/other-worker/Cloud databases, volumes or backups will be manually deleted.

## Pending before completion

Await coordinator merge of the dependency PR, then merge `origin/main` into this branch without rewriting history. Record final SHA and lock/toolchain hashes, run updated standard checks and relevant API tests at no more than two build jobs, build a separate updated-sha2 harness, and wait for an explicit quiet slot before timing. Preserve original measurements separately. Recheck final-head CI and CodeRabbit coverage, then hand the verified Draft PR to the coordinator for Ready/merge decisions.
