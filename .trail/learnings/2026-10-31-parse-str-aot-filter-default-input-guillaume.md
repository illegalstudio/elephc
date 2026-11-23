---
title: parse_str AOT + filter.default input filtering
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - src/parse_str_prelude.rs
  - crates/elephc-builtin-contract/src/catalog_data.rs
  - src/filter_prelude/request.rs
  - src/filter_prelude.rs
  - src/types/checker/functions/resolution/resolved.rs
  - src/types/checker/functions/resolution/specialization.rs
  - src/types/checker/functions/call_validation.rs
  - src/types/checker/functions/reference_outputs.rs
  - src/types/checker/inference/expr/effects.rs
  - tests/codegen/strings/parse_str.rs
  - tests/error_tests/parse_str.rs
  - scripts/filter/port_phpt.py
---

parse_str is now a PreludeProvided contract injected by src/parse_str_prelude.rs. The wrapper parses through mb_parse_str into a managed Mixed local, then applies the baked filter.default recursively when configured, and publishes the caller's by-reference result. filter.default now converts a rejected/null scalar to PHP's empty-string input value and casts successful results to strings, fixing the request input semantics as well. The checker was taught that an undefined variable passed to a user-defined by-reference parameter is created, not an undefined read: the effect pre-pass, untyped-parameter specialization, resolved-call validation, and output recording all handle it, including named arguments. The codegen harness injects the filter prelude with its request initializer disabled and the parse_str prelude with the test INI overrides, because the PHPT setup populates superglobals after startup. filter_default_parse_str.phpt is restored to the original php-src source and expectations. Remaining parse_str gaps: no eval route; inherits mb_parse_str encoding detection and diagnostic prefix; mb_parse_str itself does not apply filter.default. Tracked in elephc-lp3.
