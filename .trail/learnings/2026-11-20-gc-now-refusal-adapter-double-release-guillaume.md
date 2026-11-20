---
title: "gc-now refusal, adapter double release, defrag heap test"
author: Guillaume
date: 2026-11-20
branch: bead-refusal
cost: { minutes: 59, tokens: 338620 }
touches:
  - src/ir_lower/stmt/array_write_core.rs
  - src/global_decls.rs
  - src/codegen/lower_inst/builtins/eval/dynamic_calls.rs
  - crates/elephc-magician/src/context/request_statics.rs
  - tests/codegen/runtime_gc/heap.rs
---

## Decided
- Offset write on a raw int/float local (native reference view of `int &$x` that eval may retype) goes through the boxed writer (`lower_boxed_array_set`, which boxes the scalar), as main does. The merged peek only routed Mixed/Union there.
- global_decls::collect_global_var_names no longer widens a top-level name bound by `$a =& $b` when a declared body runs eval: the frame ref cell is invisible to load_global/store_global, so the alias read stale. Main has the same limitation for `global`-declared names; not fixed.
- Eval native-new/static-call sites: the branch releases the frame's boxes (status-releasing path); main's `retire_eval_call_argument_boxes` released them again. Kept the branch mechanism, removed the retire call and function.
- Eval request statics (static props of eval-declared classes) are released at CLI exit (`__elephc_eval_release_request_statics`, before the request reset); web still forgets them with the arena.
- Defrag heap test: main's lowering converts a packed list to a hash on unset, persisting a second copy of every string (~200 KB peak), so the test frees via `$keep = []`. Pre-merge branch kept `$keep` a hash from the start.

## Tried and ruled out
- OwnerTransfer peephole (497670a27e) for the bad refcount: the double decref was in emitted asm of the eval bridge call, IR identical to main.
- Allocator/heap_free diffs: whitespace only versus main.

## Watch out
- array_to_hash persists strings even when it consumes the source; moving them would remove the peak (not done).
- test_gc_heap_parks_a_mid_sized_block_in_its_size_class still fails (separate).
