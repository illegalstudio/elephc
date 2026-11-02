---
id: gotcha-the-runtime-closure-bind-does-not-write-the-new--82153f10
type: gotcha
title: "The runtime Closure::bind does not write the new receiver into the compiled capture slot"
description: "Typing a bound closure's $this Mixed on both the checker and backend sides compiles and then fatals with an empty class name, so the missing piece is the runtime bind, not the type"
created: 2026-09-17
sources:
  - path: src/ir_lower/expr/static_method_calls.rs
    blob: 387d3f1dd8c378b9200f10dd217b0a03c2b7eb02
  - path: src/ir_lower/expr/closures.rs
    blob: 8d3836d616cbe357631227ea0321ada5889f8c7e
  - path: src/types/checker/inference/objects/methods.rs
    blob: a648e490a426510150f2c261527fecf7315cd538
supersedes: "gotcha-closure-bind-rebinds-this-only-for-a-return-a-pr-5e0d56f7"
superseded_by: "bug_fix-closure-bind-boxed-the-box-and-both-type-sides-k-40d71c19"
superseded_why: "Wrong culprit: the helper was right and its caller handed it a boxed value; fixed, along with both type sides together"
---

# The runtime Closure::bind does not write the new receiver into the compiled capture slot

## Fact

`closure_bind_property_receiver_type` rebinds `$this` only for a closure body of exactly
`return $this->prop;`, and `build_bound_closure_binding` does the same on the backend side — "the
only form whose `$this` is fully known at compile time". Every other body keeps the ENCLOSING
class in both, which is the one thing a bound `$this` is definitely not. Symfony's `RedisTrait`
line 70 is the standing failure:

    \Closure::bind(function () { $this->options['exceptions'] = false; }, $options, $options)();
    → Undefined property: Symfony\Component\Cache\Adapter\RedisAdapter::options

TRIED AND BACKED OUT, both halves together so they could not disagree: type a bound `$this`
`Mixed` when the target names no class — in the checker, and in lowering by setting a flag around
the closure literal that makes the `this` capture `Mixed` (the same answer the arm beside it
already gives a top-level closure with no enclosing `$this`). It got past the checker AND past
EIR, and the program then fataled:

    Warning: Undefined property: ::$options
    Fatal error: Cannot write undeclared property $options through a mixed receiver

The EMPTY class name is the finding: the closure's `this` capture is not the bound object, so the
runtime `Closure::bind` does not put the new receiver into the compiled capture slot. Visibility
is not involved — a public property behaves identically. Fix THAT first; until then a
compile-time refusal beats a runtime fatal on code PHP runs.

## Why

Saves the next session from re-deriving the same dead end, and names the actual missing piece
