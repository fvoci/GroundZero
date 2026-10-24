# deps-integration owned resources (inventory 2026-10-24, task_9a7166336220)

Full IDs are in `resources-manifest.tsv` in this directory. It records container, network, volume and image IDs, creation times, compose project, working_dir and config_files labels. No environment values or secrets are recorded.

Every compose resource is labelled with working_dir `/home/kinesis/orca/workspaces/GroundZero/deps-integration/...`, which is this worktree. The project names come from `xtask::compose::project_name`, which hashes the checkout path (5fcf21).

| Compose project | Created by (this worktree, commands logged in target/deps-integration) | Containers | Network | Volumes / data |
| --- | --- | --- | --- | --- |
| fvoci-deps-integration-5fcf21-test | `cargo xtask test --keep` at b69df07, 2026-10-24 10:54Z (test-plain.log). The Keycloak profile was added by `--oidc --keep` at 10:56Z (test-oidc.log). | postgres (PG17, tmpfs), meilisearch, seaweedfs, keycloak | `_test_default` | no named volumes. Integration DBs are tmpfs; the tests created and dropped their own scratch DBs. |
| fvoci-deps-integration-5fcf21-supabase | `cargo xtask test --supabase --session-pooler --oidc --keep` at b69df07, 10:58Z (test-supabase.log). The official compose comes from the fetched `target/supabase/bc27d266…/docker` checkout. | db, rest, supavisor | `_supabase_default` | `_supabase_db-config` |
| fvoci-deps-integration-5fcf21-dev | `FVOCI_DEV_KEYCLOAK_PORT=8181 cargo xtask dev --oidc` at b69df07, 11:12Z (dev.log) | postgres (PG18), meilisearch, keycloak on 127.0.0.1:8181 | `_dev_default` | `_dev_pgdata`, `_dev_meilidata`, `_dev_kcdata`. They hold the dev seed plus synthetic browser edits: one test-user note, two collab lines, and the DEVP-5 move. |

Local images, not pushed:
- `fvoci-ci-base:deps-integration` (sha256:48394787…, built from the final dev.Dockerfile)
- `groundzero:pr-local` (sha256:4c848772…, app.Dockerfile at 41a05e3, same product bytes as 00c81dc)

Processes: none. The dev server and Vite were stopped with SIGINT and logged a clean shutdown. Port 127.0.0.1:8181 is held only by the dev Keycloak container.

Not owned and never touched: the fvoci-leatherback-b57275-* containers, Keycloak on 8180, other worktrees' projects, and Supabase Cloud.

Nothing has been deleted. Already removed by captured ID after label checks: the temporary smoke and guard containers and networks (fvoci-smoke-*-local-*, deps-int-guard-*, sentinels), listed in handoff.md.
