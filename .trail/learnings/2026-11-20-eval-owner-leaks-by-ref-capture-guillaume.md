---
title: "Eval owner leaks: by-ref capture stores, callable property sets, attribute helper, request statics"
author: Guillaume
date: 2026-11-20
branch: bead-eval-leaks
cost: { minutes: 96, tokens: 246038 }
touches:
  - crates/elephc-magician/src/ffi/include.rs
---

## Decided
Five independent root causes sat behind the eval-owner leaks of h17m.84. (1) StoreVar inside a closure `use (&$x)`: written_value_is_owned_by_entry said false for a Cell target that is a PHP reference, but reference_replace copies and consumes nothing, so the assignment's owner leaked (object, string, array). (2) The property setter's Callable arm reused the method/constructor cast, which adopts the descriptor into a bridge owner list the property bridge never drains; a new _owned cast only resolves. (3) retire_reflection_owner_allocation_after_boxing takes an offset BELOW x29 while the ARM64 ReflectionAttribute helper passed its sp offset (40 instead of 56), so it decref'd the target-bitmask slot and every attribute leaked. (4) Request statics (static properties and locals of eval code) were only forgotten at CLI exit; new __elephc_eval_cli_request_end releases them first, as php does at shutdown. (5) class_override_results is NOT a leak first: a by-ref method (`&items()`) called by value in pure eval returns the property's cell, so `$copy['z']=9` mutates the object; filed as a follow-up bead.

## Tried and ruled out
keep_closure_ref_capture_cells / replace_closure_capture_value ownership: releasing or tracking the registry copy changed nothing (the retain and the release of the borrowed old value cancel). A lldb walk of _rt_heap_alloc frames (--keep-symbols, frame #2) against main showed the second reference_replace clone in StoreVar (write_back_method_ref_target then set_scope_cell) that main does not have.

## Watch out
eval_foreach/iterator_apply "null-tag Mixed" leaks were one boxed int per static property (`self::$d++` in a destructor), found by reducing the test to one eval of an eval-declared class. Magician lib tests have 14 pre-existing failures identical on 3f4d4d1b30. Offsets given to abi::load_at_offset are always frame-pointer relative, never sp relative.
