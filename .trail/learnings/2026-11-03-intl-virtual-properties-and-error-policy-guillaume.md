---
title: "Intl virtual properties, intl INI error policy, and eval coverage"
author: Guillaume
date: 2026-11-03
branch: feat/intl-managed-icu
cost: { minutes: 150 }
touches:
  - src/intl/mod.rs
  - src/native_deps/recipes/elephc_icu_intl_shim.cpp
  - src/codegen/lower_inst/objects/known_property_reads.rs
  - src/codegen/lower_inst/objects/mixed_property_reads.rs
  - crates/elephc-builtin-contract/src/support.rs
  - tests/codegen/intl_php_src.rs
---

- php-src internal classes expose *virtual* properties through their read handlers. A subclass may legally redeclare `Transliterator::$id` as `readonly`, so the synthetic parent must declare it too. A `readonly_prop` write in a non-`__construct` method records a throw-access site under `Span::dummy()`, which trips the checker debug assertion; initializing it in the private constructor is exempt because the constructor guard requires a span that identifies a node.
- `var_dump` hides compiler storage only through `var_dump_debug_info_projection`: a single `return ['key' => $this->declaredProp];` folds into descriptor rows. The runtime `__debugInfo()` call path is skipped for `is_internal` classes, so a projection is the only way for a synthetic internal class to drop `__elephc_handle` from a dump.
- Nullable and single-object-union property readers (`lower_nullable_prop_get_with_warning`, `lower_union_object_prop_get`) resolved declared slots directly and never consulted `__get`. Magic properties on `?T` receivers need the unboxed payload passed to the accessor through a register-based helper.
- PHP's intl error layer resets the process-wide error on every method entry and the object error through `INTL_METHOD_FETCH_OBJECT`, except in the `getErrorCode()`/`getErrorMessage()` accessors. `intl.use_exceptions` decides whether the error throws; `intl.error_level` decides whether it is displayed. Compiled fixtures have no `-d`, so the PHPT harness must translate `--INI-- intl.use_exceptions` into a runtime `ini_set()`.
- `Transliterator::createFromRules()` uses the fixed id `RulesTransPHP` and reports `UParseError` through php-src's `intl_parse_error_to_string()` shape (`parse error after "…", before or at "…"`).
- `eval_support()` demands a Magician eval binding for every non-prelude contract, so a new builtin area fails every `eval()` program until the area is declared `EvalImplementationPending`. Intl cannot run under eval because it needs the managed ICU bridge.

## Follow-up (same day): formatters, collation, bundles

- ICU `MessageFormat` has a named-argument overload (`format(names, values, count, appendTo, status)`); php builds the key list from the PHP array keys and the argument types from `MessagePattern`. Named args in the pattern are keyed by NAME, not by the `Part::getValue()` (which is 0 for every name) — using the numeric value merges all names onto arg 0 and fabricates "Inconsistent types".
- `MessagePattern`/`MessageFormat::applyPattern` report `UParseError`; initialize it to `{0}` or the reported line/offset is garbage. php's `intl_parse_error_to_string` shape is reused for pattern syntax diagnostics.
- Collator sorting mirrors php's flags: `SORT_STRING=1` (ICU string), `SORT_NUMERIC=2` (numeric), `SORT_REGULAR=0` (numeric when both values are numeric, ICU otherwise), stable order, and `sort`/`sortWithSortKeys` renumber keys while `asort` preserves them.
- `grapheme_strpos/stripos/strrpos/strripos` are ICU `StringSearch` + a character break iterator, not `UnicodeString::indexOf`: the locale/strength semantics (full-width folding, Turkish İ, ß/ss) only match through the collator. Pass `locale.c_str()` even when empty — `usearch_open(..., nullptr, ...)` fails.
- ResourceBundle tables/arrays are returned as ResourceBundle objects (only primitive int vectors become PHP arrays); iterate with `ResourceBundle::hasNext()/getNext()/getKey()` and transfer the `unique_ptr<ResourceBundle>` into the wrapper instead of copying it.
- `$fallback = false` opens with `ures_openDirect` and surfaces `U_USING_FALLBACK_WARNING`/`U_USING_DEFAULT_WARNING`; warnings are success codes but still carry their `u_errorName` in `intl_get_error_message()`.
- The PHPT harness defines `BUNDLE` in the fixture's own unit because include-defined constants are not visible inside the includer's functions (elephc-35jx).

## Follow-up 2: procedural dispatchers, formatter styles, and error prefixes

- php's `intl_error_get_message()` ALWAYS appends `": " + u_errorName(code)`, including
  `U_ZERO_ERROR`; a custom message with a zero code is valid (formatObject's "passed object"
  error is one). The shim distinguishes the success sentinel (empty/"U_ZERO_ERROR") from a
  custom message before prefixing.
- The call-location prefix (`Class::method(): ` or `function(): `) comes from the active
  function/method, not from the shim. Object methods keep the class prefix; a procedural wrapper
  must REPLACE it (`intl_reprefix_error` strips a leading `Class::method(): ` before prepending).
  `handle_fail` returns false on error, `global_fail` preserves null (Locale::lookup).
- `NumberFormatter` always opens through `unum_open()`: SPELLOUT/ORDINAL/DURATION/compact styles
  are not available through `NumberFormat::createInstance()`. ICU honours the pattern only for
  `UNUM_PATTERN_DECIMAL` (0) and `UNUM_PATTERN_RULEBASED` (9), matching php's observed behaviour.
- `Locale::composeLocale()` accepts ARRAY values only for the singular `variant`/`private`/
  `extlang` keys; numbered keys must be strings. Numbered keys are looked up across 0..63
  without stopping at the first gap (variant1 + variant14 both count).
- `Locale::lookup()` reports a null byte in any language-tag array element as argument #2
  (`$locale`), a php-src quirk; the locale/default-locale null-byte errors are #2/#4.
- `IntlRuleBasedBreakIterator` validates arity/type at runtime (0 args, 3+ args, array compiled)
  and exposes `typeid(*iterator).name()` as the debug `type`.
- `IntlDateFormatter` validates the timezone as UTF-8 and against `TimeZone::getUnknown()`, and
  rejects non-IntlCalendar/int/null calendar arguments with a TypeError before opening.
- DateTime now carries a hidden `__elephc_initialized` flag set by `__construct`; formatObject
  throws `DateObjectError` when a subclass skipped `parent::__construct()`.
- Remaining corpus failures exposed general compiler gaps: foreach-by-value followed by a
  function call can corrupt the current element (formatter_format8's `$pattern`), dynamic method
  dispatch on a nullable/mixed ResourceBundle returned by `get()` fatals, `list()` destructuring of
  a long string can crash (formatter_get_set_symbol2), object `==` is still unsupported, plus the
  already-tracked `[&$x]`, iterator_to_array, nullsafe on mixed, Reflection dynamic lookup,
  str_replace array search, and `$GLOBALS` issues.

## Follow-up 3: locale canonicalization, ICU pattern errors, and calendar constructors

- `NumberFormatter`/`IntlDateFormatter` canonicalize their locale by dropping a POSIX
  `.charset` and a non-keyword `@modifier` (`pt_PT.utf8` -> `pt_PT`, `fr_CA@euro` -> `fr_CA`)
  while keeping keyword modifiers such as `@calendar=islamic`. Invalid locales are a
  `ValueError` from the constructor/create entry points using `uloc_getISO3Language`.
- `MessageFormat` construction failures must distinguish an invalid locale (php's
  `"Creating message formatter failed"`, with `U_ZERO_ERROR` for `formatMessage` and
  `U_ILLEGAL_ARGUMENT_ERROR` for `parseMessage`) from a pattern parse failure
  (`"pattern syntax error (parse error at offset …, after …, before or at …)"`). The
  `UParseError` overload of the `MessageFormat` constructor supplies the context.
- `IntlCalendar::equals()/before()/after()/isEquivalentTo()` report an unconstructed
  argument as `Argument #1/$other is uninitialized`; the procedural forms use position #2.
- `IntlGregorianCalendar::createInstance()` internally allocates with defaults, so the
  `>2 arguments` deprecation must require a non-null first argument; otherwise every
  `IntlCalendar::createInstance()` emits it. The procedural `intlgregcal_create_instance()`
  emits the function deprecation and builds via the 2-argument constructor + `setDateTime`
  to keep the function's own argument positions and avoid the constructor deprecation.
- `set_date_time` must zero ICU's millisecond field; `Calendar::set(y,m,d,h,mi,s)` keeps
  the object's current millisecond, which leaked a few hundred ms into `getTime()`.
- `grapheme_extract()` MAXCHARS counts UTF-8 code points and MAXBYTES counts UTF-8 bytes;
  the by-ref `next` output is the ending byte offset, encoded as the second element of the
  bridge outputs array (omitting it warned "Undefined array key 1").
- `grapheme_levenshtein()` compares grapheme clusters with `ucol_strcoll` (not exact
  `UnicodeString` equality), so canonical equivalents and variation selectors match.
- `NumberFormatter::parse(TYPE_INT64)` must read `Formattable::kInt64`; otherwise the
  double fallback silently returns 0 for values above 2^31.
- `Collator`'s SORT_REGULAR numeric path trims whitespace before `strtod`, matching PHP's
  numeric-string definition for values like `"  -100  "`.
