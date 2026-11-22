---
title: "SimpleXML after the main merge, round 2: refusal harness, boxed child selections, array_merge_recursive objects"
author: Guillaume
date: 2026-11-22
branch: bead-simplexml2
touches:
  - tests/codegen/support/compiler.rs
  - src/ir_lower/expr/array_access.rs
  - src/ir_lower/expr/native_isset.rs
  - src/array_gradual_prelude.rs
---

## Found
- compile_source_expect_backend_error compiled the closed world first (the harness mirrors the CLI Auto mode), and a closed world turns an unsupported unset shape into a runtime Error (lower_unset_builtin), so every 'EIR backend rejects this' probe saw a clean compile. The refusal belongs to the interpreter-linked build: the helper now calls the inner compile with closed_world=false. Main lowers an untyped static hash unset directly, so the static probe in typed_empty now asserts the php result instead of a refusal.
- isset/empty/?? over $mixed->child[1] answered false: the branch's lower_subscript_receiver_silently PropertyAccess arm (opaque receiver) read the child through lower_property_fetch_mode_from_value, which has no native XML dispatch. The in-mode probe (lower_property_probe_from_value_in_mode) is the XML-aware twin. array_access_expr_supports_native_isset_probe also guessed int for a property of a SimpleXMLElement|false receiver (array probe).
- array_merge_recursive over objects: php_array_merge_recursive converts an object operand with convert_to_array on a collision; the prelude helper did not (gh17409).
- Baseline trick: git worktree add --detach <pad>/basewt bc5b97bfd4 (the merge's main parent, NOT local main, which moved on) then cb.sh build --workspace; the CLI needs every bridge lib to link. Compare IR with --emit-ir and filter the user function.

## Ruled out
- Foreign eval closure by-ref leak (b(mixed $cb, int $x){$cb($x);} with a function(&$v){$v='ref';}) is not XML specific; IR matches the baseline up to acquire/release elision, so it is runtime/invoker side. See bd elephc-awkt.
