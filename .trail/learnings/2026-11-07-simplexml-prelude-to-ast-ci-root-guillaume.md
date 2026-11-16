---
title: SimpleXML prelude to AST + CI root causes on the main rebase
author: Guillaume
date: 2026-11-07
branch: feat/simplexml-php-src-compliance
touches:
  - src/simplexml_prelude/generated.rs
  - src/synthetic_class/transcribe.rs
  - src/ir_lower/expr/mod.rs
  - src/codegen_support/runtime/arrays/array_merge_recursive.rs
---

## Decided
- The SimpleXML prelude is now built as synthetic AST (`src/simplexml_prelude/generated.rs`),
  not parsed PHP. `synthetic_class::transcribe::transcribe_split` is the migration tool:
  parse once in a test, emit one builder helper per declaration, paste, delete the PHP.
  It is test-only and must never run in the compiler.
- Generated top-level declarations need `Span::synthetic()`. `class_is_user_declared`
  checks `declaration_span != Span::dummy()`; a dummy span disables PHP's weak parameter
  coercion, so `new SimpleXMLElement(0, ...)` fails to coerce int to string.
- The transcriber needed `Array`/`AssocArray`/`Buffer` type arms and a `s_synthetic`
  builder for parser-generated append-lowering groups.
## Watch out
- `__rt_object_to_hash` on x86_64 uses a custom ABI (`rax` object, `rdx` mode, `rcx`
  scope), unlike the AArch64 x0/x1/x2 form. Passing rdi/rsi/rdx segfaults.
- Undefined-variable warnings must not fire for eval-scoped names, post-eval names,
  `isset()`/`??` operands, or by-ref argument places.
- The append-property lowering originally appended in the shared prelude. Moving the push
  into the runtime else-branch must also restore it for the increment/decrement path.
- Diagnostic normalization must strip only the trailing ` in FILE on line N` suffix;
  a greedy " in ... on line" regex eats real message text (mb_split's
  "mbregex search failure in mbsplit(): ...").
