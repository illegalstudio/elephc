---
title: "filter 029: call_user_func by-ref warning at the invoker boundary"
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - src/codegen/runtime_callable_invoker/by_reference.rs
  - src/codegen/runtime_callable_invoker.rs
  - src/codegen/context.rs
  - src/ir_lower/expr/descriptor_invoke.rs
  - src/ir_lower/expr/scoped_values.rs
  - src/codegen/lower_inst/instruction_helpers.rs
  - tests/codegen/callables/state_and_variadics.rs
  - scripts/filter/port_phpt.py
---

029.phpt's original expectations are restored: a FILTER_CALLBACK whose callable declares a by-ref parameter now emits PHP's 'Warning: name(): Argument #N ($param) must be passed by reference, value given in file on line N'. Implementation: call_user_func* descriptor invokes carry a private warn marker; the invocation profile is encoded as (scope << 2) | warn so the negative weak/strict scope ids cannot collide with the marker (using a high bit collided with -1, whose sign extension sets every bit). The descriptor invoker inspects each supplied by-ref argument and skips invoker ref-cell markers, PHP reference tags and persistent references; otherwise it emits the warning through __rt_diag_warning using the descriptor's callable name and the signature's parameter name, then decodes the profile before forwarding scope to the callee. The filter prelude passes (string) $string to call_user_func, which is a non-variable expression, so no invoker ref marker is created and the warning path is reached. The general engine still diverges: call_user_func('f', $x) wraps a bare variable in a ref cell (PHP passes by value) and the checker rejects literal non-variable arguments; tracked in elephc-tdh.
