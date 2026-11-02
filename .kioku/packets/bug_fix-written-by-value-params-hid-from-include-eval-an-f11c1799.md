---
id: bug_fix-written-by-value-params-hid-from-include-eval-an-f11c1799
type: bug_fix
title: "Written by-value params hid from include/eval, and the invoker lost the ?int tag after releasing a coerced box"
description: "privatize_*_param shadow slots are invisible to the by-name eval-scope bridge; restore_pushed_value_after_release popped one register for a two-register ?int"
created: 2026-09-26
sources:
  - path: src/ir_lower/function.rs
    blob: f8fb68a0f45f4cc686e229095404825b67b1560c
    lines: 1385-1400
    snip: 85394a4547be
    anchor: "let exposes_scope = crate::ir_lower::body_contains_eval_call(body);"
  - path: src/codegen/runtime_callable_invoker.rs
    blob: df919fa6eaa2cad092ec4215a483d68864330f57
    lines: 2596-2620
    snip: e7c2434106a2
    anchor: "fn restore_pushed_value_after_release(emitter: &mut Emitter, pushed_ty: &PhpType) {"
---

# Written by-value params hid from include/eval, and the invoker lost the ?int tag after releasing a coerced box

## Fact

Two defects found while chasing the Symfony `POST /echo` worker crash, both silent wrong answers
with deterministic reductions:

1. Written by-value parameters (`privatize_mixed_param` / `privatize_container_param`) are
   re-bound to a shadow slot named `{name}#cow`. The eval-scope bridge (`eval_scope_local_slot`)
   and the prologue find a parameter's slot BY NAME, so code the function exposes to a runtime
   `include`/`eval` read the ORIGINAL slot: `$file .= '.php'; require $dir.$file;` showed the
   included file `part`, and `$items[] = 'added'` was invisible (`a` instead of `a,added`).
   Functions whose body contains `eval` or a dynamic include now skip the privatization
   (`body_contains_eval_call`). Renaming the slots is not an option: params bind by name.
   Test: `test_runtime_include_sees_written_by_value_params`.

2. The runtime callable invoker coerces a boxed argument to a `?int` parameter as a
   payload/tag PAIR, then releases the temporary box. `restore_pushed_value_after_release` popped
   only the payload register, so the tag was whatever `__rt_decref_mixed` left in x1/rdx:
   `$class::take($n, ...)` from a function that can `require` printed `n=` for php's `n=42`.
   Test: `test_dynamic_static_call_keeps_nullable_int_tag_through_ref_cells`.

## Why

Both were silent wrong answers found while chasing a layout-dependent Symfony --web crash; each has a deterministic reduction test
