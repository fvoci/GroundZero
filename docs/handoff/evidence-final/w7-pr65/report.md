# W7 #65: a late local-store attempt that lost the claim can no longer overwrite the final object

Branch `fix/audit-upload-claim-release` (fast-forward push, base 5400977)

## Commits
- 6e6a860 fix(files): publish a locally assembled upload only while holding the completion claim (#65)
- b9f5c22 fix(storage): treat parts removed while staging as already published (review)

## Change
- The storage layer now exposes `Storage::assemble` / `digest_assembled` / `publish` / `Assembled::discard`. `complete` (used by backup restore) is assemble + publish.
- Local store:
  - `stage` joins the parts into a per-attempt `uploads/<id>/staged-<uuid7>` file. It does not touch the final key.
  - The digest is computed from the open handle of that staged file.
  - `publish` renames the staged file to the final key and then removes the parts folder (a removal failure is logged as `storage.upload_dir_left`).
- S3: `assemble` still runs `CompleteMultipartUpload`, unchanged. `publish` and `discard` do nothing for S3.
- `features/files.rs` `finish`: assemble (`NotFound` → `Published`) → `record`:
  1. Digest the staged bytes and check the size (failure → `fail`).
  2. Open a transaction and run `hold_claim` (row lock).
  3. If the upload is already `done`, return Ok without publishing.
  4. Otherwise `publish`, then `app_finish_upload`, then commit.
  5. Discard the staged file at the end, whatever happened.
- If recording fails after publishing:
  - Transient error: the claim is released, and the next attempt checks the final key and records it.
  - Lasting error: `app_fail_upload` queues the key for deletion.
- No migration changes (0088 untouched). `docs/development.md` section 31 has one added sentence.

## Tests
- New in files.rs: `a_late_attempt_cannot_overwrite_what_the_take_over_stored` (FIFO stall, take-over, reset, different bytes, then the stalled attempt is released).
  - Before the fix (5400977): FAIL, stored 97a8c5… != recorded 9e2ec6… (`before-fix-xtask-test.log`).
  - After the fix: ok.
- New in storage.rs:
  - `local_staging_leaves_the_object_alone_until_published`
  - `local_staging_sees_parts_removed_midway_as_already_published`: fails without b9f5c22 (`review-fix-before-storage-local-direct.log`), passes with it.
  - `local_complete_failure_leaves_no_temp_file` was extended to check for leftover staged files.

## Commands (logs in this folder, local paths removed)
| command | head | result |
|---|---|---|
| cargo xtask test (pre-fix, new test added) | 5400977+test | FAIL as expected: only the new test failed in files.rs |
| cargo xtask check (full, licenses included) | 6e6a860 | PASS (after-fix-xtask-check-full.log) |
| cargo xtask check (full) | b9f5c22 | PASS (review-fix-xtask-check.log) |
| cargo xtask test --oidc | 6e6a860 (×2), b9f5c22 | FAIL only in search.rs (Meilisearch OOM, known on main, coordinator msg_0793478c9ef5). 29 binaries before it pass, including files.rs, backup.rs, oidc |
| cargo xtask test --supabase --oidc | 6e6a860, b9f5c22 | same: files.rs ok, only search.rs fails |
| cargo test -p fvoci-server --test storage local_ | b9f5c22 | PASS 4/4 (review-fix-storage-local-direct.log) |
| cargo xtask check --skip-licenses (first run) | uncommitted | FAIL on fmt, fixed by cargo fmt before the commit; cargo-deny was missing then (now installed) |

NOT RUN: the xtask-run integration binaries alphabetically after search.rs (sharing, shutdown, storage, study, tasks, templates, time_tracking, trash, tree, worker, workspace_delete, zotero). Cargo stops at the first failing binary. The `storage.rs` `local_*` tests were run directly and pass; the `s3_*` tests in storage.rs did not run. The S3 completion path is covered by files.rs `s3_files_round_trip_and_purge_with_the_document` (ok).

## CodeRabbit
- Reply to the summary concern (staging + guarded publication): https://github.com/fvoci/GroundZero/pull/65#issuecomment-6331631925
- Re-review of 6e6a860: one Minor finding (ENOENT while staging should be NotFound). Fixed in b9f5c22, reply https://github.com/fvoci/GroundZero/pull/65#discussion_r4344197221, and CodeRabbit confirmed the fix.
- Re-review of b9f5c22: no actionable comments; retained concerns: none. Hardening proposal (make removing the parts folder after publish retryable) declined with reasons from the current code: https://github.com/fvoci/GroundZero/pull/65#issuecomment-6331925703

## Remaining
- If `remove_dir_all` fails after a successful publish, the parts folder is left with no cleanup owner. This is logged only. A follow-up would need a DB change.
- Full integration coverage after search.rs depends on W8's Meilisearch fix.
