---
title: DOM php-src tests as Rust tests; PHP tags; dirname includes
author: Guillaume
date: 2026-11-08
branch: feat/dom-php-src-compliance
touches:
  - src/lexer/scan.rs
  - src/resolver/include_path.rs
  - src/resolver/state.rs
  - tests/codegen/php_src_corpus.rs
  - tests/codegen/dom_php_src/mod.rs
---

## Decided
- The 868 ext/dom + 32 ext/libxml php-src 8.5.8 tests are ordinary hand-maintained Rust tests (tests/codegen/dom_php_src/<topic>.rs, tests/codegen/libxml_php_src/cases.rs): the user wants them part of the suite, not a vendored .phpt corpus with a committed generator (unlike SimpleXML's tests/php_src + scripts/gen_php_src_simplexml_cases.py). Assets live in tests/fixtures/{dom,libxml}_php_src/; each test runs from its php-src folder in a mirrored tree.
- Not-yet-passing tests carry #[ignore = "DOM port pending: <first error>"]; php-src's own environment skips keep "php-src SKIPIF: <message>" (evaluated once with the pinned oracle: libxml2 2.15.3, 64-bit, release build). Guards that only fail because an oracle build lacks pcntl or zlib are NOT skips.
- php_src_corpus.rs compares raw bytes: encoding fixtures print Shift_JIS/GB18030/UTF-16, so EXPECTF becomes a regex::bytes pattern whose % tokens use (?-u:...) classes and whose non-UTF-8 expectation bytes become (?-u:\xHH).
- The lexer now implements php-src's INITIAL/ST_IN_SCRIPTING tags: case-insensitive <?php followed by whitespace/EOF (swallows one whitespace char), <?= as echo, bare <? (short_open_tag is 1 under php -n), ?> as ';' swallowing one newline, text outside tags lowered to echo, //- and #-comments end before ?>, and a leading #! line is skipped. Text before the first tag is output, like php.
- include dirname(__FILE__) . '...' folds statically (php_dirname = zend_dirname: '' stays '', '/' stays '/', bare name -> '.'), but only when calls_global_function() proves the call reaches the global function: fully qualified, or unqualified outside any namespace with no `use function` alias. ResolveState now carries FileImports {constants, functions}, saved/reset at every namespace and include boundary.

## Tried and ruled out
- Keeping the SimpleXML-style generated *_cases.rs + committed generator for DOM: the user wants the tests rewritten into the suite.

## Watch out
- ext/dom tests use <?PHP in dom_test.inc and end includes with ?>; before the lexer change every include of it failed with "Expected '<?php' at start of file".
- trail check flags verbatim php-src test bodies by word overlap; those hits are false positives.
- Magician's dirname('') returns '.', php returns '' (bead elephc-8q7.9).
