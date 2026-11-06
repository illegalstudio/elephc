---
title: "Final tokenizer review: parse-stage vs compile-stage gaps"
author: Guillaume
date: 2026-11-01
branch: feat/tokenizer-native-engine
touches:
  - crates/elephc-syntax/src/parser/expr/append_writes.rs
  - crates/elephc-syntax/src/parser/stmt/assign/postfix.rs
  - crates/elephc-syntax/src/tokenizer_validation.rs
  - src/tokenizer_prelude.rs
  - src/types/checker/functions/resolution/call.rs
---

The final three-model review found four P1 acceptance gaps that the 54 phpt corpus did not cover. PHP's `TOKEN_PARSE` validates the grammar only; several shapes elephc rejected are compile-stage errors in PHP and must be accepted by the validator.

- Empty dimensions: `echo $a[];`, `$a[] ??= 1;`, `foo()[] = 1;`, `$a[]--;`, `$a[]->b[];` are valid under `TOKEN_PARSE` but hit the compiler's "Cannot use [] for reading"/"Invalid assignment target" paths. The validation build now consumes the operator/RHS and returns a placeholder. `try_parse_postfix_assignment` also validates both sides as expressions before discarding the assignment, which covers call-result and effectful targets without weakening LHS syntax checks.
- Semi-reserved `goto`: Zend emits T_GOTO for `X::goto`, `const goto`, enum cases and named arguments. The view used to blank every T_GOTO to the next `;`, collapsing `X::` and breaking valid PHP. Blank only the real statement shape `goto <T_STRING> ;` (a close tag also terminates); every other use falls through to the parser's bareword handling and is reclassified to T_STRING.
- Namespaced `ini_set`: `program_has_ini_set` matched a user's `namespace Foo; function ini_set(...)`, then emitted a global `ini_set` call that did not exist. Only compiler-injected (`SourceMode::Internal`) declarations count now; `--web` passes its unconditional wrapper through a new `inject_if_used_with_web` flag, and everything else falls back to the compiled heap capacity.
- Static spread arity: the prelude arity check skipped all spreads. A statically sized array spread has a final positional count, so `token_get_all('x', ...[0, 1])` now raises the same surplus-arity diagnostic as plain positional arguments; dynamic spreads still belong to the call planner/runtime.

The validator wrapper now has `default = []` and the engine enables `elephc-syntax-validate/tokenizer-validation` explicitly. A manifest test rejects any dependency that enables `tokenizer-validation` on the compiler's `elephc-syntax`, which would silently strip assignment lowering while still compiling.

Final engine-only sizes: 434,672 B (size profile) and 483,904 B (fast profile), still below the Mago references. Verified: 14 engine, 39 syntax, 530 parser, 20 tokenizer codegen, 2021 error, 3791 bin and 54/54 phpt tests; zero warnings.
