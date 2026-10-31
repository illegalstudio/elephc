---
title: "Native tokenizer vs Mago: const token IDs flip TOKEN_PARSE"
author: Guillaume
date: 2026-10-30
branch: feat/tokenizer-native-engine
touches:
  - crates/elephc-builtin-contract/src/catalog_constants_tokenizer.rs
  - crates/elephc-syntax/src/tokenizer_validation.rs
  - crates/elephc-syntax/src/parser/mod.rs
  - tests/tokenizer_php_src.rs
  - tests/fixtures/php-src-tokenizer
---

Mago 1.51 was re-created from origin/main in an isolated workspace and A/B-ed against the native engine. Corpus: 20k lines / 500k tokens, release, CPU time via /usr/bin/time -l (the box had load average 45, so wall time was useless), 20 iterations interleaved, plus one-shot RSS.

Results:
- lex: native 1.64-1.69s/20 (82-85ms), Mago 3.06-3.17s/20 (153-159ms).
- TOKEN_PARSE: native 0.97-1.00s/20 (48-50ms), Mago 1.67-1.72s/20 (83-86ms).
- one-shot TOKEN_PARSE: native 0.08-0.09s / 68MB RSS; Mago 0.15-0.16s / 130MB.
- engine-only binaries: native 1.8MB, Mago 1.0MB; full tokenizer bridge 2.9MB vs 8.7MB; debug staticlib 31MB vs 351MB.
(Repeated-call RSS is an allocator high-water mark in both engines; report one-shot memory, not the 20-iteration peak.)

The three consulted models (MiniMax-M3.1-Flash, GLM-5.3-Flash — 5.2-flash was not in the catalog, Mistral-Large-4) all pointed at the same hotspot: parser_view()'s per-token token_id() HashMap probes, not the AST parser. Fix: `token_const_id()` in the contract crate is a const fn over the shared tokenizer catalog; the validation view uses 46 compile-time i64 constants and asserts every one resolves. Cold diagnostics keep the old lookup. `reserved_list_error` now uses a one-pass enclosing-brace table, removing the top-level list O(n^2). Validation-only parsing drops each top-level AST after syntax validation for bounded memory.

php-src tests are now vendored under tests/fixtures/php-src-tokenizer (54 .phpt from PHP-8.5 commit 3323b8d, PHP License 3.01). `tests/tokenizer_php_src.rs` is the Rust runner; CI no longer clones php-src. It runs 54/54 locally without PHP or network.

Do not drop the parser-view normalizations, and do not replace the parser with a second grammar. The const-ID change is target-neutral and needs no unsafe.
