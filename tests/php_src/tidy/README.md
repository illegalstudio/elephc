# PHP 8.5.10 ext/tidy PHPT corpus

This directory is the pinned PHP 8.5.10 `ext/tidy/tests` corpus. The `.phpt` files are
the provenance copy and the generator input; the test process never reads them.

- Source: <https://github.com/php/php-src/tree/PHP-8.5.10/ext/tidy/tests>
- License: PHP License 3.01 (`LICENSE`).
- Version: PHP 8.5.10.

## Migration shape

`scripts/gen_php_src_tidy_cases.py` writes `tests/codegen/tidy_php_src_cases.rs`: one
Rust `#[test]` per fixture with the PHP source, INI/ENV values, corpus-relative
directory, and EXPECT/EXPECTF/EXPECTREGEX assertion inline. The harness lives in
`tests/codegen/tidy_php_src.rs`; it compiles each fixture against the managed
`libtidy` package and compares the merged stdout/stderr bytes.

`phpt_status.json` records the reference result observed on this target. Fixtures that
php-src itself skips are emitted as `#[ignore]` with their reason; the current ledger has
no known divergence for the pinned fixtures. Intentional compiler-level differences outside
these fixtures are listed under “Differences from PHP” in `docs/php/tidy.md`.

## Test-only adaptations

These rewrites keep the fixture's observable behavior while fitting elephc's static
compiler and byte-oriented output model:

- `012.phpt`: the user function's `tidyNode` parameter hint is removed. `tidy_parse_string()`
  is declared `mixed` because it returns `tidy|false`; elephc deliberately does not narrow
  `mixed` to a class at a call site (that rejection is a compiler invariant).
- `020.phpt`: non-UTF-8 source bytes in the heredoc are written as `\xNN` escapes. PHP
  decodes the escape to the same byte; Rust source cannot carry invalid UTF-8.
- Warning/notice spacing: PHP prints a leading blank line before diagnostics; elephc's
  runtime does not. The harness restores that display-layer separator on both sides before
  comparing, without changing any diagnostic text.
- `parsing_file_too_large.phpt` is ignored: php-src guards it behind
  `RUN_RESOURCE_HEAVY_TESTS`/`GITHUB_ACTIONS`, and it writes a multi-gigabyte fixture.

## Refresh

Do not edit these fixtures locally. Refresh the copy from the pinned php-src tag, run
`python3 scripts/gen_php_src_tidy_cases.py`, then rerun
`cargo test --test codegen_tests php_src_tidy`.
