# Updated-main C validation protocol, before new timing

Normal local merge head: ba6ac88e34b1227108298c2442a825f1d056e3a4, merging dependency main e0103ef5fbcb818a66d2156cc7c742d51ea321fc into source commit 51b240b5a392c24808d0198c4e511b0a6ef2f850. Cache source hash is unchanged. Rust 1.99.1, server sha2 0.11.1, Node 24.22.1 via existing installation/Corepack, pnpm 12.15.0; exact lock/tool hashes are in updated-identity.txt.

Before timing, independent Unicode main 06809206e9f7cdc88e538502d7f6fab3dff60d76 was normally merged/pushed as 6d2d1a469a8b3f1e8172a5d653ecdc4d573d402b. Cache source, all recorded pins and the optimized harness source/binary/sha2 rlib hashes are unchanged. The actual compiled/tested SHA remains ba6ac88; do not relabel it as the newer merge head. The prebuilt benchmark is reused as explicitly directed, and new-head CI covers the independent Unicode integration.

Original Rust benchmark source and summarizer are copied separately, without changing inputs, functions, warmup, case order, batching, sample count or the predeclared acceptance criteria. Counterfactual old formatter and candidate will both use the same new locked sha2 0.11.1 and Rust 1.99.1. Original Rust1.99.0/sha2 0.10.9 timings remain separate historical evidence, not a cross-version causal comparison.

Build only after standard checks/tests complete, at <=2 jobs, in a fresh private benchmark target. There are two sha2 versions in the new lock, so select sha2@0.11.1 explicitly. Validate exact old/new function source and byte-equivalence corpus before timing. Wait for the coordinator's explicit quiet slot, pin CPU31, run two sequential warmed 31-pair AB/BA invocations, preserve first-call observations separately, and monitor competing build/test process names as before. Any detected contamination invalidates the whole new window and must be preserved under distinct names.

Original threshold retained: hex-only median improvement >=10% and >=90% paired improvements in both runs, with distributions/MAD reported. No endpoint-latency or measured allocation-count claim. Large-body total improvement requires evidence beyond observed noise; otherwise report no demonstrated gain.

Push is held until the coordinator's CI-image release, independently of local check completion. No new candidate, source cosmetic changes, toolchain/dependency edits, history rewrite, manual DB/volume/backups deletion or resource cleanup is authorized here.
