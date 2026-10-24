# Safe evidence selection at baseline PR checkpoint

`archive-files.txt` is a newline-delimited allowlist of repository-relative text files, not a glob. `archive-sha256.txt` pins exactly those bytes. Preserve these three manifest/notes/hash files alongside the listed files.

- Exact source and oracle: baseline source, minimal patch, benchmark Rust source, source-match output, fixed/deterministic equivalence output, plan and identity/pins.
- Accepted measurements: runs 3/4, independent robust summaries and the small summarizer, accepted timing bounds and shared-host process/load snapshots. Snapshots contain only PID/state/command name/CPU; no command arguments or environment.
- Excluded window: runs 1/2, interference PID and start timestamp. These are NOT VALID performance results and are preserved to show the exclusion.
- Verification: clean baseline unit, candidate unit, full standard integration log and compact count summary, Clippy log, immutable CI run/job result snapshot and review coverage/disposition links.
- Resource ownership: retained service container IDs/ports plus exact Orca run/task/dispatch provenance and fixture-cleanup evidence limits in `archive-checkpoint.md`.

No binary, image, credential/cookie contents or raw environment dump is selected. The compiler/integration logs were checked for password/secret/auth/cookie/token/DB-URL patterns; all seven hits were dependency or passing test names, not values. The Rust harness contains only synthetic byte inputs and user labels. No GitHub bot UI access URLs or HTML dumps are included.

The manifest deliberately omits the compiled `bench`, empty stderr files, broad host dumps, and original `evidence.sha256`/`harness-sha256.txt` because those manifests cover omitted binary or unnecessary artifacts. Original local evidence is unchanged and remains preserved. The selected original report records its earlier, pre-PR state; the finalization checkpoint explains the later commit/PR/CI/review state.

This is a snapshot before dependency-main integration; it contains no updated-sha2 timing or updated-main validation claim. The live `report.md` in this directory may later evolve, while `archive-checkpoint.md` remains pinned for this archive.
