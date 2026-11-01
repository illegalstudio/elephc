---
id: bug_fix-an-ancestor-typed-call-to-a-wider-override-left--addb667f
type: bug_fix
title: "An ancestor-typed call to a WIDER override left its extra parameters as register garbage"
description: "Base-typed virtual calls pass the base arity; override_arity.rs routes such sites through a class-id ladder that pads the override's defaults"
created: 2026-09-26
sources:
  - path: src/ir_lower/expr/override_arity.rs
    blob: 7af7c232ae1801f8a28e4343885d0e4ed37ad5e9
    lines: 25-150
    snip: d71f048c23ce
    anchor: "pub(super) fn lower_wider_override_dispatch("
---

# An ancestor-typed call to a WIDER override left its extra parameters as register garbage

## Fact

A virtual call pads arguments from the signature of the class the RECEIVER is typed as. When a
subclass overrides the method with MORE parameters (all defaulted), a base-typed call passes the
base arity and the override reads its extra parameters from registers the caller never set.

Symfony: `Container::make()` calls `$container->load($file)` against `Container::load(string
$file)`; the generated container overrides `load($file, $lazyLoad = true)`. `$lazyLoad` arrived as
a leftover stack address, the dynamic `$class::do($this, $lazyLoad)` built a reference cell to it,
and `__rt_str_persist` faulted on string bytes read as a pointer. It crashed `POST /echo` in about
one environment size in five (see the memory "layout-dependent crash: sweep env").

Fix: `lower_wider_override_dispatch` (override_arity.rs). For every concrete class at or below the
receiver's static class whose method takes more defaulted by-value parameters than the site's
signature, the site compares `ObjectClassId` against the class id, re-types the receiver with
`Op::Move` to that class, and lowers the call through `lower_method_call_with_receiver`, which
pads the override's own defaults. Other classes keep the ordinary call.

Why not a vtable thunk at the ancestor arity: sites typed as the SUBCLASS use the same slot and
pass the full argument list, which a narrower thunk would drop.

Not covered: eval-declared subclasses (outside the closed world), variadic or by-ref extra
parameters, owning-temporary receivers (`(new X)->m()` keeps the old path).

## Why

It crashed Symfony --web workers (Container::load's $lazyLoad) only in some memory layouts; a vtable thunk cannot fix it because subclass-typed sites share the slot
