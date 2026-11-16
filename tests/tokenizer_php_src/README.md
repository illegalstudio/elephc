# php-src tokenizer corpus

`cases.rs` transcribes the `--FILE--` and `--EXPECT--` / `--EXPECTF--` sections of
the php-src `ext/tokenizer/tests/*.phpt` suite into Rust data:

- Path: `ext/tokenizer/tests/*.phpt`
- Branch: `PHP-8.5`
- Commit: `3323b8db05d51899069b7b715e10da6fc1da29d5` (2026-10-30)
- License: PHP License 3.01, reproduced in `LICENSE`.

`tests/tokenizer_php_src.rs` compiles and runs every inline case. The test process
never reads a `.phpt` file, and no PHP installation or network access is needed.

Do not edit the transcribed expectations to make a failing case pass. Transcribe the
upstream case deliberately and update the commit above when refreshing the corpus.
