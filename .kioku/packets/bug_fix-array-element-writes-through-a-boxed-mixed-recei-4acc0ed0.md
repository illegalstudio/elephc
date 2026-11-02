---
id: bug_fix-array-element-writes-through-a-boxed-mixed-recei-4acc0ed0
type: bug_fix
title: "Array element writes through a boxed Mixed receiver need the write-back path"
description: "Why $c->prop[$k]=$v silently does nothing when $c is an untyped parameter holding an object, and what routes it correctly"
tags: [symfony, di-container, mixed-receiver]
created: 2026-09-16
verified_by: "untypedarr.php byte-compared against php -n"
sources:
  - path: src/ir_lower/stmt/property_array_writes.rs
    blob: 6ccc2804804024ed6aa2e1edb021a68280593b65
  - path: src/codegen_support/runtime/objects/stdclass.rs
    blob: a548726663c3a1999f1e44ca7782b52a47260af4
---

# Array element writes through a boxed Mixed receiver need the write-back path

## Fact

An untyped parameter holding an object is a BOXED MIXED receiver, not `PhpType::Object`, so
`generic_object_array_property_type` returns None and `$c->prop[$k] = $v` fell through to the
final RuntimeCall fallback. That fallback calls `__rt_mixed_property_get`, which only understands
stdClass and answers NULL for every other class; `__rt_mixed_array_set` then wrote into that null
and the element was SILENTLY DISCARDED.

A scalar write to the same property through the same receiver worked, because that one already
went through the declared-property class-id ladder (`declared_mixed_property_set_candidates`) --
which is exactly what made the bug hard to see: `$c->marker = 'x'` persisted while
`$c->services['k'] = 'x'` on the same object did not.

Fixed by making `is_generic_object_receiver` return true for Mixed/Union, routing the write to the
existing PropGet -> array set -> PropSet read/modify/write-back path. Every earlier branch
requires `PhpType::Object`, so a Mixed receiver reaches only the final fallback and no other
behaviour changes.

Minimal repro:

    function writeUntyped($c): void { $c->services['k'] = 'untyped'; }

php -n prints `untyped`; elephc printed `MISSING` before the fix.

## Why

Symfony's generated DI container is written entirely against this shape, so every service it built was rebuilt on the next get()
