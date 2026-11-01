---
id: bug_fix-throw-of-a-value-typed-by-an-absent-class-was-re-32db159c
type: bug_fix
title: "throw of a value typed by an ABSENT class was refused, and the existing escape hatch only covered throw new"
description: "A declared return type naming a class no installed package supplies made the checker decide it does not implement Throwable and refuse the whole file"
created: 2026-09-18
verified_by: "tests/codegen/regressions/syntax_edges.rs::test_a_throw_of_an_absent_declared_exception_type_defers_to_runtime; error_tests stayed at 1431 passed / 99 failed with identical names"
sources:
  - path: src/types/checker/type_compat/object_types.rs
    blob: 9d65a61bb1427d8ee7d3e5cc2323b2ec7d1d627f
  - path: src/types/checker/stmt_check/control_flow.rs
    blob: dc8ae5b013c25fd859cb27730ee238a72ae17bd7
  - path: src/types/checker/inference/expr/basic.rs
    blob: 411c0a03eeb1880537a7562011001a010556fc71
---

# throw of a value typed by an ABSENT class was refused, and the existing escape hatch only covered throw new

## Fact

    error[.../framework-bundle/Controller/AbstractController.php:249:13]:
    Type error: throw requires an object implementing Throwable

`denyAccessUnlessGranted` does

    $e = $this->createAccessDeniedException(...);   // declared `: AccessDeniedException`
    $e->setAttributes([$attribute]);
    ...
    throw $e;

and `Symfony\Component\Security\Core\Exception\AccessDeniedException` only ships with
`symfony/security-core`, which this app does not install. The method GUARDS itself with
`class_exists()` and throws a LogicException when the package is missing, so PHP compiles the file
and an app without the package never runs the branch.

The checker read the absent name as an ordinary concrete type, asked whether it implements
`Throwable`, got false, and refused. `Checker::unresolved_new_object_defers_to_runtime` already
covered this, but ONLY when the expression being thrown is the `new` itself -- it pattern-matches
`ExprKind::NewObject`. A declared return type carries the name just as far and matched nothing.

FIX: `Checker::absent_class_defers_to_runtime(type_name)` -- the name is non-empty, neither
`self.classes` nor `self.interfaces` has it, and `allows_absent_runtime_class()` is true for the
current context -- added as a third alternative in BOTH throw checks (the statement in
`stmt_check/control_flow.rs` and the expression in `inference/expr/basic.rs`).

REDUCTION, 20 lines, compiles and runs under `php -n` and now under elephc:

    class Box {
        public function createDenied(string $m): \Absent\Vendor\DeniedException {
            if (!class_exists(\Absent\Vendor\DeniedException::class)) {
                throw new \LogicException('the absent/vendor package is not installed');
            }
            return new \Absent\Vendor\DeniedException($m);
        }
        public function deny(string $m): void { $e = $this->createDenied($m); throw $e; }
    }

This is another instance of the shape already recorded as "a class without its dependencies gets a
fabricated type": an absent class-like must stay UNKNOWN, never become a concrete type the checker
can answer questions about. The same probe turned up a second live instance that is NOT fixed:
`foreach ($routes as ...)` where `$routes` is a parameter typed `RouteCollection` reports
"requires ... to implement Iterator or IteratorAggregate outside its declaring scope" when the
class is absent from the closed world -- and a parameter type hint is deliberately NOT an autoload
reference point (`reference_points_defer_named_callable_signature_types`), so absent is a normal
state for it.

## Why

It blocked compiling symfony/framework-bundle's AbstractController, which is on the critical path of any compiled Symfony controller
