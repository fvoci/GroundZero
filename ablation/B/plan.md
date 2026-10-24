# B: serialized document byte count

Baseline: 22ad80ee205332e3ef1ffd8f64f783789955921b, fvoci/ablation-body-size.
Initial tree clean. rustc 1.99.0 (b940084d7 2026-09-28); cargo 1.99.0.
Cargo.lock SHA256 afacc07bfd285d2079e5ef770e62e2d3aafd011f8aeb31dff0de63a9314a0d49.
rust-toolchain.toml SHA256 72f7dd8c9fdf9c1fcd8eb8bf9326d4bc66833e4ebf4bc2932636e2b85efcb7af.
Original documents.rs SHA256 eaf06666159cb69024f45f9ef0164fec85fbf973bc1368d4bb099b9258134ff9.

Current reason: validate_body constructs a full serde_json Vec only to obtain its
length, then drops it before references/image/link validation. Replacing its sink
with a counting std::io::Write should remove that payload allocation. It must
still run serialization fully, preserving the full error size, not short-circuit
at the configured limit. No serializer, JSON representation, caller, DB, auth,
version, request id, or transaction changes.

Baseline behavior (source inspection; execution initially NOT RUN): object type
check, compact serialization or BadRequest serialization error, inclusive byte
limit, iterative reference/image/link checks. The stack visits array entries in
reverse order. Reference count increments and rejects above the limit before
validating that excess node. Record regression execution BEFORE product change.

Baseline execution PASS before product change: CARGO_TARGET_DIR=target/ablation-body-size/build
cargo test --locked -j2 -p fvoci-server --test ablation_body_size. All 3 tests
passed (0.14s test time; 2m03s cold dependency/test compilation), see baseline-test.log.

Tests: exact size +/-1 including Unicode, control characters, escapes, integral
limits, fractional/exponent numbers, null/bools/arrays/objects; large text;
128 nested arrays; unchanged input; wrong root type before size; size before
invalid image/link/reference; traversal precedence; inclusive reference count
and excess count before malformed-node error. Durable test file uses public
validate_body. No DB is needed to establish this pure-helper contract.

Metrics: full helper CPU/wall ns per operation, alternating baseline/variant
batches on identical preconstructed deterministic values, report medians/p95,
spread and paired changes. First-call samples separate from warmed batches;
no assertion of OS cold caches. Vec capacity is the directly observable peak
temporary buffer proxy; sink size is constant, not an RSS/allocator count claim.
Inputs: minimal, small prose, Unicode/escape-heavy, repeated nodes, near-limit
large text, over-limit large text, deep tree, references. Do not benchmark while
dependency builds or other measurement tasks run. Build output stays private
under target; no updated dependencies/other worktree outputs are used.

Prepared harness uses verbatim extracted validate_body and reference/image/link
helpers from each version, with minimal equivalent Limits/BadRequest boundary
types. It is not a full request benchmark. Both variants share the identical
pinned release serde_json/uuid dependency artifacts. Warm runs alternate AB/BA
order over 41 pairs. Separate fresh processes take first-validator samples
(input construction occurs first); those are distinct from warmed batches and
do not claim controlled OS/page-cache/CPU-cache coldness. serde_json 1.0.154
ser.rs:2213-2219 confirms to_vec uses Vec::with_capacity(128) then to_writer.

Accept as adopt-proposal only with identical regression results and byte/error
semantics, removed size-proportional temporary payload, repeatable benefit or no
material CPU regression (>5% beyond repeat noise on ordinary/large inputs),
unchanged call-path contract, and a minimal reviewable patch. Reject mismatches
or consistent meaningful regressions. Insufficient if noise/resource constraints
or missing execution prevent a defensible conclusion. Coordinator makes final
adoption decision from raw data.

Call paths: documents::save validates title/body before opening a UserTx; all
permission, replay, If-Match, base revision, same-content checks and atomic
revision/reference/audit writes remain subsequent and untouched.
revisions::restore_in_db reads historical content in a user transaction and
validates before app_restore_document_revision; failed validation drops tx.
Room::snapshot_with validates before choosing an author/write_snapshot; failure
sets failed_checks and emits collab.invalid_document, leaving unsaved content.
Room update validation projects a scratch CRDT document after shape/state-size
checks; BadRequest text becomes Verdict::Invalid. Equal helper return values
preserve these decisions, but do not alone prove DB/RLS/events/audit behavior.
Request isolated integration validation separately; never equate static review
or helper tests with DB coverage. No service started at planning checkpoint.
