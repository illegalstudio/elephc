---
id: bug_fix-spreading-a-callable-materialize-before-the-unbo-189e74cf
type: bug_fix
title: "Spreading a callable: materialize BEFORE the unbox, never instead of it"
description: "How ...$callable is lowered, the five sites it needed, and the three regressions the first design caused"
tags: [spread, callable, symfony]
created: 2026-09-16
verified_by: "four reduction probes byte-compared against php -n; examples/symfony-app compiling from a PRISTINE vendor tree; the three regressed codegen tests passing again"
sources:
  - path: src/codegen_support/runtime/callables/spread_array.rs
    blob: 64b45e5ca7f3dd7188cbf4277a0b74380a9675c5
  - path: src/ir_lower/expr/positional_spreads.rs
    blob: 7f3f5e493094cd0cf53e69d62bd42e6503c0f4c4
  - path: src/ir_lower/expr/indexed_array_literals.rs
    blob: 88becb17d0574f9e13c895e4fd72adab3bd0d7e6
---

# Spreading a callable: materialize BEFORE the unbox, never instead of it

## Fact

A `callable` slot holds a DESCRIPTOR, so `...$c` had nothing to unpack.
`__rt_mixed_spread_array` is **Mixed -> Mixed**: it replaces ONLY a descriptor whose kind is ARRAY
with a boxed two-slot array (the bound receiver from runtime capture 0, and the bare method name),
and returns every other cell untouched. `narrow_gradual_indexed_spread_source` runs it and THEN
the ordinary `MixedUnbox(4)`.

**Do not replace the unbox with it.** `lower_mixed_unbox` does more than read the payload: for an
`array<mixed>` result it calls `emit_clone_indexed_array_for_invoker_with_runtime_tag`, giving the
invoker its own tagged copy, and on a wrong tag it raises PHP's
"Only arrays and Traversables can be unpacked, X given". A replacement helper that skipped the
clone broke `test_gradual_array_spread_into_dynamic_method_call` with
`call_user_func_array(): missing required argument` — the other caller,
`lower_indexed_array_spread_into_array`, is on that path.

Five sites had to change, and missing any one left a different failure:
1. `infer_call_argument_type` (ops.rs) — the ordinary call site.
2. `calls_objects.rs` — the CONSTRUCTOR site. `new \ReflectionMethod(...$c)` fails here, not there.
3. `effects.rs` — the assignment-effects site.
4. `indexed_spread_source_type` (positional_spreads.rs) — accept `Callable`, and accept a PROPERTY
   READ as gradual: syntactic inference cannot see a declared property type, so
   `...$this->controller` used to leave the spread path and lower as one operand.
5. `narrow_gradual_indexed_spread_source` — run the materializer before the unbox.

Accepting property reads also exposed the min-length guard to hash storage, and `ArrayLen` on a
`Heap(Hash)` fails EIR validation outright, so that guard now skips a shape it cannot measure.
