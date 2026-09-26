# Retained mbstring reproducers

This directory holds source-only PHP reproducers cited by
[`mbstring-extension.md`](../mbstring-extension.md), plus the two startup eval
ownership probes that the plan identifies as follow-up work. They are not
generated fixtures or inputs to the normal test suite. A reference in the plan
records why a probe was kept; it does not by itself establish that the current
compiler still fails that case.

## Referenced by the plan

- `detect-order-global-reference.php`
- `detect-order-eval-global-array.php`
- `eval-binary-escapes.php`
- `conversion-input-reference.php`
- `persistent-reference-nested-slot-return.php`
- `scope-destructor-parameter.php`
- `dynamic-callable-strict-mime.php`
- `test_mbstring_eval_callback_trace_ownership.php`
- `test_mbstring_eval_append_compound_source_ownership.php`
- `test_mbstring_query_ternary_value_ownership.php`

## Startup eval ownership follow-ups

- `test_mbstring_startup_eval_array_ownership.php`
- `test_mbstring_startup_eval_loop_ownership.php`

Keep compiled executables and assembly outside this source directory. Permanent
regression coverage belongs under `tests/codegen/`.
