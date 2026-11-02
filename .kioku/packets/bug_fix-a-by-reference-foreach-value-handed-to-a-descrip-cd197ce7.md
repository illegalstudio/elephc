---
id: bug_fix-a-by-reference-foreach-value-handed-to-a-descrip-cd197ce7
type: bug_fix
title: "A by-reference foreach value handed to a descriptor call corrupted the array it points into"
description: "The invoker ref marker retained and released an INTERIOR array address; the fix bridges the value through a boxed stack temporary and writes it back after the call"
created: 2026-09-22
sources:
  - path: src/ir_lower/expr/descriptor_args.rs
    blob: ecdf57623e0dd75b6e24a25a8d8a37f41b8f5a3f
  - path: src/ir_lower/stmt/typed_foreach.rs
    blob: cffeaf1f6bbcf3dc2966284042d3021cf94d43e3
  - path: src/ir_lower/context.rs
    blob: bd4e0e2163e750a7fa723d1ba41b627009678073
---

# A by-reference foreach value handed to a descriptor call corrupted the array it points into

## Fact

`foreach ($arr as &$v)` binds `$v`'s slot to the ADDRESS OF AN ELEMENT inside the array's payload
(`bind_indexed_current_value_ref`). Passing `$v` to a descriptor call (`$cb($v)`, `call_user_func`,
the array_walk helpers) built an invoker ref marker over that address with
`__rt_mixed_from_value(tag 11, addr, source_tag)`, which RETAINS tag-11 payloads with `__rt_incref`;
freeing the marker ran `__rt_decref_any` on the same address. Both write `[addr - 12]` -- the
previous element, or for element 0 the array's own capacity word. `__rt_incref` skips non-heap
pointers, which is why a plain local (stack address) always worked.

MEASURED against php 8.5.10, identical on the pristine HEAD compiler:
    $d = [1,2,3]; foreach ($d as &$v) { $double($v); }        php 2,4,6   elephc segfault
    $c = [1,'two',3]; walk(&$c, static fn ($x) => ...);       php 1,two,3 elephc "requested array
                                                              size exceeds the maximum allowed"

FIX (IR only, no runtime change): `LoweringContext::mark_interior_ref_local(name, element_type)`
is set at the foreach binding; `lower_invoker_ref_arg_marker` bridges such a local as
`$tmp = $v; call(&$tmp); $v = $tmp;` (a boxed stack temp is range-skipped by both refcount
helpers). The copy-out is written as an ordinary AST assignment and runs right after the invoke
whose callee value is OLDER than the marker (`take_ref_bridges_after`) -- PHP evaluates the callee
first, so that is what separates `$outer($v, $inner($w))`.

Three follow-on traps, each measured:
- the cell's storage for the copy-out is the recorded ELEMENT type, not the slot type (the checker
  boxes the name) nor the flow type (Mixed after an opaque by-ref call), and it must win over a
  ref-cell OWNER slot left by an earlier loop reusing `$v`;
- `store_local`'s Mixed->Int ref-cell narrowing CONSUMES the box, so a borrowed source (`$v = $tmp`)
  must be acquired first, or `$tmp` is freed under itself;
- the interior mark is cleared by `unset()` and any new reference binding.

A new tag for "borrowed, unowned" markers was the alternative and was rejected: 35 consumers
across both architectures recognise tag 11.

## Why

Every array_walk/array_walk_recursive call, and any foreach (&$v) that passes $v to a closure, crashed or printed garbage; the cause sits in two runtime helpers far from the symptom
