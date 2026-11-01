---
id: bug_fix-a-by-ref-parameter-captured-by-use-p-must-be-wid-d7edd023
type: bug_fix
title: "A by-ref parameter captured by use(&$p) must be widened AND its omitted cell must outlive the call"
description: "Closure by-ref capture of a by-ref param: boxed cell vs raw caller cell, and stack temp cells that die before the closure runs"
created: 2026-09-26
sources:
  - path: src/types/checker/ref_param_widening.rs
    blob: 39183df1172630dc6742ef6a6db146dc98f05cb7
  - path: src/codegen/lower_inst/method_call_types.rs
    blob: 4a8ce97ff1f14df5c686b69be24020035efb4578
  - path: src/ir_lower/expr/nullable_method_calls.rs
    blob: 1aa70ed7e1af7581044f1c8a21df496e58a7ed30
---

# A by-ref parameter captured by use(&$p) must be widened AND its omitted cell must outlive the call

## Fact

Three separate defects, one shape: `function f(array &$p = []) { return function () use (&$p) {...}; }`.

1. EIR lowering gives a by-ref CAPTURE a boxed Mixed cell, so the body reads the param as Mixed, while the signature (and every caller) said `array`. A string-keyed write inside the closure stored a box the caller read as a raw array. Fix: `scan_widened_ref_params` seeds the widened set with by-ref params named in any closure's `capture_refs` (walked via `CallTarget::RefCapture` in `for_each_call`).
2. An OMITTED by-ref argument got a caller-STACK temp cell (`RefArgCellLifetime::CallOnly`). The closure outlives the call, so the cell dangles. Fix: the checker publishes `ref_param_escaping_callees` (lowercase bare names, captured params propagated through by-ref forwarding); codegen's `RefArgCellLifetime::for_callee` picks `MayOutliveCall` (heap cell) and `materialize_temporary_ref_arg_cell` now increfs the unconverted value (the caller releases its operand right after the call).
3. Method calls pad an omitted by-ref arg with a SYNTHETIC FRAME LOCAL (`pad_omitted_by_ref_args`), which is the caller frame again. For an escaping callee the default expression is passed instead so the backend owns the cell (thread-local `ref_param_may_escape`, set per program lowering).

Still open: a caller's own frame LOCAL passed by reference to an escaping callee (`function c() { $x = []; return mk($x); }`) dangles the same way -- the local would need promoting to a heap ref cell before the call. Probes: scratchpad refcap*.php, resolvegen2.php.

## Why

Symfony ContainerBuilder::doResolveServices returns a RewindableGenerator over a closure capturing &$inlineServices; lint:container read freed stack
