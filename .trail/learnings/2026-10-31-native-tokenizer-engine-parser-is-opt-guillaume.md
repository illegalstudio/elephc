---
title: "Native tokenizer engine: parser is opt-in, lexer-only beats Mago"
author: Guillaume
date: 2026-10-31
branch: feat/tokenizer-native-engine
touches:
  - crates/elephc-tokenizer-engine/Cargo.toml
  - crates/elephc-tokenizer-engine/src/lib.rs
  - crates/elephc-syntax/src/tokenizer_validation.rs
  - docs/internals/architecture.md
  - scripts/measure-tokenizer-engine-size.sh
  - crates/elephc-syntax-validate/Cargo.toml
---

The engine-only size comparison only got fair once the AST parser became optional. `elephc-tokenizer-engine` now defaults to the byte-preserving lexer (`default = []`), and `parse` pulls `elephc-syntax`; the C tokenizer bridge and Magician enable it so TOKEN_PARSE still validates and reclassifies reserved words. A lexer-only build rejects TOKEN_PARSE with a clear error instead of silently skipping validation.

Measured on the 20k-line / ~500k-token corpus with the size-z profile (opt-level z, fat LTO, codegen-units 1, panic abort, strip) and release:
- engine-only lexer: size-z 435,184 B vs Mago 634,352 B; release 662,800 B vs 1,085,328 B.
- full TOKEN_PARSE engine without immediate-abort: size-z 751,280 B vs 634,352 B; release 1,891,792 B vs 1,085,328 B. This configuration links the compiler's complete AST parser (about 213 KB of size-z text).
- full TOKEN_PARSE engine with the engine-only size profile (opt-level z + panic immediate-abort, via `-Z build-std`): 451,216 B vs 634,352 B. The `fast` profile keeps the lexer at opt-level 3 and compiles only the validation parser at opt-level z: 483,936 B vs 798,784 B release-lto and 498,736 B same-profile Mago. Reproduce with `scripts/measure-tokenizer-engine-size.sh size|fast`.
- the validator is the same parser source compiled through `elephc-syntax-validate` with `tokenizer-validation`: assignment lowering and deep AST clones are skipped, so the validator never pays for trees it discards. The compiler keeps the full `elephc-syntax` build, which is why the feature had to live in a separate crate (Cargo features unify across the compiler + Magician graph).
- Fair warning: building Mago with the same immediate-abort profile gives 367,504 B (`size`) and 498,736 B (`fast`). The parser is still elephc's size premium; do not claim the parser itself is smaller than Mago's.

Perf after removing the per-token `SyntaxToken` copy, keeping the parser-view map in u32, skipping UTF-8 revalidation on the valid path, folding the three parser-view marking passes into one, and skipping reserved-list brace mapping when no T_LIST exists (interleaved CPU time, 20 iterations):
- TOKEN_PARSE: native 1.54-1.75 s vs Mago 2.71-2.88 s.
- lex: native 0.88-0.91 s vs Mago 1.50-1.55 s.
- one-shot TOKEN_PARSE: native 0.08 s / 51 MB RSS vs Mago 0.15 s / 131 MB.
- one-shot lex: native 0.04 s / 22 MB vs Mago 0.07 s / 28 MB.

Do not make `parse` default again to win a size number; the feature split is the design. Do not inline the parser copy back into validation: SourceToken is what keeps peak RSS near 51 MB by not materializing a second token vector.
