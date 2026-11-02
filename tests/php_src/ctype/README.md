# php-src `ext/ctype/tests` corpus

This directory is a faithful copy of PHP 8.5.10's `ext/ctype/tests` fixtures.
The fixtures are generated into ordinary Rust `#[test]` functions in
`tests/codegen/ctype_php_src_cases.rs` by `scripts/gen_php_src_ctype_cases.py`:
each test carries its PHP source and assertion inline.
`tests/codegen/ctype_php_src.rs` provides the shared compile/run helper. No
php-src checkout or host PHP is needed at test time, and the test process never
parses a `.phpt` file.

Source: https://github.com/php/php-src/tree/PHP-8.5.10/ext/ctype/tests
License: PHP License 3.01 (`LICENSE`).

`lc_ctype_inheritance.phpt` exercises the real libc `LC_CTYPE` table. Elephc
implements `setlocale()` through libc and classifies `ctype_*` bytes with the
same libc `is*` predicates, so the fixture runs rather than being skipped.
`preg_match('/\w/', ...)` receives PCRE2 character tables built with
`pcre2_maketables()` for the active locale.

Do not edit these fixtures locally. Refresh the copy from the pinned php-src tag,
run `python3 scripts/gen_php_src_ctype_cases.py`, then rerun
`cargo test --test codegen_tests php_src_ctype`.
