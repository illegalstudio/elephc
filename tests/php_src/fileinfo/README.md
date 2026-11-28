# php-src `ext/fileinfo/tests` corpus

This directory is a faithful copy of PHP 8.5.10's `ext/fileinfo/tests` fixtures
(the `.phpt` files, their binary/text assets, `resources/`, and the libmagic
upstream fixture pairs). The fixtures are generated into ordinary Rust `#[test]` functions in
`tests/codegen/fileinfo_php_src_cases.rs` by
`scripts/gen_php_src_fileinfo_cases.py`: each test has its PHP source and assertion
inline. `tests/codegen/fileinfo_php_src.rs` provides the shared asset-aware
compile/run helper. No php-src checkout or host PHP is needed at test time, and the
test process never parses a `.phpt` file.

Source: https://github.com/php/php-src/tree/PHP-8.5.10/ext/fileinfo/tests
License: PHP License 3.01 (`LICENSE`). The `magic` database and `upstream/`
fixtures originate from libmagic; see `crates/elephc-fileinfo/NOTICE.md`.

Do not edit these fixtures locally. Refresh the copy from the pinned php-src tag,
run `python3 scripts/gen_php_src_fileinfo_cases.py`, then rerun
`cargo test --test codegen_tests php_src_fileinfo`.
