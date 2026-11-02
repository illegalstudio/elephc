---
id: bug_fix-a-closure-capturing-itself-by-reference-inside-a-22e48fb0
type: bug_fix
title: "A closure capturing itself by reference inside a function stored its raw descriptor into a boxed cell"
description: "Stores and loads of a ref-bound callable followed the flow type (callable) while the cell is a Mixed box; the closure read it as Mixed and saw a string"
created: 2026-09-22
sources:
  - path: src/ir_lower/context.rs
    blob: bd4e0e2163e750a7fa723d1ba41b627009678073
    lines: 1540-1565
    snip: 18fed4ddd944
    anchor: "return LoweredValue {"
  - path: src/ir_lower/context.rs
    blob: bd4e0e2163e750a7fa723d1ba41b627009678073
    lines: 1970-2010
    snip: 3bc96c2b9452
    anchor: "let is_ref_bound = self.is_ref_bound_local(name);"
---

# A closure capturing itself by reference inside a function stored its raw descriptor into a boxed cell

## Fact

`use (&$c)` before `$c` is assigned gives `$c` a boxed Mixed cell. The enclosing function then
stored the closure with `store_ref_cell` typed by the name's FLOW type (`callable`), so the raw
descriptor pointer went into the box, and read it back the same way -- a direct `$c(...)` worked.
The closure body reads its capture by the cell's storage (Mixed) and took the descriptor's first
word as a type tag.

MEASURED against php 8.5.10, identical on the pristine HEAD compiler, inside a FUNCTION body:
    gettype($c)                 php object   elephc string
    call_user_func($c, $n - 1)  php works    elephc Call to undefined function <dynamic>()
At top level the program worked: a global's slot is not ref-bound this way.

FIX: `store_local` falls back to the SLOT's storage type (when Mixed) instead of the flow type for a
ref-bound store with no owner slot -- the codegen then boxes; `load_local` reads a ref-bound
`callable` whose cell is Mixed AS Mixed, and a call on a Mixed value dispatches dynamically. Both
exclude interior `foreach` references, whose cell is an array ELEMENT (see the bridge packet).
Narrowed to `callable` deliberately: widening every ref-bound load to its storage type is the
general fix and was not measured.

## Why

$c = static function () use (&$c) {...} is the standard recursive-closure idiom; passing $c as a value (call_user_func, array_walk_recursive) fatalled with Call to undefined function <dynamic>()
