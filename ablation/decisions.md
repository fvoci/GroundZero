# Ablation adoption review — provisional until dependency-main validation

Baseline: `22ad80ee205332e3ef1ffd8f64f783789955921b`. These are coordinator observations and adoption decisions, not product policy. A passing test suite alone did not determine adoption.

| Candidate | Current conclusion | Evidence and remaining gate |
| --- | --- | --- |
| A: incremental character accounting in search text extraction | Conditional adopt, PR #122 | Full-string oracle on 326 cases and cap/Unicode regressions preserve traversal and output. Many-node helper measurements improve about 96–99%; tiny inputs cost about 3–4%, and a single huge node is unchanged. Revalidate against the updated dependency/toolchain main and repeat quiet measurements. |
| B: serialize document JSON into a counting writer | Reject | Three repeated runs improve the large fixture by about 52% but regress escaped Unicode by 18–20% and deep structures by 22–24%, beyond the declared 5% threshold. Preserve the rejected source and raw results; do not merge. |
| C: direct fixed hexadecimal ETag encoding | Conditional adopt, PR #121 | 3,076 body/user pairs preserve exact tags and user separation. Small full-hash cases save about 0.5 microseconds; 1 MiB input has no demonstrated gain. Verify with updated sha2 and repeat quiet measurements. |
| D: parallel home calendar/notification requests | Reject at static triage | The shared client connection banner is order-sensitive: a later success can clear an earlier failure. Component-only parallelization changes failure behavior; fixing that requires a separate client-state design. No performance experiment was run. |
| E: combine home pin/favorite reads | Conditional adopt, PR #124 | Same read-time authorization, RLS, category order and duplicate category membership. Two optimized ABBA/BAAB passes show paired warm-stage reductions of 48.6–49.2%, 35.7–36.4%, and 19.3% at 0/1/100 marks per category. Complete standard validation, then revalidate updated main and repeat quiet measurements. |
| F: merge Up Next candidate/count queries | Insufficient evidence; end investigation | No measured physical-plan cost or race-equivalence evidence justified a speculative SQL change. No product change or performance claim. |

E observes one READ COMMITTED statement snapshot instead of two. Stable-input outputs and authorization at statement read time were checked; identical concurrent instruction interleavings and immediate suppression of a response after later revocation are not claimed. The coordinator accepted this qualification before conditional adoption.

A and C are helper-level measurements; E measures SQL fetch/decode/filter/split. None demonstrates whole-endpoint or user-perceived latency improvement. Measurements used synthetic data, recorded shared-host background activity, and separated warm from process/plan-first conditions. Raw evidence describes those conditions more precisely.

The bounded investigation ends with A–F. No further candidates are authorized by this review. Final adoption still requires latest-main inclusion, relevant regression checks, CI and CodeRabbit, and a coordinator merge decision. All three adoption PRs remain Draft at this checkpoint.
