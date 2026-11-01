---
id: bug_fix-every-symfony-route-404s-because-eventdispatcher-59288a7a
type: bug_fix
title: "Every Symfony route 404s because EventDispatcher::optimizeListeners miscompiles twice"
description: "A by-reference foreach over a HASH read boxed storage as its declared type, and $x = &$arr[] reused one ref cell per source position, so every event ran its LAST listener over and over"
created: 2026-09-17
verified_by: "A faithful optimizeListeners reduction now matches php -n exactly; the compiled Symfony app stopped 404ing and reached Twig's FilesystemLoader"
sources:
  - path: src/ir_lower/stmt/typed_foreach.rs
    blob: 641a3dd5c50c35764f47e016b364f86b4f3e3763
  - path: src/parser/stmt/assign/compound.rs
    blob: de8ff8a50c303994952f50fef06ad867d34959da
  - path: examples/symfony-app/vendor/symfony/event-dispatcher/EventDispatcher.php
    blob: 43bc16b85b4e5e8f55b0e50577d00f7991bc27ce
---

# Every Symfony route 404s because EventDispatcher::optimizeListeners miscompiles twice

## Fact

THE SYMPTOM that hid this for several sessions: the app booted, served a PERFECT Symfony 404 page
for every URL, and logged nothing. The error page worked because `kernel.exception` has ONE
listener, and "run only the last listener" is indistinguishable from correct when there is one.

HOW IT WAS FOUND, after five reductions missed it: a diagnostic compiled with `--web` that boots
the real kernel and prints what it sees. It separated the halves in one run —

    matcher=RedirectableCompiledUrlMatcher
    match(/)={"_route":"home",...}          <- the matcher is FINE
    matchRequest={"_route":"home",...}      <- routing is FINE
    status=404  _route=NULL  _controller=NULL   <- the LISTENER never ran

`HttpKernel::handleRaw` throws `NotFoundHttpException` when `_controller` is missing, so a
listener that never runs looks exactly like a route that does not exist.

DEFECT 1 — a by-reference foreach over a HASH read the bucket as its DECLARED type.
`iterator_source_kind_from_type` routes every `AssocArray` to `DynamicIterable` (an associative
checker shape can wear compact indexed storage at run time), and that path's by-reference arm calls
`__rt_hash_to_mixed`, which REWRITES EVERY BUCKET to a boxed Mixed cell. `foreach_ref_value_type`
kept answering `*value`, so `load_ref_cell` handed back the BOX POINTER:

    $v = ['a' => 1, 'b' => 2];
    foreach ($v as $k => &$z) { echo "$k=$z;"; }    // a=4374092928;b=4374092976;

Fixed by making the `AssocArray` arm answer `Mixed` — the storage is genuinely Mixed by then. An
INDEXED source with concrete elements keeps `Indexed { elem }`, takes
`ensure_unique_static_iter_source`, is never boxed, and stays exact. That asymmetry is exactly why
indexed by-reference loops always worked and hash ones never did.

DEFECT 2 — `$x = &$arr[]` shared ONE reference cell per SOURCE POSITION.
The parser desugars it to a temp named after `span.line, span.col, source_start`, so a loop reuses
one local; `$temp = null` on an already-bound name writes THROUGH the cell instead of rebinding.

    foreach ([1, 2, 3] as $i) { $c = &$opt[]; $c = $i; }   // every element aliased one cell -> 3,3,3

Fixed by appending `unset($temp)` to the desugared block. IT MUST COME LAST: lowering walks the
block ONCE and `unset_local` only emits `Op::UnsetLocal` for a name that is ref-bound AT LOWERING
TIME — a leading `unset` lowered to nothing and changed nothing.

STILL OPEN, measured and separate: an array element holding a reference is followed by `$a[0]`,
`foreach` and `count`, but NOT by `implode`, `json_encode` or `in_array` — those read the slot and
answer null/false. `foreach` is what `callListeners` uses, which is why Symfony works regardless.

## Why

It was the last blocker between a compiled Symfony kernel and a rendered page
