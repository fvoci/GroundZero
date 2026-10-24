# Bounded tail follow-up, declared before additional samples

Coordinator message msg_ac05a449f6b6, 2026-10-24T13:18:44Z. This supplements, and does not replace, the pre-existing median/oracle gates.

Initial new-pin passes1/2 have consistent median improvements, but n100 plan-first p95 changed +3.4819747% and +11.8064777%. Each pass has only24 samples per implementation in this phase. This is a concrete tail concern, not a discarded failure or a flaky label.

Run exactly two further passes of the unchanged prebuilt protocol on the same synthetic DB and source/lock/binary inputs, after a new coordinated quiet release. Preserve all four passes and all samples; do not rebuild or change implementation, cases, profile, thresholds or filtering. Label new samples passes3/4.

Additional conservative adoption gate: the existing oracle and median/noise gates must still hold; pooled n100 plan-first p95 across all four passes must be at most1.05 times baseline, and the two additional passes must not both individually repeat greater-than5% p95 regression. Use the same nearest-rank p95 definition. Failure leaves E rejected/insufficient for this batch; no further attempts to chase a passing result.

This is not a product SLO or new policy. It is a bounded experiment acceptance decision responding to the observed tail, and neither outcome establishes endpoint-wide or production-load performance.
