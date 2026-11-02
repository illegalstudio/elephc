---
id: bug_fix-a-cached-eval-class-constant-cell-outlived-the-r-ba936d2c
type: bug_fix
title: "A cached eval class-constant CELL outlived the request that allocated it, and the second / request died on a parameter default"
description: "forget_declared_class_likes dropped the declarations but kept class_constants, whose handles point into the wiped request heap"
created: 2026-09-18
verified_by: "index_preload_hotpath served / on request 1 and answered ControllerDoesNotReturnResponseException on request 2, every time, for as long as the app has existed; after the fix the same worker serves / repeatedly"
sources:
  - path: crates/elephc-magician/src/context/classes_aliases.rs
    blob: 887775fecadd44e0093a46dc540aed87a892b205
  - path: crates/elephc-magician/src/context/runtime_state.rs
    blob: f751c62f6ccf5b2d7df8227d11d5acac7542b9cc
---

# A cached eval class-constant CELL outlived the request that allocated it, and the second / request died on a parameter default

## Fact

`ElephcEvalContext::class_constants` is a cache of `(class, constant) -> RuntimeCellHandle`. The
handles point into the REQUEST heap, which `__rt_web_reset` wipes at every request boundary, and
`forget_declared_class_likes` -- the reset hook that drops everything the previous request declared
into the process-lifetime null-handle context -- cleared the class tables but NOT that cache.

So request 2 re-declared the class and then read a DANGLING constant cell. `twig/twig`'s

    public function addPath(string $path, string $namespace = self::MAIN_NAMESPACE): void

could not materialize its default, the bind threw before the body ran, the generated Symfony
container's own `catch (\Exception)` swallowed it, `$container->get('twig')` answered null, and
`AbstractController::render()` returned null -- so HttpKernel reported "the controller must return a
Response object but it returned null", with nothing in the log naming the real cause.

THE FIX: drop the constant cells with the declarations (`class_constants.clear()` and
`evaluating_class_constants.clear()` in `forget_declared_class_likes`). The same file's own comment
already explains the rule for method tables; the constant cache was the piece that did not follow
it. `enum_cases` and `enum_case_values` were already cleared there -- they hold handles too.

HOW IT WAS FOUND: `ELEPHC_EVAL_TRACE=1` over two requests. The trace's
`phase=dynamic_method_bind_error` line names the method, the argument tags and the parameter list;
it now also names the THROWABLE's class, which is what separates "an argument did not match" from
"a default did not materialize".

## Why

It is the last thing between the compiled Symfony app and a Twig route that works on every request
