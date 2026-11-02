---
id: gotcha-the-console-hits-at-least-two-premature-frees-an-b8e85e8a
type: gotcha
title: "The console hits at least two premature frees, and narrowing the first only uncovers the second"
description: "eval_method_receiver_is_temporary treats any method call as a disposable receiver so a chained container service is freed; narrowing it to constructions exposes a second free in the statement dispatcher inside a closure, so the narrowing was reverted"
created: 2026-09-22
verified_by: "two lldb backtraces on the compiled Symfony bin/console, before and after narrowing the predicate"
sources:
  - path: crates/elephc-magician/src/interpreter/expressions.rs
    blob: 46f8024e2bf651eac3a546b9a5adcee91d39d1c7
    lines: 660-676
    snip: b74386ae48da
    anchor: "fn eval_method_receiver_is_temporary(expr: &EvalExpr) -> bool {"
  - path: crates/elephc-magician/src/interpreter/statements/dispatch.rs
    blob: e9bb12f43ce430f22c9a13c0f50ee4e350db1af0
    lines: 16-60
    snip: fe820991693c
    anchor: "pub(in crate::interpreter) fn execute_statements("
---

# The console hits at least two premature frees, and narrowing the first only uncovers the second

## Fact

The compiled Symfony console hits MORE THAN ONE premature free, and removing the first only
uncovers the next. Both were named with `--heap-debug --keep-symbols` plus
`lldb -o "breakpoint set -r heap_debug_fail" -o run -o "bt 25"`.

SITE 1, with the shipped predicate:

    _rt_object_free_deep
    _rt_mixed_free_deep
    interpreter::statements::array_updates::eval_release_value
    interpreter::expressions::eval_method_call_with_temporary_receiver_cleanup
    interpreter::expressions::eval_expr  (deeply recursive)

`eval_method_receiver_is_temporary` calls a receiver DISPOSABLE when it is any method call --
`MethodCall`, `DynamicMethodCall`, the nullsafe pair, the static pair -- alongside the genuine
constructions (`NewObject`, `Clone`, anonymous class). A chained `$container->get('x')->m()` then
releases a SERVICE the container still holds.

SITE 2, reached by narrowing that predicate to constructions only:

    _rt_object_free_deep
    _rt_mixed_free_deep
    interpreter::statements::array_updates::eval_release_value
    interpreter::statements::dispatch::execute_statements
    interpreter::dynamic_functions::closure_execution::eval_closure_with_optional_binding

The statement dispatcher releases a discarded expression result inside a closure body, and that
result is likewise still owned elsewhere.

THE NARROWING WAS REVERTED. It removes releases, so it cannot have CREATED site 2 -- but it did
not fix the crash either, and shipping it would trade a corruption for a leak and buy nothing.
Measurement, not plausibility, is what a change here has to answer to.

This matches the shape already recorded as "Eval borrowed-cell ownership: one predicate, four
sites, three different right answers". The interpreter has no per-call ownership transfer
information, so every one of these sites is guessing whether the value it holds is borrowed.

## Why

It stops the next session from shipping the obvious narrowing, which trades a corruption for a leak and fixes nothing
