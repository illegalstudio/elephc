---
title: filter.default/raw request storage + options leak fix
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - src/filter_prelude/request.rs
  - src/filter_prelude/options.rs
  - src/filter_prelude/input.rs
  - crates/elephc-builtin-contract/src/mbstring_abi/ini/core.rs
  - tests/web_tests.rs
---

filter.default/filter.default_flags are now Core INI directives, baked into the filter prelude, applied once per request to superglobals; raw INPUT_* arrays live in __ElephcFilterState::$raw for filter_input. Web bootstrap prepends before the filter initializer, so the snapshot sees parsed request data. The per-call filter_var options leak came from keeping the mixed-option scalar branches in the same function as the is_array branch: splitting parse_options into an array-only function and a separate scalar helper made the original 2000-iteration leak PHPT pass. PHPT 004/005/041 and filter_throw_on_failure_leak are restored to original expectations. Remaining gaps: dynamic ReflectionFunction for by-ref callback warnings, reference array literals, AOT parse_str, test-harness ini override plumbing, addcslashes.
