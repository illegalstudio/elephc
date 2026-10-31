# php-src tokenizer `.phpt` fixtures

These files are copied verbatim from `php/php-src`:

- Path: `ext/tokenizer/tests/*.phpt`
- Branch: `PHP-8.5`
- Commit: `3323b8db05d51899069b7b715e10da6fc1da29d5` (2026-10-30)
- License: PHP License 3.01, reproduced in `LICENSE`.

They are test data for `tests/tokenizer_php_src.rs`. That Rust test extracts each
`--FILE--` section, compiles it with the local `elephc` binary, runs it, and
matches `--EXPECT--`, `--EXPECTF--`, or `--EXPECTREGEX--`. No PHP installation or
network access is required at test time.

Do not edit these `.phpt` files in place. Refresh them deliberately from the
upstream commit above and update this README with the new provenance.
