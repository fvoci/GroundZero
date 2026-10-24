# deps-node report (branch fvoci/deps-node, base 22ad80e)
Commits: 691291c (frontend/tool refresh), 3792924 (pnpm 12 via Corepack + engineStrict). Not pushed.
Model: Claude Opus 5.5 (claude-opus-5-5); matches the brief, no discrepancy.

## Changed (before -> after)
packageManager pnpm 11.24.0 -> 12.15.0+sha512.b3951d6d…128e61 (corepack use); engines.pnpm >=11 -> >=12
eslint 10.12.0->10.13.0; typescript-eslint 8.71.2->8.72.0; svelte-check 4.7.6->4.7.7; vite 8.3.4->8.3.5
@inlang/paraglide-js 2.29.0->2.29.1; @inlang/plugin-message-format 4.5.4->4.5.5; @lucide/svelte 1.58.0->1.61.0
cn 0.4.0->0.5.0; diff 8.0.4->9.0.0
pnpm-workspace.yaml: drop minimumReleaseAgeExclude lucide@1.58.0; add engineStrict: true. .npmrc: dead settings -> comment.

## Retained
typescript 6.0.3 (7.0.2): typescript-eslint 8.72.0 peer typescript <6.1.0; svelte-check 4.7.7 peer ^5||^6.
katex 0.18.10: 0.18.11 deprecated upstream ("Accidentally published with breaking changes. Use 0.19.0"); 0.19.x outside @tiptap/extension-mathematics 3.32.0 peer ^0.16.4||^0.17||^0.18.
bits-ui 2.19.5 (2.20.2): 2.20.0..2.20.2 fail 6 jsdom component tests; isolated to animations-complete.js (PR huntabyte/bits-ui#2193); browser behavior unverified.
@aiden0z/pptx-renderer 1.3.0 (1.4.0): version-pinned approval D-74 + xtask/src/notices.rs + THIRD_PARTY_NOTICES.
Node 24 LTS (26): Node 26 LTS only from 2026-10-28; Corepack not distributed from Node 25.0.0; Node 24.22.1 bundles corepack 0.36.0.

## Sources
https://github.com/pnpm/pnpm/releases/tag/v12.0.0 ; https://github.com/pnpm/pnpm/releases/tag/v12.15.0 ; https://github.com/pnpm/pnpm/releases/tag/v11.0.0 (.npmrc auth/registry only)
https://github.com/nodejs/Release/blob/main/schedule.json ; https://nodejs.org/en/blog/release/v24.22.1 ; https://github.com/nodejs/node/blob/main/doc/contributing/distribution.md (corepack) ; https://github.com/nodejs/node/blob/v24.22.1/deps/corepack/package.json
https://github.com/kpdecker/jsdiff/blob/master/release-notes.md#900 ; https://github.com/shadcn-ui/cn/releases/tag/cn%400.5.0
https://github.com/huntabyte/bits-ui/releases/tag/bits-ui%402.20.0 ; https://github.com/huntabyte/bits-ui/pull/2193
npm registry: katex@0.18.11 deprecated field; typescript-eslint@8.72.0 / svelte-check@4.7.7 / @tiptap/extension-mathematics@3.32.0 peerDependencies

## Results @3792924 (linux-x64 WSL2, host Node 24.21.0, bundled corepack 0.36.0; darwin/arm64 NOT RUN)
corepack use pnpm@12.15.0: PASS (hash = npm dist.integrity). pnpm config get engineStrict -> true.
Clean frozen install (pnpm 12.15.0): PASS. format:check, lint, svelte-check (3258 files 0/0), api-client tsc: PASS.
vitest 138 files 766/766 PASS; web build PASS; generate:api no diff PASS; pnpm audit 0 vulns (487 deps).
cargo xtask check (CARGO_BUILD_JOBS=2, 10m02s): exit 1. 12 stages PASS (fmt, clippy, Rust unit 98/98, deny advisories/bans/sources, openapi, format, lint, check, i18n, rhwp notices, pnpm test 766/766, licenses cargo+pnpm). 1 FAIL: third-party notices (3 version headers; integration-owned; diff target/deps-node/notices.diff).
Execution path: the later pnpm stages ran through the host pnpm 11.24.0, which switched itself to 12.15.0 from packageManager, not through the Corepack shim. The CI/Docker Corepack-shim path is NOT RUN here (integration).
Lockfile: the dependency document was written by pnpm 11.28.8 at 691291c. pnpm 12.15.0 validated it with no re-resolution and appended the packageManagerDependencies document.
NOT RUN: browser validation (integration), cargo xtask test integration/OIDC/Supabase (not dependency-affected by Node bundle; integration), Docker builds.
