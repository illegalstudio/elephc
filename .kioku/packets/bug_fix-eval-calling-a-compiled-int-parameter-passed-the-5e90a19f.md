---
id: bug_fix-eval-calling-a-compiled-int-parameter-passed-the-5e90a19f
type: bug_fix
title: "Eval calling a compiled ?int parameter passed the box's address as the int"
description: "The runtime callable invoker could not coerce Mixed to TaggedScalar, so it pushed the boxed pointer where the callee reads an inline int|null pair"
created: 2026-09-26
sources:
  - path: src/codegen/runtime_callable_invoker.rs
    blob: 20e4f01a55279aabb7b346f0a47c4bac863873a1
---

# Eval calling a compiled ?int parameter passed the box's address as the int

## Fact

`function f(?string $s, ?int $n = 1)` compiled, called from eval as f("abc", 2): $n arrived as
4350032560. `?int` lowers to PhpType::TaggedScalar (payload x0/rax + tag x1/rdx), which
can_coerce_result_to_type did not list, so coerce_current_value_to_target kept the Mixed repr
and pushed the box pointer.

Fix: Mixed -> TaggedScalar unboxes (tag 8 -> emit_tagged_scalar_null, else __rt_mixed_cast_int
+ emit_tagged_scalar_from_int_result), and Int -> TaggedScalar just tags. The method-call bridge
already had the same conversion (emit_aarch64_cast_eval_tagged_scalar_arg in
src/codegen/eval_method_helpers.rs); the function invoker did not.
Pin: tests/codegen/eval.rs test_eval_call_passes_a_nullable_int_argument_to_a_compiled_function.
Watch: any NEW codegen_repr added to PhpType must be added to can_coerce_result_to_type, or the
invoker passes it raw with no error.

## Why

Every compiled function with a nullable int parameter called from eval read garbage; can_coerce_result_to_type silently falls back to the source repr
