---
title: Override ladder narrowing is undone by refine_member_receiver
author: Guillaumeloulier
date: 2026-11-18
cost: { minutes: 529, tokens: 6152606 }
touches:
  - src/ir_lower/expr/override_arity.rs
---

## Decided
The override ladder (override_arity.rs) lowers the call against the narrowed class through lower_method_call_with_refined_receiver, which skips refine_member_receiver.

## Tried and ruled out
Changing the ladder arms or wider_override_classes: they were correct. The Move to the descendant class was right in IR but retyped back.

## Watch out
Main added refine_member_receiver to lower_method_call_with_receiver; it re-types any receiver to the checker-recorded member class (the ancestor), undoing the ladder narrowing. The override then got the ancestor arity and crashed (pc=0xc / garbage defaults). Any new caller that narrows a receiver via Op::Move must use the refined entry point.
