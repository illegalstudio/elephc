# PHP `ext/intl` PHPT provenance

This directory is the pinned PHP 8.5.10 `ext/intl/tests` corpus. The `.phpt` files are the
provenance copy and the generator input; the test process never reads them.

`phpt_status.json` records the status and skip reason observed from the reference PHP 8.5.10
interpreter with ICU 78.3 on macOS ARM64. It drives the `#[ignore]` markers for fixtures
php-src itself skips, and for the known `XFAIL` fixture.

## Migration shape

`scripts/gen_php_src_intl_cases.py` writes
`tests/codegen/intl_php_src_cases.rs`: one Rust `#[test]` per fixture with the PHP source,
INI/ENV values, corpus-relative directory, and EXPECT/EXPECTF/EXPECTREGEX assertion inline.
The harness lives in `tests/codegen/intl_php_src.rs`.

## Test-only adaptations

These preserve the fixture's observable intent while fitting elephc's current language surface:

- `ut_common.inc` is a shared test harness, not a php-src fixture. Its `$GLOBALS['oo-mode']`
  switch is replaced by a static `__ElephcIntlMode::$oo` property because elephc does not yet
  expose PHP's `$GLOBALS` superglobal. Wrappers for `collator_get_default`-style functions that
  PHP 8.5 no longer ships were removed; no fixture calls them.
- The wrapper parameters and returns are annotated. PHP's untyped wrapper parameters are
  dynamic `mixed`; elephc's checker resolves them eagerly, so the migration spells the concrete
  `Collator`, `NumberFormatter`, `IntlDateFormatter`, string, and array types the fixtures pass.
- `ini_set`/`ini_restore`/`ini_get` for `intl.default_locale` are rewritten to
  `locale_set_default`/`locale_get_default`, which is the API those directives control.
- `--INI-- error_reporting`, `intl.default_locale`, and `date.timezone` are applied at fixture
  startup because the compiled runtime bakes its defaults.

## Known migration gaps

- `locale_bug74993.phpt` and `locale_bug74439.phpt` iterate a string table through
  `ReflectionFunction` / `ReflectionMethod`, which currently require literals.
- Some error-path fixtures pass invalid argument types that elephc rejects at compile time
  rather than at run time.
- `locale_bug74439` and similar wrappers depend on dynamic `mixed` parameter inference.
