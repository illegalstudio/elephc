# PHP 8.5.10 ext/tidy case provenance

The PHP 8.5.10 `ext/tidy/tests` cases are migrated into ordinary Rust tests:

- `tests/codegen/tidy_php_src_cases.rs` carries each PHP source and its
  EXPECT/EXPECTF/EXPECTREGEX assertion inline.
- `tests/codegen/tidy_php_src.rs` compiles and runs each case against the managed
  `libtidy` package. The tiny HTML/config fixtures are embedded in that harness and
  written into each isolated work directory, so no `.phpt` file is stored or parsed.

- Source: <https://github.com/php/php-src/tree/PHP-8.5.10/ext/tidy/tests>
- License: PHP License 3.01 (`LICENSE`).
- Version: PHP 8.5.10 (`VERSION`).

Test-only adaptations are documented in the harness doc comment and
`docs/php/tidy.md`.
