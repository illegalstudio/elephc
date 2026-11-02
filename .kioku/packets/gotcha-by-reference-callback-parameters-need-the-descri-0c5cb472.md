---
id: gotcha-by-reference-callback-parameters-need-the-descri-0c5cb472
type: gotcha
title: "By-reference callback parameters need the descriptor invoker, and that is what blocks the Symfony binary"
description: "array_walk over gradual elements now works by value, but a by-reference callback parameter cannot work until the per-closure descriptor invoker passes cell addresses; the same machinery is what the spread-into-by-ref-callable error wants"
created: 2026-09-17
verified_by: "By-value walks over array<mixed> now byte-match php -n; the by-reference form is refused in the checker rather than dropping writes, which it did when the gate was merely widened"
sources:
  - path: src/codegen/lower_inst/builtins/arrays/reduce_sets.rs
    blob: d737417e9c6288d654c64df7ffff941b1820c6e7
  - path: src/codegen/lower_inst/core_closures.rs
    blob: f99c0a197f6f2ae17207dd56862f6bc0214aa69e
  - path: src/codegen/lower_inst/callables.rs
    blob: fe23cb832a644848b55baeaa56a38703fb2490a2
  - path: src/builtins/array/array_walk.rs
    blob: cccb17d13371035e04ead478c77567db43a68720
supersedes: "gotcha-array-walk-has-two-gaps-and-blocks-the-symfony-w-1c1fb7c8"
---

# By-reference callback parameters need the descriptor invoker, and that is what blocks the Symfony binary

## Fact

FIXED: `array_walk()`'s element gate was the shared `eight_byte_callback_value_type`, which admits
only `Int`/`Bool`. A declared bare `array` parameter is `array<mixed>`, so the most ordinary
spelling there is had never compiled. `array_walk_element_type` now admits anything whose runtime
payload is ONE 8-byte slot — boxed `Mixed` and object handles — the same reasoning
`indexed_sort_element_type` applies to the permuting sorts. `Str` stays out: a multi-word
descriptor is not one slot.

STILL OPEN, and it is a feature: a BY-REFERENCE callback parameter.

    array_walk($filesInfo, static function (&$v) use (&$errors) { … });   // LintCommand::displayJson

Widening the gate alone made this compile and DROP every write — `2,4,6` from `php -n`, `1,2,3`
from elephc — so the checker now refuses it (`src/builtins/array/array_walk.rs`). A refusal is the
honest answer until the write-back exists.

What it would take, all four pieces confirmed:

1. `__rt_array_walk_ref`: the same loop handing the callback `&slot[i]` instead of `slot[i]`. One
   instruction different per target (`add x0, x1, x20, lsl #3` / `lea rdi, [r10+r13*8+24]`). The
   full emitter is in this session's scratchpad as `patch_walk_ref_rt.py`. Written, verified to
   build, removed again rather than ship unwired.
2. elephc ALREADY compiles a by-reference parameter as a cell pointer — the closure body uses
   `load_ref_cell` / `store_ref_cell`, which release the old value and retain the new one — so
   the closure needs no change, boxed `Mixed` elements included.
3. The blocker: a closure literal is `Op::ClosureNew`, which `require_static_callback_source` does
   not accept, so `array_walk` always takes the DESCRIPTOR wrapper. `lower_closure_new` builds a
   per-closure invoker (`emit_runtime_callable_invoker_inline`) that takes an ARGUMENT CONTAINER,
   by value. `codegen/lower_inst/callables.rs` even filters by-ref functions out of runtime-string
   dispatch (`!param.by_ref && !param.variadic`) — by-reference through a descriptor is simply not
   implemented.
4. Symfony's callback also captures `use (&$errors)`, so a direct call to the closure's compiled
   entry would still need the capture environment the descriptor holds (entry at descriptor+8,
   capture values from the capture slots).

So the real unit of work is "by-reference arguments through the descriptor invoker", which is also
what `index_preload_hotpath.php`'s one remaining error wants: "callable $callback cannot be invoked
with spread arguments when it has pass-by-reference parameters".

## Why

It is the single thing between a zero-error Symfony --web type-check and a linked binary
