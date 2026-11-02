---
title: Validator dominators must follow CFG order including exception edges
author: Guillaumeloulier
date: 2026-10-21
branch: reconcile/dirname-symfony
touches:
  - src/ir/validator.rs
  - target/framework-audit-20261020.Wcomr7
---

## Decided
In src/ir/validator.rs, compute_dominators now traverses reachable blocks in reverse postorder instead of numeric block-id order. Derive that order from compute_predecessors, including TryPushHandler's implicit handler edges. Keep unreachable blocks as singleton dominator sets and retain every validator check. An iterative DFS avoids adding Rust stack depth. The optimizer's terminator-only CFG is not interchangeable with this edge set.

## Tried and ruled out
Bitsets alone did not remove the bad schedule: a reversed 256-block chain required 255 complete sweeps; a 2048-block chain required 2047. A live application sample spent 71 of 81 samples in the validator dominator calculation. Replacing the validator graph with the existing optimizer RPO would omit exception edges and change validation semantics.

## Watch out
19 focused validator tests and 15 mem2reg tests pass. An independent set-based fixed point checks 512 deterministic CFGs, including nonzero entries, dead islands, cycles and exception edges. The manual profile is separately executed, not ignored as evidence: three interleaved baseline/candidate trials reduced median inner time from 1432381 microseconds to 3776, and sweeps from 2047 to two. These are graph-level measurements, not application-wide speedup claims; quadratic bitset storage remains. Artifacts are under target/framework-audit-20261020.Wcomr7, including validator-paired logs, immutable benchmark binaries and hashes, and source-state-rpo.json. The CLI enum-array smoke test passes with the rebuilt compiler. Its native web probe responds, but reports measured, UNFIXED PHP divergences: ini_get variables_order returns false instead of EGPCS, and SERVER exposes a process environment variable that PHP cli-server does not. Framework warning-free and complete console acceptance remain open.
