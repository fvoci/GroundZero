# Product-image smoke failure diagnosis

Failed PR head: `41a05e3170d1e9a8978f03bd6f4637a66efa3f16`. GitHub run `40090125648`, amd64 job `120861126500`, arm64 job `120861126216`.

Both images built successfully. The amd64 smoke passed platform, OCI labels, UID 10001, runtime files and compiled `version` assertions, then failed after 60 seconds because `/healthz` did not serve build metadata. The smoke started the API process with an unreachable loopback database endpoint.

On the pinned main `22ad80ee205332e3ef1ffd8f64f783789955921b`, `app::run` already awaits `state.db.check_baseline()` before binding the HTTP listener. This was introduced in `2e8b58b235449f461c3d4f7d8a190c5bb576fee3`, not by the dependency update.

Coordinator reproduction ran the existing debug binary from the completed Node worker and the updated dependency-integration binary with an empty inherited environment, API role, ephemeral loopback bind port and the same unreachable loopback database endpoint. Both exited with status 1 after five seconds: `server.starting`, then `server.exit_error` with `database pool timed out`; neither reached `server.listening`. The Node worker's Cargo.lock, Cargo.toml, crates, xtask and rust-toolchain files have no diff from pinned main. Binary hashes and the two short logs are retained locally alongside this note; this is a baseline-equivalent artifact comparison, not a new clean release build of baseline.

Decision: preserve the product startup/baseline guard and all smoke assertions. Repair the existing smoke setup with a fresh owned PostgreSQL instance initialized through the existing product `db bootstrap` and `db migrate` commands, use the app role for runtime, and clean up only verified newly created resources. Validate the repaired smoke before requesting the final CI cycle. No failed-check bypass or product-policy change is approved.
