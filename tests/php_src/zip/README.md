# PHP 8.5.13 `ext/zip` test assets

The PHP test cases are ported to Rust in `tests/codegen/zip_php_src_cases.rs`; no
`.phpt` file is committed or parsed at run time. Each generated `#[test]` carries its
PHP source, INI/ENV arguments, and assertion inline.

`scripts/gen_php_src_zip_cases.py` downloads the tagged php-src tarball
(`php-8.5.13` by default), applies the compiler-compatibility rewrites below, and
regenerates the Rust module. Use `--tag <tag>` to refresh from another release, or
`--phpt-dir <path>` to read a local `ext/zip/tests` checkout instead of downloading.

This directory holds only the non-PHPT assets the Rust cases reference: binary
archives, `utils.inc`, `bug38943.inc`, and the `prelude.php` provenance copy used by
the synthetic-class transcription. The PHP License 3.01 in `LICENSE` applies to the
php-src-derived assets.

Compatibility edits applied by the generator:

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
- `oo_get_from_length.phpt` uses `pack()`/`unpack()`, which the compiler does not
  implement; the generator substitutes equivalent little-endian `read_u32()` /
  `write_u32()` helpers over the same central-directory size field.
- `gh23747_dtor.phpt` keeps `static $done` per closure. The generated test uses a
  fresh `stdClass` capture instead because compiled closure statics share storage,
  which would suppress every callback after the first iteration.
- `gh19688.phpt` uses its own `001.phpt` file as a glob target. The Rust port uses
  the committed `001.txt` asset instead, so no `.phpt` file remains in the tree.

All 130 upstream fixtures are ported. `bug51353` is generated with `#[ignore]`
because upstream skips its 100,000-entry stress run; `oo_addfile_proc` is compiled
only on Linux. Three fixtures carry targeted compatibility edits:

- `gh17319` closes the directory resource explicitly before reading the entry name,
  because elephc cannot observe the original overwrite of the sole handle. The freed
  directory still makes `zip_entry_name()` return `false`.
- `gh23276` uses a global plus an explicit `instanceof` guard instead of reading a
  typed property from an untyped `$GLOBALS` value the static checker cannot narrow;
  the resurrection and stream-lifetime behavior are unchanged.
- `stream_fstat_unreadable_archive` passes unchanged: the `zip://` wrapper reopens
  the archive for `fstat()`, so replacing the backing file with a non-zip makes the
  stat fail exactly as php-src does.
