---
title: Erased property writes are outlined into shared helpers (bead elephc-w8nq)
author: Guillaume
date: 2026-11-23
branch: bead-erasedwrite
touches:
  - src/ir_lower/erased_writes.rs
  - src/ir_lower/stmt/property_array_writes.rs
  - src/ir_lower/stmt/instance_property_writes.rs
  - src/codegen/lower_inst/objects/outlined_dynamic_property.rs
  - src/codegen/lower_inst/core_builtins/backtrace.rs
---

## Decided
- A write through a receiver that lost its class ($o->p[$k] = $v, op=, [] =, statement ++/--, $o->p = $v) is lowered ONCE per (form, property, key/value representation, scope group, source profile) in a synthetic function (src/ir_lower/erased_writes.rs); the site evaluates receiver, key and value in source order, lends borrowed operands and calls it. The body is the ordinary statement over variables, so every guard, readonly/visibility refusal, __set reentrancy and COW rule is the code the inline site used. Probe asm lines (one erased write, 1 or 20 __set classes, with or without SimpleXML): set 2373/12728 -> 254/263, plain assign 299/3832 -> 239/247, flat in both axes; pre-merge was 384 (set) and 164 (assign).
- Scopes share a helper through the grouping the codegen named property helpers already use (codegen::named_helper_scope_groups_in); ByLexicalClass when eval can run or a name is readonly/asymmetric.
- A diagnostic raised inside a helper names the CALLER's line because the helper's spans are compiler-built (nothing is published) and the entry saves the caller's published _php_site into a hidden local and republishes it before an instruction that can warn or run user code (backtrace.rs publish_call_site). Without the restore the Notice after __get named the line inside __get. shared_static_throw::raises_on_calling_site covers the helpers so readonly/asymmetric refusals keep the line.
- Cost levers that made the call cheaper than the inline ladder: no shadow clone of the helper's parameters, key/value passed typed (not boxed) when they are int/float/bool/string, the key/value parameters are not copied again by the write's operand rooting, and the exact-class selection compares class ids (ObjectClassId) instead of building get_class() strings when the program loads no code at run time. 1M-iteration instruction counts per write: set 3961 -> 3831, append 2351 -> 1825, compound 7528 -> 7243, assign 759 -> 582 (main -> this).
- The isset/??/element-update initialization probe of a declared name on a boxed receiver is a named shared helper too (DynamicPropHelperKind::NamedMixedInit).

## Tried and ruled out
- Boxing every operand as mixed: +50% instructions on the loop benchmark (three allocations per write).
- Keeping the plain assignment inline for names some class declares readonly: builtin classes declare value/name readonly, so most names would have stayed inline; the helper's raise line comes from the published site instead.

## Watch out
- ir_lower EIR tests that count ops in the function holding a boxed write now read the function plus the helpers (tests/mod.rs with_erased_write_helpers / erased_write_helper); class arms are matched by name or by class id.
- A reference store ($o->p[k] = &$x) must NOT be outlined: the boxed operand loses the reference.
- Left inline: expression ++/-- of an element, compound/??= of a plain property under SimpleXML, nested dimensions, static properties, unset, buffer/pointer/packed values, runtime closure scope, writes inside the property's own hook.
- Residual: a bare variable key is read at call time, before the native write-fetch inside the helper (the inline path read it after the fetch).
