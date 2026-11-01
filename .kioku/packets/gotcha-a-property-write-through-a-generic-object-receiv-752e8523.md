---
id: gotcha-a-property-write-through-a-generic-object-receiv-752e8523
type: gotcha
title: "A property write through a generic object receiver leaks 3 blocks per write"
description: "Measured linear: 3 writes leave 9 live blocks, 6 leave 18; the slot's previous value is never released, and Symfony's generated container writes this way throughout"
created: 2026-09-17
sources:
  - path: src/codegen/lower_inst/objects/named_property_writes.rs
    blob: a5440bae8f34499339910d20009d8c693fb2b713
  - path: src/codegen/lower_inst/objects/runtime_property_writes.rs
    blob: 039758d04958a31abc318041597dcbb863af6416
---

# A property write through a generic object receiver leaks 3 blocks per write

## Fact

A property WRITE through a generic object receiver leaks 3 heap blocks, every time.

Measured with `compile_and_run_with_heap_debug` on a program whose only work is the write — no
unset, no arrays built anywhere else:

```php
class C {
    protected $services = ['a' => 1];
    protected static function reset($container) { $container->services = ['x' => 1, 'y' => 2]; }
    public function run() { self::reset($this); /* … */ }
}
```

- 3 writes: `allocs=25 frees=16 live_blocks=9`
- 6 writes: `allocs=46 frees=28 live_blocks=18`

Exactly **3 blocks per write**, linear, so the slot's PREVIOUS value is never released. The receiver
is an untyped parameter, so the write takes the generic path (`lower_mixed_prop_set` →
`__rt_mixed_property_set`, or `lower_declared_mixed_prop_set`) rather than the declared-slot store,
whose own doc says "Any refcounted payload the slot owned is released first".

This is the shape Symfony's generated container is written in throughout —
`getXService($container, …)` mutating `$container->…` — so a long-running `--web` worker leaks on
every service store, unbounded.

Found while heap-checking the new `unset($obj->prop[$key])` lowering: that lowering inherits the
same leak because it replaces the property's container, which is why its regression test asserts
BEHAVIOUR and records this instead of asserting a clean heap. Fixing the store makes both clean.
