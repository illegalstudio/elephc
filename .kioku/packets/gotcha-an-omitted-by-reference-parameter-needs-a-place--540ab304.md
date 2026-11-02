---
id: gotcha-an-omitted-by-reference-parameter-needs-a-place--540ab304
type: gotcha
title: "An omitted by-reference parameter needs a place, and the mixed-dispatch path still has none"
description: "Padding omitted by-ref parameters with a synthetic local fixed the nominal-receiver case, but the mixed-receiver candidate dispatch builds param_types per CANDIDATE in codegen, so the caller cannot pad for all of them"
created: 2026-09-17
verified_by: "A reduction with ?array &$used = null omitted now compiles and matches php -n; error_tests stayed at its 99-failure baseline with identical names"
sources:
  - path: src/ir_lower/expr/nullable_method_calls.rs
    blob: 70f7b687adc65b18c965015d82acc26c03980f6d
  - path: src/ir_lower/expr/method_calls.rs
    blob: 98c1564b3eb85304a99a401d4bde3bbe62250efb
  - path: src/codegen/lower_inst/reference_arguments.rs
    blob: e63092c59b7f4812202988320a350549ced78b1b
  - path: src/codegen/lower_inst/method_dispatch.rs
    blob: a353c8c12f5d7675b3978d226b5c7320d663cd39
superseded_by: "bug_fix-an-omitted-by-reference-parameter-gets-a-discard-95d1a4c8"
superseded_why: "The mixed-receiver half it described as STILL OPEN is now fixed in the backend fill loop"
---

# An omitted by-reference parameter needs a place, and the mixed-dispatch path still has none

## Fact

PHP lets a by-reference parameter carry a default and a caller omit it:

    ContainerBuilder::resolveEnvPlaceholders(mixed $value, $format = null, ?array &$usedEnvs = null)
    $this->container->resolveEnvPlaceholders($value, $this->format);      // two arguments

The callee writes through `&$usedEnvs` and the caller never looks. elephc had nowhere to put those
writes: the default-argument loop materializes a VALUE, which is not a cell.

FIXED for a receiver whose class is known. `pad_omitted_by_ref_args` declares a synthetic local
(`declare_synthetic_php_local`, the same primitive `prepare_ref_place_args` uses), seeds it with the
parameter's default, and passes it positionally — after which every existing by-reference mechanism
applies unchanged. It is called from BOTH operand-assembly paths, `lower_method_call` and
`lower_method_call_with_receiver`; they do not share one.

STILL OPEN for a MIXED receiver. `lower_mixed_method_candidate` (method_dispatch.rs) builds
`param_types` and `ref_params` from EACH CANDIDATE's signature at dispatch time, so the caller
cannot pad for all of them at once — one candidate's third parameter is another's second. The
refusal is `receiver-register method call with missing non-value parameter`
(reference_arguments.rs, the `operands.len()..param_types.len()` loop), and Symfony reaches it at
`HtmlErrorRenderer::getAndCleanOutputBuffer`'s closure, `$request->headers->get('X-Php-Ob-Level', -1)`
over a gradual `headers`.

The fix belongs in that loop: for an omitted by-reference parameter, reserve a scratch cell and
push its address, the same way `plan_ref_arg_temp_cells` does for a supplied one. The accounting to
be careful with is `arg_temp_bytes`, which the loop advances per pushed argument.

## Why

It is the current head of the Symfony --web backend chain
