---
title: "Eval object property overlay leaks at CLI exit: scope freed after the registry reset"
author: Guillaumeloulier
date: 2026-11-20
cost: { minutes: 141, tokens: 576778 }
touches:
  - crates/elephc-magician/src/ffi/context.rs
---

## Decided
h17m.88 was not about __get or by-ref appends. Any eval-declared class with a public property default (`class R { public $p = "abc"; } $o = new R();`) or a dynamic property write, still alive at process end, leaked the overlay value (a boxed copy the context owns, stored by eval_store_dynamic_property_value). Cause: the CLI epilogue ran __elephc_eval_include_request_reset, whose reset_dynamic_object_contexts cleared the object-owner registry first; reset_retained_eval_contexts then called forget_request_scoped_state (drops the overlay table without releasing) and drop_eval_context_box freed the retained global scope, so $o died with no owner and forget_released_object returned early. Fix: release_retained_global_scopes_at_exit (ffi/context.rs), called from release_request_statics_at_exit (CLI only), frees the retained global scope while the registry and contexts are intact, so each object's release drains its overlay. Web keeps the arena-wipe path.

## Tried and ruled out
Moving the scope free before unregister_dynamic_object_context inside drop_eval_context_box: no effect, the registry was already cleared by the reset. property_append_fetch.rs is identical to main, so the append refusal itself is innocent.

## Watch out
Reduce leaks to the smallest eval first: the ev.php probe shrank to `class R{public $p="abc";} $o=new R();` and `$o=null` made it clean, which pointed at exit ordering instead of the object code.
