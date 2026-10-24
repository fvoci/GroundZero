# Wave 2 — static assessment only

Read the authoritative goal attachment, repository guidance, applicable skills,
decisions and actual callers/tests. No product edit, build, test, benchmark,
DB/service/browser operation, PR or delegation occurred; all execution is NOT RUN.

HEAD remains `22ad80ee205332e3ef1ffd8f64f783789955921b`. Inspected home files are
clean at that base. Home source hashes: home.rs
`b610280124ad49763d5ad03772f24c0362204bf51d200c0fab6fbd67dba838ae`, HomeDashboard
`93352f58890c3e0a6ee9283f07996c4c5dc8dc9610d0ebea53663b44b8602313`.
Rejected B source/test match its checksum manifest; its uncommitted branch and
all evidence remain intact. Do not make this dirty B checkout a product branch.

| Candidate | Classification | Reason |
| --- | --- | --- |
| D: calendar/notification overlap | REJECT component-only change | Concrete shared-client banner failure-ordering mismatch |
| E: grouped pins/favorites read | ADOPT-EXPERIMENT, not adoption | One removable round trip; snapshot/correctness gates below |
| F: Up Next list/count reuse | INSUFFICIENT, triage hold | Small SQL option conceivable; physical cost/concurrency unproved |

## D — failure semantics block the proposed change

Path: App.svelte:270 → HomeScreen.svelte:25 → HomeDashboard.svelte:61. After home,
it awaits allSettled(calendars), merges week data, then requests notifications;
all results publish behind loadSeq. Individual calendar failures are tolerated;
home/notification errors become screen errors. Ideal C+N → max(C,N) is a graph
observation, not a measured speedup. Server endpoints read; notification GET
does not mark entries read. But client reads have user-visible effects.

Source-derived counterexample, NOT RUN: client.ts:2215 request() reports connection
loss when parallel notifications fail at fetch; a later calendar success clears
connection.lost. Baseline finishes calendars first, so its final notification
failure leaves the banner on. Later component awaiting cannot defer effects
inside the client. Gateway failures have the same issue; fail() also handles
session/account state. This violates protected D-82 behavior. Do not bypass the
shared client or redesign banner policy for this ablation. Existing week/count
component tests cover success, not this permutation. A separate reconsideration
would need immediate rejection handling, partial failures, stale reload/quick-add,
unmount/navigation and session/banner tests; it exceeds component-only scope.

## E — one isolated experiment is justified

home.rs:205 marks() repeats projection/joins with different mark bindings;
home():330–331 awaits Pin then Favorite in one UserTx. These are different
categories, not identical duplicate rows: save one round trip, not necessarily
half the scans/labels. A both-marked target must remain in both output lists.

Replace only those reads with one tagged pin/favorite query and stable split.
Keep UserTx, projection/joins, app_time_target_label, null-label filtering, target
parsing, each category's created_at,target_id order, schema and commit point.
No dedupe/cache/new limit/parallel transaction, write API, migration, index,
dependency or contract change; do not invent a new tie-order guarantee.

Authority: migration:10559 user_marks SELECT RLS filters app_user_id(); the STABLE
SECURITY DEFINER label function at 8382 has fixed search_path and explicit target
access checks. Outer joins retain RLS. Mark writes/advisory locking/100-visible-
marks-per-kind cap and toggle idempotence at 6540 onward remain untouched.

Mandatory caveat: normal READ COMMITTED permits different snapshots for the old
two statements; one query sees one snapshot. begin_as sets local user context,
not isolation; actual runtime isolation is NOT RUN. Mark/access/rename/delete
changes between reads can affect the old second result. Do not claim literal
interleaving identity. Coordinator must confirm existing authority at one
statement snapshot is acceptable; reject if this requires new policy or exact
old cross-statement observations. Do not add isolation/locks to hide the change.

Required baseline/variant gates: exact home output for empty/pin-only/favorite-
only/both-marked, each target kind, personal/shared/cross-tenant, hidden/deleted/
archived/renamed targets, ordering and NULL-user/owner/admin/outsider access.
Add home favorites/both-marked protection: existing home tests at 77,292,377
emphasize pins or exercise the separate mark-state endpoint. Compare 401,
invalid timezone, DB/query failures, cancellation and rollback/commit behavior;
controlled mark/access races must authorize every label at its statement snapshot.
Assert no writes, event/audit/notification effects or personal-note leaks.

Planned metrics: marks-stage/full-home median/p95 latency, round trips, returned
rows/bytes and DB execution/buffer work at 0/1/100 visible marks per category,
both-marked and hidden fixtures. Identical synthetic inputs, AB/BA repeats, warm
versus connection/plan-first, paired distributions/MAD; quiet window only.
Require meaningful cost reduction; reject repeatable >5% common-case regression
beyond measured noise, semantic failure or complexity outweighing one saved trip.

## F — hold at triage

home.rs:143 UP_NEXT_MINE feeds ranked/list (261) and count (276) queries. Current
reason: independent 50-row bucket caps plus complete totals prevent overdue
backlog hiding today/upcoming. Preserve mine/assignee/creator/RLS/workspace/
archival filters, due_date/number/id order, i64 totals and zero buckets. W23
server home.rs:292 and home-up-next-counts component tests protect truncation.

A bucket-count window before rn <= 50 might be small, not automatically a broad
redesign. But default ordered frames can accidentally yield running counts;
empty buckets, ties, concurrent bucket/assignee/access changes and statement
snapshot changes need proof. Fewer statements do not establish less physical
scan/sort/window/access-check work. No plan/cardinality/cost evidence exists here.
Hold insufficient; reject if it needs broad query/contract redesign, new indexes/
policy/cache or JSON aggregation scaffolding just to force a result.

## Ownership and stopping

Only E merits an independent clean child from the pinned base now: owner limited
to home.rs and a unique focused test. E/F overlap the handler; do not implement
them together or bundle their SQL. D requires separate rescoping. Coordinator
selects worktrees/resources/adoption; recheck validated minimal changes on
dependency-updated main before PR/Ready/merge. Integration must use --keep and
preserve synthetic DB/volume/backup resources; no Cloud or deletion. This is the
requested three-candidate wave: if E is not adoptable and no validated candidate
remains, stop under the user's rule rather than inventing more candidates.
