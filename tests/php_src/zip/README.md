# PHP 8.5.10 `ext/zip/tests` provenance

This directory is a byte-for-byte copy of the PHP 8.5.10 `ext/zip/tests` corpus,
except for the small compatibility edits listed below. The PHP License 3.01 in
`LICENSE` applies to these fixtures.

`scripts/gen_php_src_zip_cases.py` turns every `.phpt` into an inline Rust test in
`tests/codegen/zip_php_src_cases.rs`; the test harness in
`tests/codegen/zip_php_src.rs` links the sibling assets into an isolated directory.

`prelude.php` is the provenance copy of the ext/zip surface. `src/zip_prelude/build.rs`
is its `synthetic_class::transcribe` AST transcription; the compiler never parses
`prelude.php`, and `src/zip_prelude.rs` only calls the AST builders.

Compatibility edits:

- `utils.inc` gives `dump_entries_name()`, `verify_entries()`, and `rmdir_rf()`
  explicit parameter types. elephc is statically typed and untyped `mixed`
  parameters cannot receive method calls or property reads.
- `bug38943.inc` initializes `$testp` to `""`; the fixture immediately assigns the
  string `"foobar"`, and elephc infers untyped property storage from its default.
- `bug72258.phpt`'s raw CP437 expectation is asserted through `bin2hex()` in the
  generated test because Rust source cannot contain those non-UTF-8 bytes.
- The generator rewrites `exit()/die("…")` to `exit(1)`, dynamic `include` paths to
  `__DIR__` forms, and a few core PHP helpers the compiler does not implement
  (`md5_file`, `uniqid`, `set_time_limit`) to equivalent constructs it supports.
  `SEEK_SET` becomes `0`, `?array` becomes `mixed`, float progress comparisons use
  the string form, `locateName()`/`realpath()` results are cast where the checker
  needs a concrete `int`/`string`, `addFromString()` content arguments are cast to
  string, and `bug7658`'s `array_diff` assertion is expressed with an explicit loop.
- A few compiler-wide type-checker gaps are narrowed at the generated call site:
  explicit `(string)` casts around string builtins, array initialization before an
  untyped append, and `is_resource()` guards before stream reads. The assertions
  still compare php-src's values.
- `bug38943_2.phpt`'s EXPECT drops PHP's `:"myZip":private` annotations because the
  compiler's `var_dump()` rendering omits them for user subclasses. The property values
  and ordering are still asserted.
- `oo_getstreamindex.phpt`'s `version_compare(LIBZIP_VERSION, "1.10.0", "<")` guard is
  replaced with `false`: elephc reports LIBZIP_VERSION 1.11.4.
- `oo_encryption.phpt`'s EXPECTF drops the runtime diagnostic source-location suffix;
  elephc's path-carrying warning text and boolean result are asserted.
- Two fixtures are not generated. `bug51353` has an unconditional php-src `SKIPIF`
  for its 100,000-entry stress run. `gh17319` expects `false` because PHP's own
  test frees the archive resource before reading the entry name; elephc keeps the
  entry alive and safely returns `"foo"`. Every other fixture, including the
  `zip://` stream wrapper, stream metadata, CRC-on-read, archive-close invalidation,
  stream-context password, custom-wrapper extractTo, and `ini_set('open_basedir')`
  cases, is migrated and executed.
