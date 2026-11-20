---
title: ArrayObject nested writes need the boxed-cell fetch
author: Guillaume
date: 2026-11-20
branch: bead-arrayobject
touches:
  - tests/codegen/runtime_gc/property_dimension_fetch_modes.rs
---

## Decided
ArrayObject write-fetch (`__elephc_fetch_for_write`, lowered in `ir_lower/core_methods.rs`) now treats `storage` as the boxed Mixed cell it is: `PropGetForWrite` typed Mixed, then the boxed-receiver `ArrayFetchForWrite` runtime call, which attaches or autovivifies the element and answers the stored cell. This is the same path a user `array` property takes. `ArrayIterator` keeps its packed `ArrayGetForWrite` over the parallel `values` list.

## Tried and ruled out
Switching to `ArrayGetMixedKeyForWrite` alone changed nothing (the element is not an attached cell yet). Ensuring the key with a concrete-`Array` `ArrayFetchForWrite` plus `PropSet` injected garbage entries 0..4 into the storage: a `storage` property declared `array` is a boxed Mixed cell, and reading it as `Heap(Array)` reinterprets the box. Check the property type in `--emit-ir` (`prop_get ... php=array` with `Heap(Mixed)`) before choosing a container op.

## Watch out
The merge moved ArrayObject from parallel keys/values lists (main) to one native `storage` array (branch), so main-era write-fetch code that indexed `values` silently misreads it. Local `$ao[k][..]` and property `$this->data[k][..]` nested writes (+=, ++, =, append, new string key) now match php -n. test_property_nested_fetch_modes_bundle and its xml twin still fail only on a separate bad-refcount defect with no ArrayObject in it (bead elephc-h17m.87, repro w1.php: compound result on an array property sharing a copy).
