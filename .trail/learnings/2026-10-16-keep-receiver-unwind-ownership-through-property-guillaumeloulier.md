---
title: Keep receiver unwind ownership through property-source destruction
author: Guillaumeloulier
date: 2026-10-16
branch: review/1823-1834-finalize
cost: { minutes: 0, tokens: 0 }
touches:
  - src/ir_lower/stmt/instance_property_writes.rs
  - src/codegen/lower_inst/objects/property_store_values.rs
  - tests/codegen/runtime_gc/property_source_destructor_owners.rs
  - docs/internals/the-ir.md
---

## Decided

Keep a nested property receiver registered in the in-flight cleanup chain until post-publication source cleanup returns. Register receiver and source pins separately in their original order. After a successful PropSet/DynamicPropSet, detach the source pin before releasing that source; only then detach the receiver pin and perform normal receiver cleanup. Pins park existing owners and acquire nothing. Skip duplicate source registration when its ValueId equals the receiver.

An unknown runtime slot keeps the owning source explicit in EIR, while each backend arm borrows/retains its actual representation. This makes the source destructor observe the published field and also covers the scalar leak reported in #1834. The existing PR #1823 already contained that scalar correction even though its body referenced only #1643: verify related PRs by mechanism before interpreting absent closing references as absent implementation.

## Tried and ruled out

Detaching both pins directly after the store strands the receiver when a fresh Stringable source destructor throws. The two new regressions (nullable and mixed nested properties) match PHP output but retain 40 receiver references, leaving 3 blocks / 128 bytes on the rebased reviewed baseline. Keeping the source pin through its own release would instead expose that owner to a second cleanup. The corrected split passes both regressions and the eleven #1834 fixtures in both optimizer modes, alongside existing compound, string, transfer, and throwing-guard controls.

## Watch out

Source destruction is part of the throwing window even after the property has been published. Do not move receiver detachment above that release, or move the source detachment below it. Document this lifetime order in docs/internals/the-ir.md rather than treating a plan as compiler documentation. Keep updates on the existing fork PR branch and fold the added issue coverage into that PR.
