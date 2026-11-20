---
title: By-ref foreach detach must release a raw owner first
author: Guillaume
date: 2026-11-20
branch: bead-eval-foreach
cost: { minutes: 51, tokens: 270526 }
touches:
  - tests/codegen/runtime_gc/native_reference_body_views.rs
---

## Decided
The by-reference foreach value-variable detach (`unset_local` + `zero_local_slot`, added for nested loops) now starts with `release_raw_local_slot`, exactly like `unset($v)` in ir_lower/context.rs. An eval() writeback leaves an owned null Mixed (___elephc_eval_value_null) in a not-yet-bound local such as `$entry`; zero_local_slot overwrote it without a release, leaking one 40-byte null Mixed (kind=5 tag=8). Main had no unset/zero pair there, so it never overwrote the owner.

## Tried and ruled out
Not the runtime: __rt_retire_mixed_slot, __rt_mixed_box_container_3 (identical tags to main's inline boxing), bind_hash_current_value_ref and release_local_slot all match main. The IR diff (unset_local/zero_local_slot present only on the branch) plus an lldb breakpoint on _rt_heap_alloc (symbols need --keep-symbols and the name _rt_heap_alloc; frame #2 gives the fn_f offset) found it; a plain asm diff of main vs branch is useless because the frame layout differs entirely.

## Watch out
Of the 56 gc-now tests, 45 still fail after this and have other causes (wrong output, bad refcount, other leaks); this fix flipped only the primary bundle and the new test.
