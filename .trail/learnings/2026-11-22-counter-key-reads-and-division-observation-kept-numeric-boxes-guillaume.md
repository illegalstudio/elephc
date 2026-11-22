---
title: Two numeric allocation leftovers had different causes than the bead said (counter keys, a later `/`)
author: Guillaume
date: 2026-11-22
branch: bead-v3d0
touches:
  - src/ir_lower/expr/array_access.rs
  - src/ir_passes/numeric_domain.rs
  - src/codegen/stack_numeric.rs
---

1. counter_keys: the allocating read was `$arr[$i - 1]` (and `$arr[$n - 1]`), whose key is a boxed `ICheckedSub`. Lowering sent every boxed Mixed key on `array<int>` to `ArrayGetMixedKey`, whose result is a boxed Mixed element (an allocation per read); origin/main emitted `ArrayGet` (inline `int|null`). The branch routing is right for a genuinely unknown Mixed key (a string key must warn), so it stays. Fix: when the key's producer is `ICheckedAdd/Sub/Mul` (only an int or a float can come out), lower `ArrayGet`; its codegen materializes the boxed key. stack_numeric already treats `ArrayGet` keys as observations.
2. min_max: `max()` typing and `merge_local_assignment_type` were NOT the cause (the accumulator slot is Mixed on main too, and stack_numeric retires it). The slot was rejected because the code AFTER the loop did `$sum / $count`: `MixedNumericBinop Div` was not a recognized observation in `stack_numeric::load_uses`, so the slot stayed heap-boxed and every `+=` allocated. The fractional/negative variants only passed because they never divide. Fix: `numeric_domain::division` plus a `division` arm in `load_uses` (Div borrows both operands and answers a fresh cell, so it never retains the frame cell; it is an observation only, never a producer, since a zero divisor throws).
Method: probe with `--emit-ir`, then delete post-loop uses one at a time until the allocation count drops; the bead's guess was wrong and a typing change would have been unsound.
