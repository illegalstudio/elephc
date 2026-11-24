---
title: "Five-model review of the native tokenizer: 54/54 is not sufficient"
author: Guillaume
date: 2026-10-30
branch: feat/tokenizer-native-engine
cost: { minutes: 31, tokens: 349533 }
touches:
  - crates/elephc-tokenizer-engine/src/lex.rs
  - src/tokenizer_validation.rs
  - src/types/checker/schema/property_default_selectors.rs
  - crates/elephc-tokenizer-engine/Cargo.toml
  - scripts/ci/run-tokenizer-phpt.py
---

Five independent reviews (GLM-5.3-Flash, MiniMax-M3.1-Flash, Kimi-K3, Mistral-Large-4, Fable via `claude -p --model fable`) all concluded fix-first/blocked despite the 54/54 php-src phpt result.

Confirmed blockers, reproduced locally:
- The property-default undefined-constant catch-all breaks valid PHP: `define('RUNTIME_CONST', 7); class A { public $x = RUNTIME_CONST; }` now fatals, and `B::ON ? 1 : UNDEFINED_C` fatals even though PHP folds the branch to 1. Narrow the deferred-error rule or resolve at runtime.
- The lexer has real boundary bugs outside the vendored corpus: CRLF line comments include `\r` and shift every later line; a heredoc body ending in backslash-newline swallows the closing label; `$a[-1]`/`$a[1.5]`/`$a["b"]`/`$a[]` short-offset tokenization diverges; `#[` inside interpolation leaves brace depth raised.
- `TOKEN_PARSE` skips validation on non-UTF-8 input and is quadratic (`view_offset` rescans the view per name, `parse.rs` linearly finds each token). A 20k-line probe took ~355s versus PHP's 0.03s.
- `elephc-tokenizer-engine` now depends on the root compiler crate: debug `libelephc_tokenizer.a` is ~351MB, `libelephc_magician.a` ~346MB, and rustls/ring/ureq enter the runtime bridge. Split the parser into a leaf crate or an optional parse bridge.
- `memory_limit` only guards when the program mentions `ini_set`, hardcodes tokenizer.php:1, and accepts invalid values. Docs overclaim Zend wording and general memory-limit enforcement.

The 54-file vendored corpus and the phpt suite do not exercise these shapes. Add differential fixtures before claiming compliance.
