---
id: bug_fix-an-omitted-by-reference-parameter-gets-a-discard-95d1a4c8
type: bug_fix
title: "An omitted by-reference parameter gets a discarded cell in the backend, which is the only place that can see all the candidates"
description: "The mixed-receiver half of the omitted by-ref chain: caller-side IR padding cannot work when param_types come from each candidate, so the fill loop in reference_arguments.rs now stages a null-seeded discarded cell"
created: 2026-09-17
verified_by: "codegen::arrays+codegen::regressions 33 failures (baseline); error_tests 99 failures with names identical to baseline, 1430 -> 1431 passed; the two-candidate fixture matches php -n exactly"
sources:
  - path: src/codegen/lower_inst/reference_arguments.rs
    blob: 31ded2c8b1ff2c19365a368dd460a16744e140bb
  - path: src/codegen/lower_inst/method_call_types.rs
    blob: 88ea402126cb98a331ac6f2564e59a103a3e6f07
  - path: tests/codegen/regressions/param_inference.rs
    blob: 0f4e1096072aac6a245a60187ea9eff18805f7e6
supersedes: "gotcha-an-omitted-by-reference-parameter-needs-a-place--540ab304"
---

# An omitted by-reference parameter gets a discarded cell in the backend, which is the only place that can see all the candidates

## Fact

`pad_omitted_by_ref_args` (IR) closed this for a receiver whose class is known: it synthesises the
missing argument as a local seeded with the parameter's default. That approach CANNOT be extended
to a gradual receiver. `lower_mixed_method_candidate` builds `param_types` and `ref_params` from
EACH CANDIDATE's own signature at dispatch time, so one candidate's third parameter is another's
second, and a single padded argument list is wrong for whichever candidate it was not written for.

The backend is the only place that sees one candidate at a time, so the fix went there:

- `RefArgTempCell::source_value` became `Option<ValueId>`. `None` means an OMITTED argument: there
  is no operand to load, only an address to hand over.
- `plan_ref_arg_temp_cells` gained a second loop over `args.len()..param_types.len()`. Every other
  materializer asserts one argument per parameter, so that range is empty for them; only the
  receiver-register path can be short.
- `emit_ref_arg_cell_block` seeds a `None` cell with null (`emit_omitted_ref_cell_seed`: boxed null
  for Mixed, the tagged null sentinel for TaggedScalar, zero for Int/Bool/False, a refusal for
  anything else rather than a zero that the release loop would hand to a refcount helper).
- The `operands.len()..param_types.len()` fill loop stages the cell's ADDRESS instead of refusing.

THE ACCOUNTING THAT BITES: that fill loop used to push arguments WITHOUT advancing
`arg_temp_bytes`, because nothing read it afterwards. A by-reference address is
`arg_temp_bytes + cell_offset` — the cell block sits below every staged argument — so the loop now
advances it per push. Getting this wrong points the callee at another argument's stack slot.

`MayOutliveCall` still plans no cells (a caller-stack cell there is a use-after-free for a
constructor-promoted property), so an omitted by-reference parameter on a dynamic `new $cls(...)`
still refuses, deliberately, with a named error.

HOW THE TEST WAS PROVEN TO COVER IT: making `materialize_omitted_ref_arg_address` return an error
fails `test_a_gradual_receiver_may_omit_a_by_ref_parameter` (two classes whose `&$seen` sits at
DIFFERENT positions) and does NOT fail `test_an_omitted_by_ref_parameter_gets_a_place` — the
single-candidate fixture is carried by the IR padding instead. A one-candidate gradual receiver is
NOT a regression test for this path.

Symfony reached it at `HtmlErrorRenderer::getAndCleanOutputBuffer`,
`$request->headers->get('X-Php-Ob-Level', -1)` over a gradual `headers`.

## Why

It was the head of the Symfony --web backend chain
