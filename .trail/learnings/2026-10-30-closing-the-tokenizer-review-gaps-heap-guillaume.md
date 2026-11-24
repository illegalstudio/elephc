---
title: "Closing the tokenizer review gaps: heap-backed memory guard, full define walk, indexed vocabularies"
author: Guillaume
date: 2026-10-30
branch: feat/tokenizer-native-engine
touches:
  - src/tokenizer_prelude.rs
  - src/pipeline.rs
  - src/types/checker/schema/property_default_selectors.rs
  - crates/elephc-tokenizer-engine/src/lib.rs
  - crates/elephc-tokenizer-engine/src/keywords.rs
  - crates/elephc-syntax/src/tokenizer_validation.rs
  - tests/codegen/tokenizer.rs
  - tests/codegen/oop/lexical_property_defaults.rs
---

The last two review gaps are fixed, and the hot paths are indexed.

- memory_limit: `tokenizer_declarations(heap_size)` replaces `__ELEPHC_TOKENIZER_HEAP_LIMIT__` in the prelude, and `inject_if_used` takes the pipeline heap size. Without Core INI the guard uses `--heap-size` (128M default) instead of returning -1; with Core INI the real `memory_limit`/`ini_set` value wins. The fatal still names the real caller file/line. Regression: `test_tokenizer_memory_limit_defaults_without_ini_set` uses an 8M test heap and no `ini_set`.
- define(): `collect_defines_stmt` now walks if/elseif/while/do-while/for init-condition-update/foreach iterable/switch subject and case/include/property-write expressions, and `collect_defines_expr` covers calls, closures, assignments, match, arrays, member/dynamic calls and first-class callables. The walk is gated on an unresolved default and skips internal prelude function/class bodies. Regression: `test_property_default_review_defines_in_control_expressions` matches PHP's 7|3|5|11|13|17.
- Engine: `id()` uses a OnceLock tokenizer-constant map instead of `lookup_constant` per token; keyword/cast/operator lookups use OnceLock maps; keyword/cast words longer than the vocabulary are rejected before hashing.
- TOKEN_PARSE: skip `line_starts`/`mapped_names` when the parser recorded no identifiers, and build the parser-view map with `Vec::resize`.
- Best-of-10 release bench (20k lines, 500k tokens): lex 33.5ms, TOKEN_PARSE 194.8ms. Earlier quadratic repro was ~355s; the first indexed pass measured ~583ms single-run.

Verified: 53/53 php-8.5.10 phpt, 54/54 PHP-8.5 head, tokenizer codegen 19/19, tokenizer errors 3/3, engine 13/13, syntax 39/39, parser 530/530, `cargo check --tests` zero warnings, `git diff --check` clean.
