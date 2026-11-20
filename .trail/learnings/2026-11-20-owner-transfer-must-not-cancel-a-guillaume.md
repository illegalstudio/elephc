---
title: Owner transfer must not cancel a release behind an earlier consumer
author: Guillaume
date: 2026-11-20
branch: bead-nested-rc
cost: { minutes: 44, tokens: 69465 }
touches:
  - tests/codegen/runtime_gc/property_dimension_fetch_modes.rs
---

## Decided
The owner-transfer peephole (`b = acquire a; ... release a` becomes `b = a`) only allows earlier uses of `a` that fill it as the receiver of ArrayPush/ArraySet/HashSet. Any other earlier consumer keeps the pair. Codegen (`value_can_transfer_ownership_to_consumer`) lets a retaining consumer steal an owned value's reference when no explicit Release names it, so cancelling the release turned the earlier `runtime_call` array write (and an earlier `store_local`) from retaining into stealing and the value ended up with two holders. `$r = ($box->items[0][0] += 1)` freed the new cell twice: "heap debug detected bad refcount" at exit, stdout right, no copy needed.

## Tried and ruled out
Not the property lowering and not separation: main and head emit the same pre-optimizer IR (`--ir-opt=off` is clean). Disabling the pass, then skipping single transfers (ValueIds 86 and 87 of the chain acquire/acquire/store/release/release) bisected it. The "shared copy" of the bead is irrelevant.

## Watch out
Any pass that removes an explicit Release must re-check every earlier consumer of the value against codegen's transfer rule. An unrelated `StoreLocal` earlier use had the same latent hazard.
