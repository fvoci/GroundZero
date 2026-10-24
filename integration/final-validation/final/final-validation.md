# Final combined validation (task_9a7166336220 / ctx_74104eb920ec; Node 24.22.1 browser follow-up task_5ea012c1076c / ctx_3bfbc52345ac)

Worktree: fvoci/deps-integration, fast-forwarded with `git merge --ff-only`. Nothing was pushed. The source tree is clean (0 changed files).

SHAs:
- Final all-code main is 479b562522f3b2fa1696e6c5828c753fc91124fd (PRs #123, #120, #122, #121, #124).
- e9b0f2c22e499c4d4f7126d680e214cdf0a51240 adds PR #126. It changes only crates/server/tests/home_marks.rs (+25), so production bytes are the same as 479b562.

Caps used: CARGO_BUILD_JOBS=2, RUST_TEST_THREADS=2 (xtask uses 4 for its Supabase runs), VITEST_MAX_WORKERS=2.

Host tools: rustc 1.99.1, cargo-deny 0.20.2, corepack 0.36.0, pnpm 12.15.0 through a Corepack shim under target/. Host Node is v24.21.0; Node 24.22.1 was checked in the container stage.

## Results

### At 479b562

| Command / stage | Result |
| --- | --- |
| `pnpm install --frozen-lockfile` | PASS |
| `cargo xtask check` (host) | PASS in 5m22s. Rust unit tests: 101 passed, 0 failed. Vitest: 139 files, 798 tests. Also passed: deny advisories/bans/sources, openapi.json + schema.ts, NOTICE, rhwp notices, cargo deny licenses, pnpm licenses. |
| Container `ghcr.io/fvoci/groundzero-ci-base@sha256:055178ad…` (revision e0103ef). Clean clone, non-root, `COREPACK_ENABLE_NETWORK=0`. Ran `pnpm install --frozen-lockfile`, `cargo xtask check --skip-licenses`, `cargo xtask licenses`. | PASS in 13m13s. Tool versions: rustc 1.99.1, cargo-deny 0.20.2, node v24.22.1, corepack 0.36.0, pnpm 12.15.0 (Corepack shim, no runtime download). Rust unit 101/0, vitest 798/798, svelte-check 0/0, licenses ok. |
| `cargo xtask openapi && pnpm run generate:api` | PASS. 0 changed files. |
| `pnpm --filter @fvoci/web run build` | PASS. JS 2,423,787 B, CSS 120,349 B. |
| `pnpm audit` | No known vulnerabilities. |
| RUSTSEC-2026-0387 | The path crypto-bigint 0.5.5 ← elliptic-curve 0.13.8 ← p256/p384 ← openidconnect 4.0.1 is unchanged. deny.toml is unchanged since e0103ef: one entry, review date 2026-10-31. With the ignore removed (temp config), `cargo deny check advisories` FAILED on 0387 as expected. No new ignores. |

### At e9b0f2c

| Command / stage | Result |
| --- | --- |
| `cargo xtask test --keep` | PASS. 50 binaries, 381 passed / 0 failed. 18 filtered: the oidc_ and supabase_ tests, NOT RUN in this run (they run below). home_marks 1/1, unicode_boundaries 2/2. |
| `cargo xtask test --oidc --keep` | PASS. 387 / 0. 12 filtered (supabase_). |
| `cargo xtask test --supabase --session-pooler --oidc --keep`, run 1 | Direct connection PASS: 399 / 0. Session pooler FAILED: 88 passed, then `audit::workspace_audit_is_for_admins_and_personal_audit_is_private` failed with 28P01 "password authentication failed for user fvoci_app", details below. |
| Same command, run 2 | PASS. Direct 399 / 0 and session 399 / 0, 50 binaries each, 4m59s. |
| Browser run 1. Superseded as Node evidence: the Vite and script runtime was Node v24.21.0, at `/proc/641974/exe` = nvm v24.21.0. Kept separately in browser/shots. | All PASS, and each image was opened and checked:<br>• login<br>• personal note: Korean text and list typed, saved ("저장됨", Revision 2), reopened in a fresh context with content intact<br>• collab: both users "실시간 연결됨", edits visible to each other in 218 ms and 205 ms, remote caret shown<br>• kanban: DEVP-5 dragged from 진행 중 back to 할 일, still there after reload (this also undoes the earlier synthetic move)<br>• Gantt: bars, dependency arrows, month/week/day headers<br>• item and workspace calendars<br>• home view<br>• 390×844 item and collab doc: no horizontal overflow (narrow viewport, not a real device) |

### Supabase session failure in run 1

The first run FAILED and the second standard run PASSED (session 399/399). Both results stay recorded.

What was observed:
- The run was on the kept stack (`--keep`), so the xtask ran `db bootstrap` again, which resets the fvoci_app password.
- The Supavisor log shows "Auth error 28P01" at 14:05:49, "Secrets not available" right after, and "Secrets changed or not present, updating cache" at 14:06:04.
- The same test binary through the same pooler, without bootstrapping again, passed 3/3 (rerun-audit-session-*.log).

This evidence fits a stale credential cache being refreshed when a kept stack is reused. It is not an exhaustive controlled proof: there was no isolated reproduction and the cache state was not instrumented.

Follow-up: #127 (verify auth readiness when reusing a kept Supavisor). The product code involved is unchanged from the e9b0f2c/479b562 main, and main CI passed on fresh stacks.

## Browser rerun on Node 24.22.1 (follow-up task_5ea012c1076c)

Setup:
- SHA: e9b0f2c22e499c4d4f7126d680e214cdf0a51240.
- The old owned processes (xtask 636807, fvoci-server 638998, sh 639070, Vite 641974) were checked by PID, exe, cwd and parent, then stopped with SIGINT. The server logged "shutdown complete".
- The stack was restarted with `PATH=<target corepack-bin-24.22.1>:~/.nvm/versions/node/v24.22.1/bin:$PATH`. The Corepack shim was created with v24.22.1's corepack, using `--install-directory` under target/ only. Nothing was installed and no global settings changed.
- Tools: node v24.22.1, corepack 0.36.0, pnpm 12.15.0. Command: `FVOCI_DEV_KEYCLOAK_PORT=8181 CARGO_BUILD_JOBS=2 cargo xtask dev --oidc`, logged to dev-node24.22.1.log.

Runtime identity (runtime-processes-node24.22.1.txt):

| Process | PID | exe |
| --- | --- | --- |
| xtask | 725109 | target/debug/xtask |
| fvoci-server on 8080 | 726990 | target/debug/fvoci-server |
| Vite on 5173 | 729819 | /home/kinesis/.nvm/versions/node/v24.22.1/bin/node |

`/healthz`: status ok, database ok. Browser driver: playwright-core 1.64.0 with headless Chromium 156.0.8078.4, scripts run with node v24.22.1 (browser-node24.22.1-runtime.txt). Same database and volumes as before; nothing was reset.

| Flow (browser-node24.22.1/*.cjs) | Result (browser-node24.22.1-results.txt, shots in browser-node24.22.1/shots, all opened and checked) |
| --- | --- |
| Login: test-user and test-user-2 through Keycloak | PASS |
| Note: typed Korean text and a list with the keyboard API → Save → reopened in a fresh context | PASS: “저장됨”, Revision 2, content kept |
| Two-user collab on 공동 편집 연습 | PASS: both on the collab WebSocket and “실시간 연결됨”; u1→u2 in 219 ms, u2→u1 in 206 ms; remote caret shown |
| Kanban: DEVP-5 dragged 할 일 → 진행 중, then back 진행 중 → 할 일, with a reload after each | PASS: persisted both times (15a and 15 shots). Final state is 할 일, the seed state. |
| Gantt | PASS: bars, dependency arrows, month/week/day headers |
| Item calendar / workspace calendar | PASS. Shot 17 shows only the top of the item calendar. At root's request, one image was added, 21-item-calendar-scrolled.png, from s7-item-calendar.cjs: the October 2026 grid scrolled down to the 18–31 rows. The visible entries match the seeded due dates on the kanban cards: 24: DEVP-8, DEVP-4 (done, struck through); 25: DEVP-11; 26: DEVP-2, DEVP-7; 27: DEVP-6 and the ◆ DEVP-5 milestone; 28: DEVP-9; 30: DEVP-10. The first s7 attempt failed in the script itself, because its selector matched a hidden DEVP-5 element, and passed after switching to visible matches only (both recorded in browser-node24.22.1-results.txt). The workspace calendar (18) shows the seeded tasks and events. |
| Home | PASS |
| 390×844 viewport, item and collab doc | PASS: scrollWidth == innerWidth |

Not tested: real devices and OS IME. Text was entered through the Playwright keyboard API.

## Not run
- Supabase Cloud (#113 excluded).
- Real devices.
- No product image rebuild; the dependency PR's image and native smoke already passed, and the coordinator directed no rebuild.

## Resources
These are the same owned projects listed in resources-manifest.md (5fcf21 test/supabase/dev). All are kept, and none were deleted.

All owned dev processes are stopped (see the handoff section). Docker resources are untouched.

The dev DB gained synthetic edits:
- three verification notes for test-user;
- added collab lines in 공동 편집 연습;
- DEVP-5 moved and moved back, so it is in 할 일 again.

## Evidence (target/deps-integration/final)
- Logs: host-*.log, container-check.log, api-regen.log, web-build.log, pnpm-audit.log, test-*.log, rerun-audit-session-*.log, dev.log (Node 24.21 run), dev-node24.22.1.log
- Security: crypto-bigint-path.txt, deny-no-0387.{toml,log}
- Browser, Node 24.22.1 (current): browser-node24.22.1/*.cjs, browser-node24.22.1/shots/*.png (15 images: the original 14 unchanged, plus 21-item-calendar-scrolled.png), browser-node24.22.1-runtime.txt, browser-node24.22.1-results.txt, runtime-processes-node24.22.1.txt, dev-node24.22.1-calendar.log
- Browser, Node 24.21.0 (superseded as Node evidence): browser/*.cjs, browser/shots/*.png (13 images)
- container-src/ is a clean clone of 479b562

## Handoff for root cleanup
- `artifact-allowlist.sha256`: text artifacts that are safe to archive, with sha256. They were scanned for credentialed URLs, literal test passwords, cookies, tokens and env dumps; none were found. The remaining keyword hits are crate names (cookie_store), test names, advisory text, and a pointer to deploy/keycloak/realm-test.json. Excluded: every `lib.cjs` (it holds the test-realm passwords copied from deploy/), the container-src/ and main-src/ clones, the corepack-bin*/ shims, and PR bodies.
- `local-only-images.sha256`: 48 PNG screenshots, grouped by runtime. Keep them local only; never commit or post them.
- `cleanup-resource-manifest.tsv`: the exact owned resources, with full IDs identical to the earlier inventory:
  - 10 containers
  - 3 networks
  - 4 volumes
  - 2 local images
  - 0 processes
- Processes: two Node 24.22.1 runs, each identity-checked and stopped with SIGINT:
  - the flow run: xtask 725109, fvoci-server 726990, Vite 729819;
  - the calendar-capture run: xtask 838102, fvoci-server 840118, Vite 843149 on /home/kinesis/.nvm/versions/node/v24.22.1/bin/node.

  Each server logged "shutdown complete", no PID is left, and 8080/5173 are free. The dev Keycloak container keeps 127.0.0.1:8181.
- Docker: untouched. Nothing was deleted or reset.
- Source tree: clean at e9b0f2c.
- Two stale background wait loops of my own were still polling: an old `gh pr checks 123` loop (pid 1020683) that never finished because gh has no `--json` flag for that command, and a `pgrep` loop (pid 1801545) that kept matching its own command line. Both were stopped with TaskStop. They were read-only and touched no resources.
