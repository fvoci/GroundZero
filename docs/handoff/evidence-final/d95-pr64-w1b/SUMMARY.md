# W1b (task_7bb1409f3b42) evidence

Push: 0446746..2acebe9 (8cc4d89 revert, cfae027 recheck-during-move + web notice + race tests, 56e9108 contract/docs format, 2acebe9 CodeRabbit web fixes). GitHub CI on 2acebe9: all success; CodeRabbit review completed, 2 findings fixed and threads resolved.

## Matrix (code cfae027, #89 cherry-picked locally then restored; 56e9108 changes contract text and docs format only)
- cargo xtask test --oidc: 306 passed, 0 failed (45 binaries) - cfae027-oidc.log
- cargo xtask test --supabase --oidc: 317 passed, 0 failed - cfae027-supabaseoidc.log
- cargo xtask test --supabase --session-pooler --oidc: 634 passed, 0 failed (90 runs) - cfae027-supabasesessionpooleroidc.log
- cargo xtask check (licenses included) on 56e9108: all PASS - check-56e9108.log (check-cfae027.log: openapi stale + prettier, fixed in 56e9108)
- web i18n/eslint/svelte-check/vitest: 79 files, 394 tests PASS - web.log
- Earlier revert matrix on 8cc4d89: 301/312/624 passed (8cc4d89-*.log)

## Before-fix records
- prerevert-0446746-room.log: reverted stand-in behaviour test fails on 0446746 (409 invalid_state).
- observe-races-1.log: before B, an edit sent after a demotion committed during the move's lock wait was saved and acked (closed 4409).
- prefix-B-recheck.log: the two in-wait recheck tests fail on HEAD room.rs (no 4403 while waiting; move ran into lock_timeout 409).
- prefix-web-unsaved-notice.log: web test fails before the notice (draft kept, no "not saved" notice).
- fixB-run1.log: MEASURED demotion response -> 4403 close 0.90 s / 0.83 s (recheck interval 1 s). search.rs failures there are the known Meilisearch OOM without #89.

## Race table (in-room move)
| scenario | received | acked | server | client | expected | result |
|---|---|---|---|---|---|---|
| pre-wait edit + session end during wait | before wait | yes | in moved revision | (acked) | kept | PASS |
| edit after session end | during wait | no | not applied, 4403 | draft + notice | dropped, 4403 | PASS |
| pre-wait edit + demotion/removal during wait | before wait | yes | in revision | (acked) | kept | PASS |
| edit after demotion, within recheck interval (3600 s) | after demotion commit, before move commit | yes (after revision) | in moved doc, 4409 | (acked) | accepted within documented delay | PASS |
| edit after the next recheck (1 s) | after 4403 | no | not applied (4403 at 0.90/0.83 s measured) | draft + notice | dropped | PASS (before fix FAIL) |
| removal during wait | before move commit | - | move 409 (preview changed), doc unmoved | - | not moved | PASS |
| closing room + restore | before last leave | yes | revision 3 by writer, restore 409 conflict | (acked) | no loss | PASS |
| closing room + move | before last leave | yes | in moved doc, 200 | (acked) | no loss | PASS |

Bound (code): window after a revocation commit <= recheck interval + one recheck pass; recheck queries are STABLE plain reads (no row-lock waits); pool acquire failure / DB error -> Unknown -> not closed (bound broken for that tick).
Worst case of an in-room move (calculated): queue wait (unbounded) + checkpoint <= 5 s + recheck/pool acquire (<= 5 s each) + lock waits <= 10 s x (7 + L) + non-lock statements (unbounded) + post-commit revision <= 5 s + persist (unbounded).

## CodeRabbit (review 5609387577 on 56e9108)
- 4344648237 (major): notice shown even when the draft could not be stored -> fixed in 2acebe9 (keepDraft returns success; "not kept" alert; no auto reopen). Resolved.
- 4344648246 (minor): choosing an earlier draft cleared the notice of a later kept draft -> fixed in 2acebe9. Resolved.
- prefix-coderabbit-web.log: both new tests fail on 56e9108; web2.log: 79 files / 396 tests pass on 2acebe9; check-cr.log + licenses.log PASS.
