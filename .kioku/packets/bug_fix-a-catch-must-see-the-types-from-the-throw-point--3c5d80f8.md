---
id: bug_fix-a-catch-must-see-the-types-from-the-throw-point--3c5d80f8
type: bug_fix
title: "A catch must see the types from the throw point, not the ones the try body left"
description: "Why ClassStub refused to compile, the two regressions the naive fix caused, and the rule that holds"
tags: [checker, control-flow, symfony]
created: 2026-09-16
verified_by: "examples/symfony-app max preload 6 -> 5 checker errors, two new codegen regression tests, probes byte-compared against php -n"
sources:
  - path: src/types/checker/stmt_check/control_flow.rs
    blob: 75c5be591b626dfa9b9cb90d207fbba654668c13
---

# A catch must see the types from the throw point, not the ones the try body left

## Fact

`StmtKind::Try` checked the try body by MUTATING `env` in place and then handed the catches that
same env — i.e. it told them every assignment in the try completed, which is the one thing that
cannot be true of the statement that threw.

    if (\is_array($r)) {
        try   { $r = new \ReflectionMethod($r[0], $r[1]); }
        catch (\ReflectionException) { $r = new \ReflectionClass($r[0]); }   // Cannot index non-array
    }

Symfony's `ClassStub::__construct` is written exactly that way.

THE RULE, after two regressions found by widening the preload:
1. Join the environment captured BEFORE each statement of the try — those are the points an
   exception can be raised from. Do NOT include the env after the last statement: if every
   statement completed, nothing threw.
2. A name the try INTRODUCED still reaches the catch with the try's type. PHP binds a BY-REFERENCE
   argument at the call, before the callee can raise — `HttpKernel::handle` passes
   `$controllerMetadata` that way. Missing this gave `Undefined variable: $controllerMetadata`.
3. Join every catch scope back on the way out, alongside the try's own exit. PHP leaves the
   exception variable defined after the statement, and so are the catch body's assignments.
   Missing this gave `Undefined variable: $e` in `ContainerControllerResolver`.

Known residual, deliberately accepted: a statement that completes a nested assignment and only
then raises (`foo($r = 1, throws())`) reaches its catch with the pre-statement type.

SEPARATE, PRE-EXISTING divergence found while testing this, do not mistake it for the fix: after a
try that did NOT throw, elephc treats names bound only in the catch as DEFINED.
`try { ok(); } catch (B $e) { $tag = 1; } echo $tag ?? '-';` prints the empty string where php
prints `-`. Confirmed by A/B against the unpatched checker.
