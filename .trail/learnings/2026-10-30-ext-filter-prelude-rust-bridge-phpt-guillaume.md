---
title: "ext/filter: prelude + Rust bridge, PHPT replay and known compiler seams"
author: Guillaume
date: 2026-10-30
branch: feat/filter-php-src-compliance
touches:
  - crates/elephc-filter/src/lib.rs
  - src/filter_prelude.rs
  - src/filter_prelude/apply.rs
  - tests/codegen/filter_phpt.rs
  - scripts/filter/port_phpt.py
---

Implemented PHP ext/filter as a synthetic AST prelude (no PHP source parsing) backed by crates/elephc-filter. filter_var/walk/options use sequential returns and ListUnpack for php-src flag/array semantics; email/regexp live in the bridge (fancy-regex) so basic filter programs no longer need pcre2. Ported all 117 ext/filter PHPTs into tests/fixtures/filter_phpt/cases.json plus one generated test each; 117/117 plus 12 codegen tests green. Five fixtures are adapted for compiler-level gaps: mixed-array helper returns leak 40-160 B/call, call_user_func omits PHP's by-ref temporary warning, filter.default/raw SAPI input storage are absent, array-literal reference elements are rejected, and AOT parse_str is missing. Bridge list/nextest/release lists must be updated for every new bridge (the parity test catches omissions).
