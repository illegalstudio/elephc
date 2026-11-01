---
title: "Closed-world console: reflection of closures, Closure::fromCallable at run time, memo replay"
author: Guillaume
date: 2026-10-24
branch: reconcile/dirname-symfony
---

## Decided
- `new ReflectionMethod($closure, '__invoke')` builds a ReflectionFunction of the closure and copies its shape through two internal accessors (`__elephc_invoke_shape()` ints, `__elephc_invoke_type()` object — one array typed the object as int 0). Parameters get mode -1 (`__parameter_mode`): no default shown, function `__invoke`, class `Closure`, as php does for an internal method (member_table_ctor.rs).
- A `$this->m(...)` descriptor on a known receiver class is named `Class::m`, like the runtime-dispatched templates; `object::m` named no class for ReflectionFunction (callable_descriptors.rs).
- Variadic parameters never carry HAS_DEFAULT in the reflection rows (member_table.rs::parameter_row).
- `['Closure', 'fromCallable']` at run time: prelude class `__ElephcClosureFactory::fromCallable(mixed)` (closure_from_callable_prelude.rs), injected when a string literal spells `fromCallable`; codegen adds a `Closure::fromCallable` row to the runtime static table over it; the class is an all-access scope in `scope_reaches_member` (php checks the CALLER's access, which the helper cannot know). Roots: ast_usage AND optimize/reachability expressions.rs, plus the early-return list in backend_gap_prelude::inject_if_used.
- `\Closure::fromCallable($mixed)` lets a two-element array through to the scope-aware conversion: the runtime `is_callable()` sees public methods only.
- Deferral memo replays apply the probe-only policy (pipeline.rs `declares_only_probed_classes`): a rebuild used to warn and refuse the closed-world build.

## Tried and ruled out
- A free-function helper for fromCallable: no scope, protected `[$container, 'getEnv']` refused. A `callable`-typed parameter: resolved at the call boundary, outside the helper's scope.
- phpdbg breakpoints driven over stdin to list factories: it blocks. A scratch copy of the app with one logging line in its ContainerBuilder worked (never edit the real vendor).

## Watch out
- `php -S` ignores the process environment for `$_SERVER` unless `-d variables_order=EGPCS`: sfroutes compared the dev debug 404 page against the prod binary.
- A backend_gap prelude flag must also be added to the early-return list in inject_if_used, or nothing is injected.
