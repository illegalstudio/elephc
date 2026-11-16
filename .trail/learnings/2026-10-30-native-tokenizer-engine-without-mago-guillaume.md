---
title: Native tokenizer engine without Mago
author: Guillaume
date: 2026-10-30
branch: feat/tokenizer-native-engine
cost: { minutes: 104, tokens: 498948 }
touches:
  - crates/elephc-tokenizer-engine/src/lex.rs
  - crates/elephc-tokenizer-engine/src/parse.rs
  - src/tokenizer_validation.rs
  - src/parser/tokenizer_feedback.rs
---

Replaced the Mago lexer/parser dependency with a native byte lexer plus elephc's own AST parser for TOKEN_PARSE.

What mattered:
- `crates/elephc-tokenizer-engine/src/lex.rs` now owns every Zend boundary: inline HTML, open/close tags, heredoc/nowdoc indentation, interpolation, casts, halt headers, bad characters and numeric literals. The old prelude transcription (`git show 55d7c600c0:src/tokenizer_prelude.rs`) was the useful rule reference.
- `src/tokenizer_validation.rs` normalizes source into a PHP-only parser view (blank tags/inline HTML, normalize elephc-only casts/backticks/trailing-dot floats, remove reference ampersands) and reuses `crate::parser` rather than adding a second PHP grammar.
- `src/parser/tokenizer_feedback.rs` records accepted name tokens with their spelling. The caller verifies the spelling at the mapped source offset; without that check, elephc's synthetic heredoc-interpolation spans rewrote T_START_HEREDOC to T_STRING.
- `tests/data/php_src_tokenizer_files.json` vendors all 54 php-src ext/tokenizer `--FILE--` sources; `tokenizer_php_src_ext_tokenizer_files_match_php` compares both flags against PHP 8.5.

Verification used:
- `cargo test -p elephc-tokenizer-engine` (12 tests, includes the 54-file php-src corpus).
- `cargo test --test codegen_tests tokenizer` and `cargo test --test parser_tests`.
- A local phpt runner over php-src ext/tokenizer/tests reaches 49/54; the five remaining programs hit unrelated compiler gaps (dynamic-property unset, undefined constant property defaults, fatal stack formatting, memory-limit OOM), while the tokenizer engine itself matches PHP on every test source.
