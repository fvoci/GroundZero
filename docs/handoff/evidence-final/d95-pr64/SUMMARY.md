# W1 D-95 / PR #64 — evidence

Branch `fix/audit-move-live-snapshot` pushed fast-forward `5edd8f4..0446746`.

## Commits (on 5edd8f4)
- e564f03 feat(collab): record the actual editors of a revision saved on their behalf (D-95) — staged WIP 0fce62b content, squashed
- 0fb6b04 fix(collab): bound the move's lock wait and recheck access around it (#64 review) — coordinator edits + tests + has_room
- 6b22bd7 fix(collab): save a live state left without a room before restoring (D-95)
- 132777f fix(collab): checkpoint a pending state inherited by a reopened room (D-95 review) — adversarial review fixes
- fb3a11e docs(collab): record the D-95 re-review's remaining low items
- 0446746 fix(api): declare the restore 409 as a stale base or an error body (CodeRabbit)

## Commands
| Command | Code | Result | Log |
|---|---|---|---|
| `cargo xtask check` (incl. licenses) | 0446746 tree | PASS | check-3.log |
| `cargo xtask licenses` | 132777f tree | PASS | licenses.log |
| web: i18n, eslint, svelte-check, vitest (78 files / 392) | e564f03 web | PASS | web-checks.log |
| `cargo xtask test --oidc` (+ local uncommitted `--no-fail-fast`) | 132777f | 297 passed, 3 failed (search.rs only) | nff-132777f-oidc.log |
| `cargo xtask test --supabase --oidc` (same) | 132777f | 308 passed, 3 failed (search.rs only) | nff-132777f-supabaseoidc.log |
| `cargo xtask test --supabase --session-pooler --oidc` (same) | 132777f | 309 passed, 2 failed (search.rs only) | nff-132777f-supabasesessionpooleroidc.log |
| `cargo xtask test --oidc` | 132777f | collab 36/36 | fixed-test-oidc-3.log |
| `cargo xtask test` on base 5edd8f4 | 5edd8f4 | search.rs fails too (environment) | base-5edd8f4-test.log |

search.rs: test Meilisearch (512m, tmpfs) becomes unreachable late in the run — environment issue, also on main/base; fix in progress elsewhere (coordinator: W8, fix/test-meili-memory). Not caused by this branch.

## Fails before the fix
- prefix-test.log (WIP e564f03 server + has_room, new tests): lock-limit move x2 hang, session-ended move 4409, demoted editor 4409, roomless restore 200 over a pending edit.
- prefix3-test.log (6b22bd7 server, new tests): reopened-room restore 200 over an acknowledged edit; restore lock wait hangs.
- prefix2-test.log: earlier attempt where the reopened-room test had a setup bug (closed before demoting); superseded by prefix3.

## Review
- Adversarial review 1 (code only): 1 blocking (a room reopened on a pending state skipped the checkpoint) + low items → fixed in 132777f.
- Adversarial review 2 on 132777f: no blocking; 2 low items documented in docs/development.md (fb3a11e).
- CodeRabbit: r4343460648 resolved by bot after 0fb6b04; summary concern answered; outside-diff finding at fb3a11e fixed in 0446746; Merge Risk Low up to 0446746; retained security concern (revoked-write window before commit) answered as documented residual risk.

## Links
- PR: https://github.com/fvoci/GroundZero/pull/64 (body updated: Closes #71, D-95 summary, tests, remaining risks)
- Reply to CodeRabbit r4343460648 (lock timeout): https://github.com/fvoci/GroundZero/pull/64#discussion_r4344167425 (bot resolved the thread)
- Reply to summary concern (recheck during lock wait): https://github.com/fvoci/GroundZero/pull/64#issuecomment-6331707884
- Reply to fb3a11e outside-diff finding (restore 409 contract, fixed 0446746): https://github.com/fvoci/GroundZero/pull/64#issuecomment-6331937772
- Reply to retained security concern at 0446746 (documented residual window): https://github.com/fvoci/GroundZero/pull/64#issuecomment-6332148763

The integration matrix ran at 132777f. fb3a11e (docs only) and 0446746 (OpenAPI declaration + contract unit test only) were verified with `cargo xtask check` PASS at 0446746.

## Interpretation choices to confirm
- In-room restore: a connected restorer no longer saves the checkpoint on behalf of editors unless they edited since the last revision (matches the roomless rule and the handoff 2.2 "same as in-room restore" wording; behaviour change versus main). Other connected writers may still stand in (recorded in edited_by).
- Restore handler refuses non-writers itself (404 no access / 403 read-only, same codes as before; revisions.rs tests pass) and Rooms::reset (close without saving) was removed.

## Not run / remaining
- search.rs green: NOT RUN pending the Meilisearch memory fix.
- Real browser check of the history/export labels: NOT RUN (component tests only).
- Residual: write access revoked between the pre-move recheck and the commit (lock wait, ≤10s per lock) with a live session — edits are accepted; documented.
- Low (documented): reader-only reopened room with no writable editor retries the idle save every 3s; after-commit persist lock wait unbounded; ensure_loaded race has no deterministic test.
