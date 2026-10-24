# Cleanup checkpoint

Completed child worktrees removed through normal Orca removal, without force: ablation-body-size, implementation-map, deps-node, unicode-boundaries, ablation-scout, ablation-etag, ablation-home-marks. Their remote branches and necessary result evidence were retained. All worker and owned application/Vite processes have ended.

Sixteen scratch databases on the two A and two E standard-test servers were removed with ordinary DROP DATABASE, without FORCE. For each database the coordinator matched the full name to recorded CREATE evidence, the exact container/project and PostgreSQL system identifier, and checked that no client was using it. The four server catalogs were re-read afterward: only their original `postgres` and `fvoci` databases remain. Detailed before/after and deletion receipts are adjacent JSON files.

Original reproduction databases and Docker services remain. No container, volume, image, shared cache, backup, remote branch or Cloud resource was removed. The C server has retained database names without sufficient creation-time provenance; it was not modified.

The completed deps-rust and deps-integration worktrees remain because their preserved runtime containers bind-mount configuration files from those paths. Browser evidence is copied separately; PR image attachment was excluded by the user on 2026-10-25 and is no longer a retention dependency. Private reproduction bindings were copied to a local 0700 directory, with 0600 files, and excluded from Git. These retained resources are exceptions to worktree cleanup, not active worker tasks.
