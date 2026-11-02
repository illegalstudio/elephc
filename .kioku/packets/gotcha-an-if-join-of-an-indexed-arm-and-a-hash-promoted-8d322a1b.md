---
id: gotcha-an-if-join-of-an-indexed-arm-and-a-hash-promoted-8d322a1b
type: gotcha
title: "An if-join of an indexed arm and a hash-promoted arm must join at Hash, not the slot's Mixed storage"
description: "join_arm_types fell through to the frame slot type (Mixed) for Array vs AssocArray arms; ref-param cells hold raw pointers, so the merge read a hash pointer as a Mixed box"
created: 2026-09-26
sources:
  - path: src/ir_lower/stmt/conditionals.rs
    blob: 940ba51c5542c131dcbc6e7c9376ecc48b7bbf26
---

# An if-join of an indexed arm and a hash-promoted arm must join at Hash, not the slot's Mixed storage

## Fact

`if (!isset($calls[$id])) { $calls[$id] = [0, 1]; } ++$calls[$id][0];` with `array &$calls` in a METHOD: the then-arm promotes to hash (`array_to_hash` + store_ref_cell), the else-arm leaves `Array(Mixed)`. `join_arm_types` had rules for all-Array arms and for a Mixed arm only; anything else took `ctx.builder.local_php_type(slot)`, which is Mixed for a ref-bound param, so the merge block typed the cell load Mixed and `runtime.array.fetch_for_write` wrote into a phantom box: every increment vanished.
It only shows when the if TAIL is not duplicated into the arms -- the same code as a plain function worked because its tail was duplicated (each arm kept its exact type). Fix: when every arm is Array/AssocArray<_,Mixed>, join at AssocArray{Mixed,Mixed} and convert Array arms on their edge with ArrayToHash (`local_slot_is_hash_convertible` allows ref-bound slots, unlike element boxing). Test: control_flow::branches_and_loops::test_if_join_of_indexed_and_hash_ref_param_keeps_nested_writes.

## Why

PhpDumper::getDefinitionsFromArguments lost every ++$calls[$id][0] and cache:clear later crashed
