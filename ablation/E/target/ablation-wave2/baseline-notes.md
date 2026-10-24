# E experiment — pre-edit declaration

Pinned base: 22ad80ee205332e3ef1ffd8f64f783789955921b; clean tracked baseline. Exact input hashes/tool versions recorded in baseline-environment.txt. No .codegraph directory. No managed Cloud status tool or network-policy file in this local Orca session; no Cloud/external tests used.

Owned resources: private target/ablation-build, standard checkout-derived Compose fvoci-ablation-home-marks-f0352d-test (PG17/Meili/SeaweedFS), retained without deletion. Baseline compile uses CARGO_BUILD_JOBS=2. No timing while other work is heavy.

Predeclared gate: exact stable-input category output and per-statement authorization; no mutations/audits/events/notifications. Reject semantic failure, added policy/isolation/locks/cache/dependencies, or repeatable common-case >5% regression outside measured noise. Paired ABBA blocks: medians, p95 and MAD; regression is credible only when paired median delta exceeds 5% AND three times paired-delta MAD across repeated blocks. Report ambiguous/noisy result as insufficient; never infer full-home improvement from marks-only timing. Plan-first samples separate from warm samples. Synthetic counts 0/1/100 per category, identical inputs for A/B; both-marked and hidden output proof separately.

Baseline source cost: two awaited SELECT/fetch_all calls (pin then favorite), identical joins/projection and different bindings; label computation remains per returned mark. Actual query plans/rows/buffers and behavior must precede product edit. Snapshot authority accepted by coordinator task: READ COMMITTED statements can differ, grouped read observes one statement snapshot; do not claim instruction-interleaving equivalence.
