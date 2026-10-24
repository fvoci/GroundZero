# deps-integration handoff (task_6d2293f4a007 / ctx_6c78db5dfa4f)

Model: claude-opus-5-5. No delegation.

## PR
- https://github.com/fvoci/GroundZero/pull/123, base main 22ad80e, final head 00c81dc0f4cf341b0e03ce2f9aa15b459f076ef7.
  The coordinator merged it as merge commit e0103ef5fbcb818a66d2156cc7c742d51ea321fc.
- Commits:
  - 5ca68a0 / 312d016: Rust, cherry-picked with -x from fbdd7ca / b7a79c9.
  - 2a95372 / a86c151: Node, cherry-picked with -x from 691291c / 3792924.
  - e4d8290: shared Docker / CI / README changes.
  - b69df07: NOTICE regenerated.
  - 214f1b5 and 41a05e3: release-image PR paths now include the dependency and toolchain inputs (CodeRabbit findings).
  - 00c81dc: smoke fix with a fresh PostgreSQL. The bug was already on main: the baseline check from 2e8b58b.

## Results (local runs on b69df07; product bytes are the same at 00c81dc)

| Check | Result |
| --- | --- |
| Host `cargo xtask check` | PASS: 98 Rust unit, 138 files / 766 vitest, deny, openapi, NOTICE, licenses |
| Local dev.Dockerfile image as non-root, `COREPACK_ENABLE_NETWORK=0`: `check --skip-licenses` + `licenses` | PASS. Versions: rustc 1.99.1, node 24.22.1, corepack 0.36.0, pnpm 12.15.0 |
| `cargo xtask test --keep` | 375 passed / 0 failed; 18 tests filtered out by design (OIDC and Supabase, run below) |
| `--oidc --keep` | 381 passed / 0 failed |
| `--supabase --session-pooler --oidc --keep` | direct 393 / 0, session 393 / 0 |
| `pnpm audit` | 0 |
| Web bundle (JS) | 2,433,584 → 2,423,675 B; CSS unchanged |
| `cargo deny` without the RUSTSEC-2026-0387 ignore | FAILED, as expected. With the approved exception: ok. #115 is kept. |
| Browser (headless Chromium 156, own dev stack, Keycloak on 8181) | login, note save/reopen, two-user collab, kanban drag, Gantt, calendars, 390×844: all PASS |
| Local product image `app.Dockerfile` (4 CPUs) | PASS in 28m25s |
| Exact SMOKE script, sha256 4590a5c3… | positive PASS; port-sentinel and name-collision cleanup cases behave correctly; migrate and unreachable-DB guards both refuse to start |
| CI on 00c81dc | run 40092901643: all 6 jobs PASS. run 40092901633: native amd64 and arm64 smoke PASS, plus guard-check |
| CodeRabbit | 3 threads resolved. The review on 00c81dc has no actionable comments. Follow-up tracked in #125. |

## Owned resources (all kept; nothing deleted)

Compose projects:
- fvoci-deps-integration-5fcf21-test (postgres, meilisearch, seaweedfs, keycloak). Network `_test_default`; its postgres runs on tmpfs.
- fvoci-deps-integration-5fcf21-supabase (db, rest, supavisor). Network `_supabase_default`, volume `_supabase_db-config`.
- fvoci-deps-integration-5fcf21-dev (postgres, meilisearch, keycloak on 127.0.0.1:8181). Network `_dev_default`, volumes `_dev_pgdata`, `_dev_meilidata`, `_dev_kcdata`. Synthetic browser data lives here:
  - one test-user note
  - two collab lines in 공동 편집 연습
  - DEVP-5 moved to 진행 중

Local images (not pushed): fvoci-ci-base:deps-integration (483947877ca9), groundzero:pr-local (4c848772e2d5).

Processes: none running.
- The dev server (target/debug/fvoci-server) and Vite were stopped with SIGINT and logged a clean shutdown.
- `cargo xtask dev` exited when it received SIGINT.
- I stopped them before reading the "do not close services" message; containers and volumes are untouched.

Temporary smoke and guard resources were removed by captured ID after a label check:
- fvoci-smoke-linux-amd64-local-0-{2244756,2254250}
- deps-int-guard-2265843
- sentinel container 3ab3e535…
- sentinel network 8896c122…

## Files to archive (all in target/deps-integration, which git ignores)
- Plans and notes: prep-notes.md, commands.md, handoff.md, pr-body.md
- Build and check logs: host-check.log, container-check.log, dev-image-build*.log, app-image-build.log, notices.log, host-install.log, web-build-{head,main}.log
- Test logs: test-plain.log, test-oidc.log, test-supabase.log
- Smoke and advisory evidence: smoke.sh, smoke-*.log, deny-no-0387.{toml,log}
- CI logs: ci-licenses.log, ci-prbuild-amd64.log
- Browser: browser/lib.cjs, browser/s*.cjs, browser/explore.cjs, browser/shots/01..20.png
- Scratch copies: container-src/ and main-src/ are full clones; devctx/; corepack-bin/ (a local shim directory)
