---
id: bug_fix-reflection-of-the-builtin-closure-listed-no-meth-c8d01835
type: bug_fix
title: "Reflection of the builtin Closure listed no methods, and getMethod never walked a compiled ancestor of an eval class"
description: "lint:container reflects the [Closure, fromCallable] factory and then setName on interpreted commands; both answered 'does not exist' while hasMethod said true"
created: 2026-09-26
sources:
  - path: crates/elephc-magician/src/interpreter/reflection/builtin_closure.rs
    blob: 7a24d365549e6bbfe6ec2e377bcfcb97639b3809
  - path: crates/elephc-magician/src/interpreter/reflection/class_member_api.rs
    blob: e3b2b089ea5e0b1793a0331273d3cfea7e9cb1b1
  - path: crates/elephc-magician/src/interpreter/statements/method_dispatch.rs
    blob: a9ab8859a75a9ec21fee744e5f26e4470c5b2ed1
---

# Reflection of the builtin Closure listed no methods, and getMethod never walked a compiled ancestor of an eval class

## Fact

Closure: builtin_closure.rs holds php 8.5's seven methods (private __construct, static bind,
bindTo, call, static fromCallable, static getCurrent, __invoke) with typed parameters and
defaults. It feeds: the class object's __method_names/__methods (class_construction +
owner_materialization), eval_reflection_method_metadata, eval_reflection_member_name,
getMethods, hasMethod and ReflectionMethod construction. The compiled ReflectionClass only
reaches those interpreter handlers once its identity is BOUND, and the rebind
(eval_rebound_reflection_class_name) needed an explicit Closure case because Closure is in
neither the eval nor the AOT registry; unbound, getMethods read the compiled reflector's empty
slots.

Inherited method: ReflectionClass::getMethod on an eval class whose method comes from a
compiled parent (AboutCommand::setName from Command) only searched the eval class's own names.
It now falls back to eval_reflection_method_object_result_if_exists, which already walks the
compiled ancestry, as hasMethod and new ReflectionMethod did.
Pin: tests/codegen/eval.rs test_eval_class_get_method_reaches_its_compiled_parent_and_closure_methods.
Still open: getMethods() counts for other builtins (ArrayObject 9 of 25, Exception 9 of 11), and
a compile-time `new ReflectionMethod('Closure', ...)` literal is refused by the checker.

## Why

hasMethod, getMethod, getMethods and new ReflectionMethod each resolve members through a different path; fixing one leaves the others lying
