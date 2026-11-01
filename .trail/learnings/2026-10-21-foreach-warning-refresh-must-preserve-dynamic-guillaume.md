---
title: Foreach warning refresh must preserve dynamic return contracts
author: Guillaume
date: 2026-10-21
branch: reconcile/dirname-symfony
touches:
  - src/types/checker/driver/functions.rs
  - ".plans/symfony-web-execution-ledger.md"
---

## Decided
Keep scalar foreach diagnostics as the latest answer per storage scope and source span, rather than appending speculative warnings to the ordinary warning vector. Revisit affected free functions after both method-return and function-return inference settle, including declared void functions and loops inside nested closures. Preserve an already widened Mixed return while revisiting its body for diagnostics.

The implementation is in src/types/checker/driver/functions.rs, driver/mod.rs, stmt_check/control_flow.rs and checker/mod.rs. Nine focused driver tests pass, including seven foreach controls. The new native dynamic-return control also passes.

## Tried and ruled out
Refreshing after methods but before free-function return reinference leaves the wrapper case warning against its int seed. Re-resolving without preserving the widened return turns a dynamic return into Void. Both were reproduced by failing unit tests before the final adjustment. Old scalar runtime expectations omitted source locations; PHP and the frozen compiler already print the exact path and line, so the updated assertions check both.

## Watch out
Diagnostic success is not native acceptance. The original wrapper segfault and the
later 32-function scalar-seed failure are now repaired by canonical property
storage before return inference and declaration-length return propagation. The
current source passes 10 driver, 41 EIR and 17 native tests; the earlier thirteen
passed / one failed result below the old checkpoint is historical evidence, not
current status. See the companion learning
.trail/learnings/2026-10-21-array-property-layout-must-be-canonical-guillaume.md.
The independently observed vault-list use-after-free and web memory growth remain
separate open gates in .plans/symfony-web-execution-ledger.md.

Build and edit compiler sources only in lazy-petting-popcorn. The scratch target directory holds build outputs, not an alternate compiler checkout.
