---
title: Reconcile enum constant reviews with merged declaration APIs
author: Guillaumeloulier
date: 2026-10-20
branch: fix/1317-enum-trait-constant
touches:
  - ".plans/pr1827-review-followup.md"
  - src/types/checker/schema/classes/mod.rs
  - src/types/checker/schema/enums.rs
---

## Decided
After main a8f730c5 merged PR1674 during the final PR1827 rebase, keep main's dedicated walk_class_constant and bind_trait_constant_values APIs, the expanded enum schema/driver signatures, and its interface/reference-return paths. The final PR1827 diff adds only missing case collisions, lazy parent normalization, shared enum visibility checks, declaration-scope evaluation and MAY_THROW effects for constant reads.

## Tried and ruled out
The temporary walker opt-in/binding-signature extension in the earlier2026-10-20-preserve-enum-trait-constant-declaration-scope learning described candidate2 on main40c2. It is superseded by the merged declaration APIs; do not restore duplicate magic-walker or driver plumbing. Rebase conflicts were resolved against actual main, not by taking the whole prior patch. Remove the unused eager-normalizer re-export instead of suppressing compiler warnings. Old-base green tests are not acceptance for the integrated source.

## Watch out
Candidate4 replays all final evidence on a8f plus the independent TLS advisory fix:23 standard tests per IR mode (including11 targeted main regressions),32 strict argv cases,50 target program/runtime pairs,102 clean native macOS/Linux executions,20 target diagnostics and the examplebothIR. iOS has real probe function bodies and assembly-only evidence. The enum-case receiver retains the same40bytes on reviewed baseline and candidate4; instance/closure controls remain functional, not heap-clean claims. Keep source/archive digests, a fresh main rebase and exact remote lease when updating existing PR1827. Never replace the user's PR or push upstream.
