# deps-rust: Rust/Cargo dependency refresh report

- Base: 22ad80ee205332e3ef1ffd8f64f783789955921b. Commits: fbdd7ca (WIP, kept per coordinator; lacks Co-Authored-By trailer) + b7a79c9 (final). Not pushed, no PR.
- Worker: Claude Code, Opus 5.5 (claude-opus-5-5). No discrepancy.
- Files: rust-toolchain.toml, Cargo.lock, crates/server/Cargo.toml, xtask/Cargo.toml, crates/server/src/features/accounts.rs, crates/server/src/storage/s3.rs, xtask/src/notices.rs. Reserved ablation files (search_text.rs, documents.rs, http/cache.rs) are untouched.

## Versions (before -> after)
| item | before | after | note |
|---|---|---|---|
| toolchain | 1.99.0 | 1.99.1 | official stable 2026-10-15 (x86_64 FFI bool miscompile fix). MSRV rust-version "1.99" is unchanged. |
| tokio | 1.53.3 | 1.54.1 | lock |
| uuid | 1.28.0 | 1.29.0 | lock |
| zip | 9.0.4 | 9.0.5 | lock |
| arc-swap, cc, crc32fast, find-msvc-tools, libc, litrs, redox_syscall 0.9, smallvec, typed-path | | patch | lock |
| axum-extra | 0.10.3 | 0.12.6 | cookie only; axum 0.8.9 compatible |
| base64 (direct) | 0.22.1 | 0.23.1 | declared with default-features=false + std. The unified set still has simd-unsafe on, via hyper-util's defaults. |
| hmac / sha2 | 0.12.1 / 0.10.9 | 0.13.0 / 0.11.1 | digest 0.11. Edits: `KeyInit` import (s3.rs). xtask hex helper replaces `{:x}` (notices.rs). |
| rand | 0.9.5 | 0.10.3 | `RngCore` -> `Rng` import (accounts.rs) |
| tower-http | 0.6.11 | 0.7.1 | ServeDir now sends a strong ETag and Vary, and returns 404 for a file path with a trailing slash. Cache/static tests pass. |
| tokio-tungstenite (dev) | 0.28.0 | 0.30.0 | test client only |

New duplicates (bans multiple-versions = warn, non-blocking): block-buffer, const-oid, crypto-common, digest, hmac, sha2 (0.10 stays via openidconnect/oauth2/rsa) and tower-http 0.6.11 (via reqwest 0.12) + 0.7.1. They go away only when openidconnect and reqwest move to newer versions.

## Holds / retentions
- unicode-normalization held at 0.1.25. 0.1.26 changed its license to "(MIT OR Apache-2.0) AND Unicode-3.0", so cargo deny licenses FAILS (not approved, D-72).
- icu_* 2.3.x, tinystr 0.8.4, yoke 0.8.3, zerovec 0.11.8, zerovec-derive 0.11.6 held at the approved Unicode-3.0 exception pins. Exceptions not re-pinned.
- sqlx 0.8.6 (0.9.0 available): query*() takes `impl SqlSafeStr` (~283 dynamic-SQL call sites), and the Migrate trait / _sqlx_migrations changes interact with the baseline migration. Redesign scale.
- reqwest 0.12.28 (0.13.5): openidconnect 4.0.1's AsyncHttpClient is reqwest 0.12, and our Client is passed in (accounts.rs:107,386).
- utoipa 5.5.0 (6.0.0): the generated OpenAPI still declares 3.1.0 but drops the REQUIRED Response.description in 563 responses. Otherwise it is equivalent (only oneOf null ordering changes; TS diff is 50 lines of union reordering). Evidence: target/deps-refresh/openapi-utoipa6.diff, schema-ts-utoipa6.diff, n5.json/n6.json.
- openidconnect 4.0.1 is the latest release (2025-07-06).

## RUSTSEC-2026-0387
Path unchanged: crypto-bigint 0.5.5 <- elliptic-curve 0.13.8 <- ecdsa/p256 0.13.2/p384 0.13.1/primeorder <- openidconnect 4.0.1. The advisory lists patched >=0.7.6 only (0.7.6 released 2026-10-23), and there is no openidconnect release. Exception kept verbatim (2026-10-31 review, #115). The no-ignore advisories run still FAILS (exit 1, RUSTSEC-2026-0387). The excepted run PASSES. No new security issue found.

## rhwp (assessment only, D-76)
- The pinned deploy/rhwp/Cargo.lock passes on a manually reproduced audit graph: cargo deny advisories/bans/licenses/sources PASS with a fresh DB.
- A full update would move wasm-bindgen 0.2.127 -> 0.2.129, which is coupled to wasm-bindgen-cli in app.Dockerfile, inputs.env and build.sh. Upstream v0.8.7 is out of scope (pinned commit).
- cargo-deny 0.20.2 is still the latest. wasm-bindgen-cli latest is 0.2.129. No change.

## Required edits for integration (not mine)
- deploy/docker/dev.Dockerfile:5 and deploy/docker/app.Dockerfile:4: rust:1.99.1-bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8 (multi-arch index; the 1.99.0 index digest matches the current pin). The rhwp-build stage is unchanged. CI base image / CI_BASE_IMAGE_DIGEST needs a rebuild.
- README.md:13: Rust 1.99.1.
- THIRD_PARTY_NOTICES.txt: regenerate with `cargo xtask notices` after the Node changes (Rust crates 250 -> 260). Local copy: target/deps-refresh/THIRD_PARTY_NOTICES.generated.txt (sha256 534391a3b4db944883298fac7779e9c458681f6e62abfee1ee67971744ec5001). **b7a79c9 as committed FAILS `cargo xtask check` at "third-party notices up to date" until this lands.**

## Verification (tree = b7a79c9 + locally regenerated THIRD_PARTY_NOTICES.txt, rustc 1.99.1, CARGO_BUILD_JOBS=8)
- pnpm install --frozen-lockfile: PASS
- cargo xtask check (incl. licenses): PASS, all 13 steps. Rust unit 98 passed / 0 failed (3 suites). Web vitest 766 passed (138 files).
- cargo deny advisories bans licenses sources: PASS (17 duplicate warnings)
- cargo xtask test --oidc --keep: PASS, 381 passed / 0 failed / 48 suites (oidc 5, s3 6, cache 3, collab 42). Supabase NOT RUN in this invocation.
- cargo xtask test --supabase --session-pooler --oidc --keep: PASS, 786 passed / 0 failed / 96 suites (direct + Supavisor session)
- NOT RUN: cargo xtask rhwp / rhwp --audit via xtask (Docker; inputs unchanged), release image build, aarch64/darwin compile (deny graph only).
- Kept compose projects (not torn down, per the no-deletion rule): fvoci-deps-rust-cf3466-test, fvoci-deps-rust-cf3466-supabase.
- Raw logs: target/deps-refresh/*.txt

## Official sources
- Rust 1.99.1: https://github.com/rust-lang/rust/blob/stable/RELEASES.md#version-1991-2026-10-15
- sqlx: https://github.com/launchbadge/sqlx/blob/main/CHANGELOG.md
- utoipa: https://github.com/juhaku/utoipa/blob/master/utoipa/CHANGELOG.md and utoipa-gen/CHANGELOG.md
- OpenAPI 3.1.0 Response.description REQUIRED: https://spec.openapis.org/oas/v3.1.0#response-object
- tower-http: https://github.com/tower-rs/tower-http/blob/main/tower-http/CHANGELOG.md
- axum-extra: https://github.com/tokio-rs/axum/blob/main/axum-extra/CHANGELOG.md
- rand: https://github.com/rust-random/rand/blob/master/CHANGELOG.md
- base64: https://github.com/marshallpierce/rust-base64/blob/master/RELEASE-NOTES.md
- hmac: https://github.com/RustCrypto/MACs/blob/master/hmac/CHANGELOG.md, sha2: https://github.com/RustCrypto/hashes/blob/master/sha2/CHANGELOG.md
- tokio-tungstenite: https://github.com/snapview/tokio-tungstenite/blob/master/CHANGELOG.md
- RUSTSEC-2026-0387: https://rustsec.org/advisories/RUSTSEC-2026-0387
