# Bounded tail characterization — declared before passes 3/4

Coordinator instruction msg_ac05a449f6b6 (2026-10-24 13:18:44 UTC) authorizes exactly two additional full passes of the unchanged prebuilt optimized harness. Original passes 1/2 remain retained. No new code, profile, case, data, policy, harness build or automatic rerun loop. Await a new explicit quiet grant. Compiled product4c8551b; current branchd69cbd4 after independent A/C main merges; E/lock/migration/harness identities unchanged.

Original median/oracle and contamination gates remain. Additional acceptance gate, declared before these follow-ups:

1. Pool n100 plan_first samples from all four passes (24 per variant/pass, 96 per variant), using the same nearest-rank p95 definition ceil(0.95*N).
2. Pooled candidate p95 must be <=1.05 times baseline p95.
3. Both new passes must NOT each repeat candidate p95 >1.05 times their baseline p95.
4. If any required gate fails, classify E insufficient/reject for tail risk and stop; do not seek a different profile or more samples until green.

Initial observed p95 deltas: +3.4819747% and +11.8064777%. At N=24, p95 is the 23rd sorted value (second largest), so this estimate depends on very few tail rows; it is still actual evidence and is not dismissed. Pass1 block10: A58.055/56.038ms, B73.752/48.848ms, block-median +7.456%. Pass2 block10: A56.965/57.874ms, B72.991/48.525ms, +5.814%; block7 A56.874/55.913ms, B72.940/47.631ms, +6.902%. Full rows and ranks are in tail-pass1-2.json. Repeated block10 high B and a second high B in pass2 warrant bounded characterization despite faster overall medians.

Workload evidence: no compiler/cargo/pnpm/npm/git job or container start/die was observed during initial updated window. The approved pre-existing Python process used5005 ticks (~one core across50s). A new native2.1.310 process appeared only in the last monitor sample at13:15:45.528Z with zero measured ticks; this does not establish a heavy overlapping job or explain earlier plan-first tails. Four new PostgreSQL processes are the expected benchmark connections. All observations remain retained; no samples removed and no dedicated-hardware claim.
