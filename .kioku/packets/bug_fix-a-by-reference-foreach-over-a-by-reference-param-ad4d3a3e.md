---
id: bug_fix-a-by-reference-foreach-over-a-by-reference-param-ad4d3a3e
type: bug_fix
title: "A by-reference foreach over a by-reference PARAMETER lost its writes when the array was shared"
description: "The COW separation's new array was published only to a load_local origin, never through a load_ref_cell one"
created: 2026-09-22
sources:
  - path: src/codegen/lower_inst/iterators.rs
    blob: 2601c1a219de36b1f3b09be69b59877cfce1f242
    lines: 600-660
    snip: 1bcef4a4c57e
    anchor: "enum IterSourceOrigin {"
---

# A by-reference foreach over a by-reference PARAMETER lost its writes when the array was shared

## Fact

A by-reference `foreach` separates a shared array (`__rt_array_ensure_unique`) and must put the
new array back where the source was read. `source_load_local_slot` recognised only `Op::LoadLocal`,
so a source read through a by-reference parameter (`Op::LoadRefCell`) was separated and then
abandoned: the loop wrote into a copy nobody kept, and the separation had already given up the
variable's share of the original, which the caller then released once too often.

MEASURED against php 8.5.10, identical on the pristine HEAD compiler:
    function bump(array &$a) { foreach ($a as &$x) { $x = 7; } }
    function run(array $e) { $c = $e; bump($c); return $c; }      php [7,7]   elephc array(0) {}
A fresh local (`$l = [1,2]; bump($l);`) worked: nothing shared it, so no separation ran.

FIX: `IterSourceOrigin::{Local, RefCell}`; the ref-cell origin writes through
`local_stores::store_value_through_ref_cell_slot`, the path relocating builtins already use.
Both the static (`ensure_unique_static_iter_source`) and dynamic
(`store_iter_source_to_origin_if_local`) paths go through `publish_iter_source_to_origin`.

## Why

function f(array &$a) { foreach ($a as &$x) ... } returned array(0) whenever the caller's array had a second holder
