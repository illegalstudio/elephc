---
title: Shared Cargo targets need exact native eval bridge provenance
author: Guillaumeloulier
date: 2026-10-15
cost: { minutes: 0, tokens: 0 }
touches:
  - crates/elephc-magician/src/lib.rs
  - ".plans/issue-1482-constant-identity-review.md"
---

## Decided
When several Elephc worktrees reuse one Cargo target, validate native eval against a default Magician archive built explicitly from the current worktree. Cargo's relative fingerprints and the test helper's timestamp check can accept another worktree's newer archive. Force the bridge crate entrypoint fresh before its build when provenance is uncertain, and record the archive/compiler hashes.

Use Cargo's target/debug/libelephc.rlib alias for focused external harnesses after a successful --lib build, rather than selecting the newest matching hashed rlib: a later documentation exporter build with curl can leave another feature configuration newer in deps. Complete default native/eval tests before the feature-on docs exporter and check whether the top-level default archive actually changed afterward.

## Tried and ruled out
The rebased constant identity test first printed an old unnamed define warning and Fatal error: eval() runtime failed with the previous enum worktree's Magician archive. Without any source change, an explicit default Magician rebuild made all twelve constant codegen tests pass. Treating that first replay as a source regression would have been wrong.

## Watch out
Keep the failed provenance run separate from final evidence. Build, replay, and freeze artifacts before editing their source. A successful compiler build alone does not establish which runtime archive a generated eval host links. Time and token costs were not measured; zero disables automatic historical attribution.
