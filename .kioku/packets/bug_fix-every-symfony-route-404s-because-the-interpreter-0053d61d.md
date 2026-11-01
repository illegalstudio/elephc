---
id: bug_fix-every-symfony-route-404s-because-the-interpreter-0053d61d
type: bug_fix
title: "Every Symfony route 404s because the interpreter cannot store a closure into a compiled object's Closure-typed property"
description: "The controller that found it, five reductions that missed, and the diagnostic that did not"
tags: [symfony, magician, web, routing]
created: 2026-09-16
verified_by: "scratchpad/fccnca.php + ncafrag/frag.php against php -n; six-route diff in scratchpad/verify-routes.sh"
sources:
  - path: crates/elephc-magician/src/interpreter/statements/instance_property_access.rs
    blob: 1cdc675e8c4ecd255b76adeed47b95407711dc01
  - path: examples/symfony-app/src/Controller/HelloController.php
    blob: a2d8bfdc37bfc134d8cdc90eb43fa0b0fb649acf
---

# Every Symfony route 404s because the interpreter cannot store a closure into a compiled object's Closure-typed property

## Fact

The example app had NO controller, so every request 404ed and the byte-identical 404 everyone kept
measuring was the only response the app could produce — the one response this bug cannot break.
Adding three routes turned it into: elephc 404 on all of them, `php -S` 200 on all of them, from
the SAME cache.

Root cause, reduced to two files and confirmed by bisection:

    class AppContainer {
        protected \Closure $factory;
        final protected function getService(string $id): string { … }
    }
    // in an INTERPRETED fragment, as Symfony's container fragments are:
    $container->factory = $container->getService(...);   // unsupported Assign expression

The interpreter cannot store a CLOSURE into a declared `\Closure`-typed property of a COMPILED
object. Symfony's generated container does it on line 30 of `getRouting_LoaderService.php`
(`$container->getService ??= $container->getService(...)`), so the routing loader service dies, the
router gets no routes, and every URL 404s while the error page stays byte-perfect.

What the bisection ruled OUT, each on its own working: `??=` (a plain `=` fails the same way, only
the message changes to `unsupported Assign expression`); the first-class-callable syntax (an
ordinary `static fn` fails too); the property and method SHARING the name `getService`; a declared
uninitialized typed property on its own; a protected method's first-class callable on its own.

Five earlier reductions never reproduced it at all — a promoted parent property, a trait reaching
one, `$this` narrowed to an interface, `__DIR__` in a preloaded file, `??=` on a static-property
array element. Guessing at the shape cost more than an hour.

What worked: a temporary `ELEPHC_ROUTE_DIAG`-guarded block in `Kernel::boot()` printing the
container class, its cache dir, the router class, the matcher class and the route names — run under
`php` and under elephc, and diffed. Two builds and the failing line named itself. When a reduction
will not reproduce, make the APPLICATION talk.

One fix landed on the way and is kept: a name the compiled object has no slot for is a DYNAMIC
property, so the read consults the interpreter's overlay and answers null when the bridge refuses,
and the write falls into the overlay. It only changes paths that previously FAILED. It is not
enough on its own — the Closure-typed declared property above is a different hole.
