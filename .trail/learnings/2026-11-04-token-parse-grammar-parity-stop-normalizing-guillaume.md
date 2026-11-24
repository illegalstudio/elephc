---
title: "TOKEN_PARSE grammar parity: stop normalizing syntax the parser now understands"
author: Guillaume
date: 2026-11-04
branch: feat/tokenizer-native-engine
touches:
  - crates/elephc-syntax/src/tokenizer_validation.rs
  - crates/elephc-syntax/src/parser/stmt/params.rs
  - crates/elephc-syntax/src/parser/expr/prefix_complex.rs
  - crates/elephc-tokenizer-engine/src/lex.rs
  - crates/elephc-tokenizer-engine/src/tests.rs
---

The final four-model tokenizer review needed grammar-only validation, not just parser-view rewrites.

What changed and why:
- Removed the parser-view rewrites that made `&` look like `|` and deleted `(` after `new`. Both hid real grammar once `parse_type_expr_nested` learned parenthesized DNF members and `parse_new_object` learned `new (expr)`. Keeping the rewrites caused false accepts (`new (foo())[0]`, `A&B|C`) and false rejects (`(double)$x` through the typed-local probe).
- Adding `Token::LParen` to type parsing made `looks_like_typed_assign("(int) $x")` true; typed-locals must explicitly reject a leading `(` so casts stay expressions.
- `consume_set_marker` must require byte adjacency between the visibility keyword and `(set)`: PHP accepts `private(set)` but rejects `private (set)`.
- Validation mode now bypasses compile-stage checks, so every check that PHP raises after parsing must be cfg(not(tokenizer-validation)): bare `try`, non-block catch, surplus variadics, bare properties, duplicate `switch default`, top-level `use`/`const`, enum/interface properties, static readonly, `break 0`.
- Reject elephc-only grammar in validation (`extern`, `packed`, generics, typed locals), and grammar-invalid inputs PHP rejects (chained comparisons, parenthesized write targets, reference-to-`new`, empty `isset`, `exit->`, mixed DNF, non-qualified `insteadof`).
- Lexer: `T_NUM_STRING` offsets use the numeric prefix only (`scan_offset_number`), and heredoc indentation inspects only the first `indent.len()` bytes of each body line.
- Local codegen tests link cached bridge staticlibs: after parser changes run `cargo build -p elephc-tokenizer -p elephc-magician` before `cargo test --test codegen_tests tokenizer`, or stale libraries make runtime `token_get_all` fail with old errors.
- The phpt runner now recovers a concrete fatal line from `EXPECTF` (`on line N`) or from the source declaration line of a `::method` in a final-method fatal, so a compile refusal with the right text but the wrong line fails.
- The generic `validate<T>` monomorphized in the engine's opt-3 crate, defeating the fast profile's validator opt-z override. Route the engine through the non-generic `validate_tokens(&[TokenView])` so the validator is compiled in `elephc-syntax-validate` at opt-z; this moved 33 KB out of the fast artifact (500,368 -> 467,424 B) without changing the lexer's opt-3 hot path.

A plain cargo release probe (no LTO, 16 CGUs, unwind, symbols) is 1,577,248 B and is not comparable to Mago's stripped release; fat-LTO + one CGU + strip gives 931,264 B with `panic = "abort"` and 1,014,096 B with unwind, both below Mago's 1,085,328 B, and 681,104 B with immediate-abort.

Evidence: engine 18/18 (including a PHP 8.5 message-and-line corpus and the ST_VAR_OFFSET matrix), phpt 54/54, parser 532/532, error 2133/2133, syntax 39/39, tokenizer codegen 20/20, isolation 1/1; `size` 451,216 B, `fast` 467,424 B, default release 1,577,248 B.
