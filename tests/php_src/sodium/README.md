# `ext/sodium` PHPT provenance

These files are copied verbatim from the pinned PHP 8.5.10 `ext/sodium/tests` tree
(`/Users/guillaumeloulier/PhpstormProjects/oss/php-src-8.5.10-oracle`) used as the
compatibility oracle for this repository. They are the provenance copy and the input
to `scripts/gen_php_src_sodium_cases.py`; the compiled test process never parses a
`.phpt` file.

The generated inline Rust tests live in `tests/codegen/sodium_php_src_cases.rs` and
are driven by `tests/codegen/sodium_php_src.rs`.
