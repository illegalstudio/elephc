# php-src tokenizer sources

`php_src_tokenizer_files.json` holds the hex-encoded `--FILE--` sources from the
php-src `ext/tokenizer/tests` suite:

- Branch: `PHP-8.5`
- Commit: `3323b8db05d51899069b7b715e10da6fc1da29d5` (2026-10-30)
- License: PHP License 3.01, reproduced in `tests/tokenizer_php_src/LICENSE`.

It feeds the differential lexer test (`tokenizer_php_src_ext_tokenizer_files_match_php`)
only. No `.phpt` file is read at test time.
