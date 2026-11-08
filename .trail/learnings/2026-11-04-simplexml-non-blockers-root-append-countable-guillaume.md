---
title: "SimpleXML non-blockers: root append, Countable coercion, wrapper reads"
author: Guillaume
date: 2026-11-04
branch: feat/simplexml-compliance
cost: { minutes: 28, tokens: 74230 }
touches:
  - src/parser/expr/append_writes.rs
  - src/core_prelude.rs
  - src/codegen_support/interface_wrappers.rs
  - src/codegen_support/runtime/io/phar_read.rs
  - src/optimize/fold/expr.rs
---

## Decided
- A document-root `$sxe[]->prop = v` must fail before any append. Detect root
  with the shim's `elephc_xml_sx_int(handle, 9)` (first node == document root),
  exposed as a SimpleXML prelude method called through a core helper that tests
  `is_a($container, 'SimpleXMLElement')`. The string class name keeps programs
  without the XML prelude compiling; the dynamic method call is legal on mixed.
- `Countable::count()` tentative returns apply to every implementor under
  `#[ReturnTypeWillChange]`, not only SimpleXMLElement subclasses. Emit adapters
  for string, float and mixed returns; bool/int already share the int ABI. A
  float conversion lands in x9/r11, and string/mixed payloads are owned, so move
  the result and release the payload after saving the integer.
- `file_get_contents()` on a registered user wrapper drains `stream_read` via
  `__rt_stream_get_contents`; a refused `stream_open` keeps PHP's failed-open
  warning before returning false.
- The `str_replace([...], ...)` fold iterates the search array FORWARD. Wrapping
  in reverse order makes the last search run first and changes overlapping
  replacements.
## Watch out
- `value_may_be_false` must read the semantic type: `codegen_repr()` maps every
  union (including `Object|false`) to `Mixed`, so the old check was always false.
- The parser append prelude still contains the original push. Move it into the
  runtime else-branch AND delete `prelude.extend_from_slice(&append[boundary..])`,
  or the root probe runs after the mutation it is supposed to prevent.
