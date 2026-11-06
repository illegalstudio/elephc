---
title: "Remediating the tokenizer review: index the catalog, split the frontend, defer only what PHP defers"
author: Guillaume
date: 2026-10-30
branch: feat/tokenizer-native-engine
touches:
  - crates/elephc-syntax/src/tokenizer_validation.rs
  - crates/elephc-tokenizer-engine/src/lex.rs
  - crates/elephc-tokenizer-engine/src/parse.rs
  - src/types/checker/schema/property_default_selectors.rs
  - src/tokenizer_prelude.rs
  - scripts/ci/run-tokenizer-phpt.py
---

All five review blockers are fixed and re-verified.

- Property defaults: the catch-all undefined-constant check now knows literal `define()` names, catalog/prescan constants, and folds constant branches before reporting. `define('RUNTIME_DEFAULT', 7)` and `Switch::ON ? 1 : MISSING` compile; `SEEK_CUR/END/SET` were added to the shared catalog. Regression tests live in tests/codegen/oop/lexical_property_defaults.rs.
- Lexer: CRLF comments stop at CR, heredoc backslash-newline no longer swallows the label, short interpolation offsets (`$a[-1]`, `$a[1.5]`, `$a["b"]`, `$a[]`, `$a?->b`) match Zend, `#[` no longer leaves brace depth raised, `public(set)` requires adjacency, and binary/EOF/digit heredoc labels are covered by differential fixtures.
- TOKEN_PARSE: non-UTF-8 source is validated, unsupported-but-valid PHP is normalized in the parser view (goto, `$$x`, `${...}`, dynamic `::`, DNF, abstract hooks, first-class callables), reserved `list` positions are enforced, and lexical-vs-parser errors are ordered by source position. `token_id`/`token_name` now use process-wide OnceLock indexes instead of rescanning the whole constant catalog per token.
- Performance: 20k lines / 500k tokens release lex 59ms, TOKEN_PARSE 583ms; debug lex 1.1s, TOKEN_PARSE 5.5s. The earlier quadratic probe measured ~355s.
- Layering: the frontend moved to `crates/elephc-syntax` (no compiler/backend dependency). Debug `libelephc_tokenizer.a` is 31MB (was 351MB) and `libelephc_magician.a` is 118MB (was 346MB); a tokenizer program is 2.9MB.
- memory_limit: Core INI now validates/reads `memory_limit`; the tokenizer guard reports the real caller file/line via debug_backtrace and exits 255. It is active when Core INI is linked (documented limitation for tokenizer-only programs).
- phpt runner: `%e/%S/%a/%A` match run-tests.php, non-UTF-8 fixtures are written/read with surrogateescape, fatal tests require a non-zero exit, CI is pinned to php-8.5.10.

Evidence: 53/53 php-8.5.10 tokenizer phpt and 54/54 PHP-8.5 head; engine 13/13, syntax 39/39, tokenizer codegen 18/18, tokenizer errors 3/3, parser 530/530, mbstring suite and startup tests pass; `cargo check --tests` has zero warnings; `git diff --check` clean.
