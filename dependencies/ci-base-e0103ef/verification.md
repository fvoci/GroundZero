# CI base transition

- Source main: e0103ef5fbcb818a66d2156cc7c742d51ea321fc.
- Build/publication: https://github.com/fvoci/GroundZero/actions/runs/40093828827 — PASS.
- Registry index digest: sha256:055178adbb91af6ec677fc26e56ea5f1bdb72ac9b50ed2c077688e8ed2f8946f.
- Linux/amd64 manifest: sha256:99ed249f643227ba25b76a288e473861f9f23c8bbd0d731291e82ac5d96365e7.
- Build log and OCI tag match source revision and digest.
- Previous CI_BASE_IMAGE_DIGEST: sha256:f97099653399b5d85d97234d75c98a6e000542e9f6301411f5347862f4ee9327.
- Repository variable changed at 2026-10-24T12:34:56Z, independently read back through GitHub REST.
- Local digest pull started before pointer change, remains pending at this checkpoint. Because publication/provenance are verified, new CI is allowed to verify actual runtime versions concurrently; local non-root/offline check remains required before final completion.
- Expected pins: Rust/Cargo 1.99.1, Node 24.22.1, Corepack 0.36.0, pnpm 12.15.0. Runtime verification is PENDING at this checkpoint; publication success alone is not runtime success.

## Completed runtime checks

- New digest CI licenses job 120873968095 (PR120 run40094552932): exact Rust1.99.1, Node24.22.1, Corepack0.36.0, pnpm12.15.0; license allowlists PASS.
- Local published digest pull completed. OCI revision label equals e0103ef.
- Initial artificial manifest-only mount printed the right versions and exit0, but pnpm emitted a metadata lookup warning because the lockfile was missing. This is NOT a hermetic PASS.
- Correct repository-input check mounts exact e0103ef package.json + pnpm-lock.yaml + pnpm-workspace.yaml read-only; UID/GID1000, HOME=/tmp, CI=true, COREPACK_ENABLE_NETWORK=0, network=none. All five versions PASS without warning in under one second; container removed by --rm.
- This check verifies tool startup and cached package-manager binary, not offline installation of every project dependency. Frozen project installs are verified by the worker/CI standard checks.
