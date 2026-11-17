---
title: "PHP-src corpus review: scope handle normalization, align asm comments"
author: Guillaume
date: 2026-11-04
branch: feat/simplexml-compliance
cost: { minutes: 32, tokens: 74230 }
touches:
  - tests/codegen/simplexml_php_src.rs
  - src/codegen_support/runtime/io/phar_read.rs
  - ".config/nextest.toml"
---

The php-src SimpleXML corpus runner must normalize only real `var_dump()` handles:
`object(Foo)#N`. A blanket `#\d+` rewrite also erases `Argument #1` indexes,
`&#38;` character references and bug numbers, which are exactly the assertions
some fixtures own. The fatal helper already skipped normalization; keep both
paths consistent and add a guard test.

Every inline assembly comment added to a runtime emitter needs one trailing
space re-check with `python3 scripts/check_asm_comments.py <files>` before
committing; a 47-line drift slipped through in the first pass even though the
base files were clean. The checker reports the exact line and current column.

CI only triggers `pull_request` on `main`, so a PR targeting `feat/simplexml`
gets no checks. State that explicitly instead of implying a green CI gate; run
the corpus plus focused shared-surface suites locally and record the numbers.

`cargo nextest` overrides use first-match semantics: when the corpus budget is
copied to the top of `.config/nextest.toml` for the macOS override, delete the
older copy at the bottom instead of keeping a dead duplicate.
