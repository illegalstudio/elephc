---
title: "Intl (ICU)"
description: "The managed ICU-backed ext/intl surface: locale, normalizer, grapheme, date, number, message, collation, timezone, calendar, and the Intl classes."
sidebar:
  order: 27
---

## Overview

elephc implements PHP's `ext/intl` surface on a statically pinned ICU 78.3 build. The
compiler ships no ICU headers or runtime dependency to the user: `elephc native add icu`
materializes the managed package, and a program links it only when it references an Intl
symbol.

```bash
elephc native add icu
```

The PHP surface is split into two paths that share one C ABI:

- **Registry builtins** for scalar-only procedural entry points (`grapheme_*`,
  `normalizer_*`, `locale_*`, `intl_*`, `intltz_*` scalar helpers). Their EIR lowering calls the
  synthetic `__elephc_intl_invoke_*` helpers.
- **Synthetic AST declarations** for object-bearing and by-reference entry points and every
  Intl class. These are ordinary `FunctionDecl` / `ClassDecl` nodes built in Rust, exactly what
  the parser would produce. No PHP source is tokenized or parsed to construct them.

The bridge staticlib is `elephc_intl`; it forwards the fixed C ABI to
`libelephc_icu_intl_shim.a`, which owns the ICU C++ objects, the handle table, and the
per-object error state. The catalog's `icu` package provides `libicui18n.a`, `libicuuc.a`,
`libicudata.a`, the C++ runtime, and the shim, in that order.

## Supported surface

The registry and generated declarations cover the complete PHP 8.5 `ext/intl` function and
class inventory. The php-src PHPT migration under `tests/php_src/intl/` is the compliance
ledger; tests that php-src itself skips on this platform remain ignored with the upstream
reason, and `locale/bug74993.phpt`/`locale/bug74439.phpt` are marked as dynamic-reflection
migration gaps.

The implementation is actively closing the remaining behavioural gaps cluster by cluster.
Current focus order:

1. `IntlDateFormatter` and `IntlTimeZone`
2. `IntlCalendar` / `IntlGregorianCalendar`
3. `MessageFormatter`, `NumberFormatter`, `Collator`
4. `IntlBreakIterator` and its iterator classes
5. `Transliterator`, `Spoofchecker`, `ResourceBundle`, `UConverter`, `IntlListFormatter`
6. `IntlChar` and the remaining static helpers

## Behavioural notes

- The default locale starts at ICU's default and follows `locale_set_default()`; the
  `intl.default_locale` startup directive is applied by the test harness through that API.
  Setting it also mirrors the value into ICU's `Locale::setDefault()` so calendars,
  formatters, and collators created without an explicit locale observe it.
- Milliseconds vs seconds follows PHP: `IntlCalendar::getTime()`/`setTime()`/`getNow()` use
  milliseconds, `IntlDateFormatter::format()` takes seconds, and `parse()` returns seconds.
- The bridge invokes ICU exactly once per call. It never issues a null-buffer sizing query,
  because that would execute mutating methods (`add`, `roll`, `set`, `setTimeZone`) twice.
- Nullable scalar arguments are split into explicit branches before the typed value stream is
  packed; a nullable scalar packed into a `mixed` array loses its tag.
- Intl functions are effectful: ICU keeps process-wide default locale and error state, and the
  bridge's handle table is mutex-protected for compiled web programs.
- `--strict-php` hides elephc-only extensions but not PHP's own `ext/intl` names.
- Invalid argument types that PHP rejects at runtime may be rejected at compile time by
  elephc's checker. Runtime weak-mode TypeError parity is tracked separately.

## Testing

The php-src corpus is migrated to inline Rust `#[test]` cases; the test process never parses a
`.phpt` file. See `tests/php_src/intl/README.md` and
`scripts/gen_php_src_intl_cases.py`.

```bash
cargo test --test codegen_tests test_php_src_intl
cargo test -p elephc-intl
```

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`collator_asort()`](./builtins/intl/collator_asort.md) | `(object $object, array $array, int $flags = Collator::SORT_REGULAR): bool` | `bool` | ✓ | - |
| [`collator_compare()`](./builtins/intl/collator_compare.md) | `(object $object, string $string1, string $string2): int|false` | `int|false` | ✓ | - |
| [`collator_create()`](./builtins/intl/collator_create.md) | `(string $locale): ?object` | `?object` | ✓ | - |
| [`collator_get_attribute()`](./builtins/intl/collator_get_attribute.md) | `(object $object, int $attribute): int|false` | `int|false` | ✓ | - |
| [`collator_get_error_code()`](./builtins/intl/collator_get_error_code.md) | `(object $object): int|false` | `int|false` | ✓ | - |
| [`collator_get_error_message()`](./builtins/intl/collator_get_error_message.md) | `(object $object): string|false` | `string|false` | ✓ | - |
| [`collator_get_locale()`](./builtins/intl/collator_get_locale.md) | `(object $object, int $type): string|false` | `string|false` | ✓ | - |
| [`collator_get_sort_key()`](./builtins/intl/collator_get_sort_key.md) | `(object $object, string $string): string|false` | `string|false` | ✓ | - |
| [`collator_get_strength()`](./builtins/intl/collator_get_strength.md) | `(object $object): int` | `int` | ✓ | - |
| [`collator_set_attribute()`](./builtins/intl/collator_set_attribute.md) | `(object $object, int $attribute, int $value): bool` | `bool` | ✓ | - |
| [`collator_set_strength()`](./builtins/intl/collator_set_strength.md) | `(object $object, int $strength): bool` | `bool` | ✓ | - |
| [`collator_sort()`](./builtins/intl/collator_sort.md) | `(object $object, array $array, int $flags = Collator::SORT_REGULAR): bool` | `bool` | ✓ | - |
| [`collator_sort_with_sort_keys()`](./builtins/intl/collator_sort_with_sort_keys.md) | `(object $object, array $array): bool` | `bool` | ✓ | - |
| [`datefmt_create()`](./builtins/intl/datefmt_create.md) | `(?string $locale, int $dateType = IntlDateFormatter::FULL, int $timeType = IntlDateFormatter::FULL, object|string|null $timezone = null, object|int|null $calendar = null, ?string $pattern = null): ?object` | `?object` | ✓ | - |
| [`datefmt_format()`](./builtins/intl/datefmt_format.md) | `(object $formatter, mixed $datetime): string|false` | `string|false` | ✓ | - |
| [`datefmt_format_object()`](./builtins/intl/datefmt_format_object.md) | `(mixed $datetime, mixed $format = null, ?string $locale = null): string|false` | `string|false` | ✓ | - |
| [`datefmt_get_calendar()`](./builtins/intl/datefmt_get_calendar.md) | `(object $formatter): int|false` | `int|false` | ✓ | - |
| [`datefmt_get_calendar_object()`](./builtins/intl/datefmt_get_calendar_object.md) | `(object $formatter): object|false|null` | `object|false|null` | ✓ | - |
| [`datefmt_get_datetype()`](./builtins/intl/datefmt_get_datetype.md) | `(object $formatter): int|false` | `int|false` | ✓ | - |
| [`datefmt_get_error_code()`](./builtins/intl/datefmt_get_error_code.md) | `(object $formatter): int` | `int` | ✓ | - |
| [`datefmt_get_error_message()`](./builtins/intl/datefmt_get_error_message.md) | `(object $formatter): string` | `string` | ✓ | - |
| [`datefmt_get_locale()`](./builtins/intl/datefmt_get_locale.md) | `(object $formatter, int $type = 0): string|false` | `string|false` | ✓ | - |
| [`datefmt_get_pattern()`](./builtins/intl/datefmt_get_pattern.md) | `(object $formatter): string|false` | `string|false` | ✓ | - |
| [`datefmt_get_timetype()`](./builtins/intl/datefmt_get_timetype.md) | `(object $formatter): int|false` | `int|false` | ✓ | - |
| [`datefmt_get_timezone()`](./builtins/intl/datefmt_get_timezone.md) | `(object $formatter): object|false` | `object|false` | ✓ | - |
| [`datefmt_get_timezone_id()`](./builtins/intl/datefmt_get_timezone_id.md) | `(object $formatter): string|false` | `string|false` | ✓ | - |
| [`datefmt_is_lenient()`](./builtins/intl/datefmt_is_lenient.md) | `(object $formatter): bool` | `bool` | ✓ | - |
| [`datefmt_localtime()`](./builtins/intl/datefmt_localtime.md) | `(object $formatter, string $string, int $offset = null): array|false` | `array|false` | ✓ | - |
| [`datefmt_parse()`](./builtins/intl/datefmt_parse.md) | `(object $formatter, string $string, int $offset = null): int|float|false` | `int|float|false` | ✓ | - |
| [`datefmt_set_calendar()`](./builtins/intl/datefmt_set_calendar.md) | `(object $formatter, object|int|null $calendar): bool` | `bool` | ✓ | - |
| [`datefmt_set_lenient()`](./builtins/intl/datefmt_set_lenient.md) | `(object $formatter, bool $lenient): void` | `void` | ✓ | - |
| [`datefmt_set_pattern()`](./builtins/intl/datefmt_set_pattern.md) | `(object $formatter, string $pattern): bool` | `bool` | ✓ | - |
| [`datefmt_set_timezone()`](./builtins/intl/datefmt_set_timezone.md) | `(object $formatter, object|string|null $timezone): bool` | `bool` | ✓ | - |
| [`grapheme_extract()`](./builtins/intl/grapheme_extract.md) | `(string $haystack, int $size, int $type = GRAPHEME_EXTR_COUNT, int $offset = 0, mixed $next = null): string|false` | `string|false` | ✓ | - |
| [`grapheme_levenshtein()`](./builtins/intl/grapheme_levenshtein.md) | `(string $string1, string $string2, int $insertion_cost = 1, int $replacement_cost = 1, int $deletion_cost = 1, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_str_split()`](./builtins/intl/grapheme_str_split.md) | `(string $string, int $length = 1): array|false` | `array|false` | ✓ | - |
| [`grapheme_stripos()`](./builtins/intl/grapheme_stripos.md) | `(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_stristr()`](./builtins/intl/grapheme_stristr.md) | `(string $haystack, string $needle, bool $beforeNeedle = false, string $locale = ''): string|false` | `string|false` | ✓ | - |
| [`grapheme_strlen()`](./builtins/intl/grapheme_strlen.md) | `(string $string): int|false|null` | `int|false|null` | ✓ | - |
| [`grapheme_strpos()`](./builtins/intl/grapheme_strpos.md) | `(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_strripos()`](./builtins/intl/grapheme_strripos.md) | `(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_strrpos()`](./builtins/intl/grapheme_strrpos.md) | `(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_strstr()`](./builtins/intl/grapheme_strstr.md) | `(string $haystack, string $needle, bool $beforeNeedle = false, string $locale = ''): string|false` | `string|false` | ✓ | - |
| [`grapheme_substr()`](./builtins/intl/grapheme_substr.md) | `(string $string, int $offset, ?int $length = null, string $locale = ''): string|false` | `string|false` | ✓ | - |
| [`idn_to_ascii()`](./builtins/intl/idn_to_ascii.md) | `(string $domain, int $flags = IDNA_DEFAULT, int $variant = INTL_IDNA_VARIANT_UTS46, mixed $idna_info = null): string|false` | `string|false` | ✓ | - |
| [`idn_to_utf8()`](./builtins/intl/idn_to_utf8.md) | `(string $domain, int $flags = IDNA_DEFAULT, int $variant = INTL_IDNA_VARIANT_UTS46, mixed $idna_info = null): string|false` | `string|false` | ✓ | - |
| [`intl_error_name()`](./builtins/intl/intl_error_name.md) | `(int $errorCode): string` | `string` | ✓ | - |
| [`intl_get_error_code()`](./builtins/intl/intl_get_error_code.md) | `(): int` | `int` | ✓ | - |
| [`intl_get_error_message()`](./builtins/intl/intl_get_error_message.md) | `(): string` | `string` | ✓ | - |
| [`intl_is_failure()`](./builtins/intl/intl_is_failure.md) | `(int $errorCode): bool` | `bool` | ✓ | - |
| [`intlcal_add()`](./builtins/intl/intlcal_add.md) | `(object $calendar, int $field, int $value): bool` | `bool` | ✓ | - |
| [`intlcal_after()`](./builtins/intl/intlcal_after.md) | `(object $calendar, object $other): bool` | `bool` | ✓ | - |
| [`intlcal_before()`](./builtins/intl/intlcal_before.md) | `(object $calendar, object $other): bool` | `bool` | ✓ | - |
| [`intlcal_clear()`](./builtins/intl/intlcal_clear.md) | `(object $calendar, ?int $field = null): bool` | `bool` | ✓ | - |
| [`intlcal_create_instance()`](./builtins/intl/intlcal_create_instance.md) | `(object|string|null $timezone = null, ?string $locale = null): ?object` | `?object` | ✓ | - |
| [`intlcal_equals()`](./builtins/intl/intlcal_equals.md) | `(object $calendar, object $other): bool` | `bool` | ✓ | - |
| [`intlcal_field_difference()`](./builtins/intl/intlcal_field_difference.md) | `(object $calendar, float $timestamp, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_from_date_time()`](./builtins/intl/intlcal_from_date_time.md) | `(object|string $datetime, ?string $locale = null): ?object` | `?object` | ✓ | - |
| [`intlcal_get()`](./builtins/intl/intlcal_get.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_actual_maximum()`](./builtins/intl/intlcal_get_actual_maximum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_actual_minimum()`](./builtins/intl/intlcal_get_actual_minimum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_available_locales()`](./builtins/intl/intlcal_get_available_locales.md) | `(): array` | `array` | ✓ | - |
| [`intlcal_get_day_of_week_type()`](./builtins/intl/intlcal_get_day_of_week_type.md) | `(object $calendar, int $dayOfWeek): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_error_code()`](./builtins/intl/intlcal_get_error_code.md) | `(object $calendar): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_error_message()`](./builtins/intl/intlcal_get_error_message.md) | `(object $calendar): string|false` | `string|false` | ✓ | - |
| [`intlcal_get_first_day_of_week()`](./builtins/intl/intlcal_get_first_day_of_week.md) | `(object $calendar): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_greatest_minimum()`](./builtins/intl/intlcal_get_greatest_minimum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_keyword_values_for_locale()`](./builtins/intl/intlcal_get_keyword_values_for_locale.md) | `(string $keyword, string $locale, bool $onlyCommon): object|false` | `object|false` | ✓ | - |
| [`intlcal_get_least_maximum()`](./builtins/intl/intlcal_get_least_maximum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_locale()`](./builtins/intl/intlcal_get_locale.md) | `(object $calendar, int $type): string|false` | `string|false` | ✓ | - |
| [`intlcal_get_maximum()`](./builtins/intl/intlcal_get_maximum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_minimal_days_in_first_week()`](./builtins/intl/intlcal_get_minimal_days_in_first_week.md) | `(object $calendar): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_minimum()`](./builtins/intl/intlcal_get_minimum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_now()`](./builtins/intl/intlcal_get_now.md) | `(): float` | `float` | ✓ | - |
| [`intlcal_get_repeated_wall_time_option()`](./builtins/intl/intlcal_get_repeated_wall_time_option.md) | `(object $calendar): int` | `int` | ✓ | - |
| [`intlcal_get_skipped_wall_time_option()`](./builtins/intl/intlcal_get_skipped_wall_time_option.md) | `(object $calendar): int` | `int` | ✓ | - |
| [`intlcal_get_time()`](./builtins/intl/intlcal_get_time.md) | `(object $calendar): float|false` | `float|false` | ✓ | - |
| [`intlcal_get_time_zone()`](./builtins/intl/intlcal_get_time_zone.md) | `(object $calendar): object|false` | `object|false` | ✓ | - |
| [`intlcal_get_type()`](./builtins/intl/intlcal_get_type.md) | `(object $calendar): string` | `string` | ✓ | - |
| [`intlcal_get_weekend_transition()`](./builtins/intl/intlcal_get_weekend_transition.md) | `(object $calendar, int $dayOfWeek): int|false` | `int|false` | ✓ | - |
| [`intlcal_in_daylight_time()`](./builtins/intl/intlcal_in_daylight_time.md) | `(object $calendar): bool` | `bool` | ✓ | - |
| [`intlcal_is_equivalent_to()`](./builtins/intl/intlcal_is_equivalent_to.md) | `(object $calendar, object $other): bool` | `bool` | ✓ | - |
| [`intlcal_is_lenient()`](./builtins/intl/intlcal_is_lenient.md) | `(object $calendar): bool` | `bool` | ✓ | - |
| [`intlcal_is_set()`](./builtins/intl/intlcal_is_set.md) | `(object $calendar, int $field): bool` | `bool` | ✓ | - |
| [`intlcal_is_weekend()`](./builtins/intl/intlcal_is_weekend.md) | `(object $calendar, ?float $timestamp = null): bool` | `bool` | ✓ | - |
| [`intlcal_roll()`](./builtins/intl/intlcal_roll.md) | `(object $calendar, int $field, mixed $value): bool` | `bool` | ✓ | - |
| [`intlcal_set()`](./builtins/intl/intlcal_set.md) | `(object $calendar, int $year, int $month, int $dayOfMonth = null, int $hour = null, int $minute = null, int $second = null): bool` | `bool` | ✓ | - |
| [`intlcal_set_first_day_of_week()`](./builtins/intl/intlcal_set_first_day_of_week.md) | `(object $calendar, int $dayOfWeek): bool` | `bool` | ✓ | - |
| [`intlcal_set_lenient()`](./builtins/intl/intlcal_set_lenient.md) | `(object $calendar, bool $lenient): bool` | `bool` | ✓ | - |
| [`intlcal_set_minimal_days_in_first_week()`](./builtins/intl/intlcal_set_minimal_days_in_first_week.md) | `(object $calendar, int $days): bool` | `bool` | ✓ | - |
| [`intlcal_set_repeated_wall_time_option()`](./builtins/intl/intlcal_set_repeated_wall_time_option.md) | `(object $calendar, int $option): bool` | `bool` | ✓ | - |
| [`intlcal_set_skipped_wall_time_option()`](./builtins/intl/intlcal_set_skipped_wall_time_option.md) | `(object $calendar, int $option): bool` | `bool` | ✓ | - |
| [`intlcal_set_time()`](./builtins/intl/intlcal_set_time.md) | `(object $calendar, float $timestamp): bool` | `bool` | ✓ | - |
| [`intlcal_set_time_zone()`](./builtins/intl/intlcal_set_time_zone.md) | `(object $calendar, object|string|null $timezone): bool` | `bool` | ✓ | - |
| [`intlcal_to_date_time()`](./builtins/intl/intlcal_to_date_time.md) | `(object $calendar): object|false` | `object|false` | ✓ | - |
| [`intlgregcal_create_instance()`](./builtins/intl/intlgregcal_create_instance.md) | `(mixed $timezoneOrYear = null, mixed $localeOrMonth = null, mixed $day = null, mixed $hour = null, mixed $minute = null, mixed $second = null): ?object` | `?object` | ✓ | - |
| [`intlgregcal_get_gregorian_change()`](./builtins/intl/intlgregcal_get_gregorian_change.md) | `(object $calendar): float` | `float` | ✓ | - |
| [`intlgregcal_is_leap_year()`](./builtins/intl/intlgregcal_is_leap_year.md) | `(object $calendar, int $year): bool` | `bool` | ✓ | - |
| [`intlgregcal_set_gregorian_change()`](./builtins/intl/intlgregcal_set_gregorian_change.md) | `(object $calendar, float $timestamp): bool` | `bool` | ✓ | - |
| [`intltz_count_equivalent_ids()`](./builtins/intl/intltz_count_equivalent_ids.md) | `(string $timezoneId): int|false` | `int|false` | ✓ | - |
| [`intltz_create_default()`](./builtins/intl/intltz_create_default.md) | `(): object` | `object` | ✓ | - |
| [`intltz_create_enumeration()`](./builtins/intl/intltz_create_enumeration.md) | `(string|int|null $countryOrRawOffset = null): object|false` | `object|false` | ✓ | - |
| [`intltz_create_time_zone()`](./builtins/intl/intltz_create_time_zone.md) | `(string $timezoneId): ?object` | `?object` | ✓ | - |
| [`intltz_create_time_zone_id_enumeration()`](./builtins/intl/intltz_create_time_zone_id_enumeration.md) | `(int $type, ?string $region = null, ?int $rawOffset = null): object|false` | `object|false` | ✓ | - |
| [`intltz_from_date_time_zone()`](./builtins/intl/intltz_from_date_time_zone.md) | `(object $timezone): ?object` | `?object` | ✓ | - |
| [`intltz_get_canonical_id()`](./builtins/intl/intltz_get_canonical_id.md) | `(string $timezoneId, mixed $isSystemId = null): string|false` | `string|false` | ✓ | - |
| [`intltz_get_display_name()`](./builtins/intl/intltz_get_display_name.md) | `(object $timezone, bool $dst = false, int $style = IntlTimeZone::DISPLAY_LONG, ?string $locale = null): string|false` | `string|false` | ✓ | - |
| [`intltz_get_dst_savings()`](./builtins/intl/intltz_get_dst_savings.md) | `(object $timezone): int` | `int` | ✓ | - |
| [`intltz_get_equivalent_id()`](./builtins/intl/intltz_get_equivalent_id.md) | `(string $timezoneId, int $offset): string|false` | `string|false` | ✓ | - |
| [`intltz_get_error_code()`](./builtins/intl/intltz_get_error_code.md) | `(object $timezone): int|false` | `int|false` | ✓ | - |
| [`intltz_get_error_message()`](./builtins/intl/intltz_get_error_message.md) | `(object $timezone): string|false` | `string|false` | ✓ | - |
| [`intltz_get_gmt()`](./builtins/intl/intltz_get_gmt.md) | `(): object` | `object` | ✓ | - |
| [`intltz_get_iana_id()`](./builtins/intl/intltz_get_iana_id.md) | `(string $timezoneId): string|false` | `string|false` | ✓ | - |
| [`intltz_get_id()`](./builtins/intl/intltz_get_id.md) | `(object $timezone): string|false` | `string|false` | ✓ | - |
| [`intltz_get_id_for_windows_id()`](./builtins/intl/intltz_get_id_for_windows_id.md) | `(string $timezoneId, ?string $region = null): string|false` | `string|false` | ✓ | - |
| [`intltz_get_offset()`](./builtins/intl/intltz_get_offset.md) | `(object $timezone, float $timestamp, bool $local, mixed $rawOffset, mixed $dstOffset): bool` | `bool` | ✓ | - |
| [`intltz_get_raw_offset()`](./builtins/intl/intltz_get_raw_offset.md) | `(object $timezone): int` | `int` | ✓ | - |
| [`intltz_get_region()`](./builtins/intl/intltz_get_region.md) | `(string $timezoneId): string|false` | `string|false` | ✓ | - |
| [`intltz_get_tz_data_version()`](./builtins/intl/intltz_get_tz_data_version.md) | `(): string|false` | `string|false` | ✓ | - |
| [`intltz_get_unknown()`](./builtins/intl/intltz_get_unknown.md) | `(): object` | `object` | ✓ | - |
| [`intltz_get_windows_id()`](./builtins/intl/intltz_get_windows_id.md) | `(string $timezoneId): string|false` | `string|false` | ✓ | - |
| [`intltz_has_same_rules()`](./builtins/intl/intltz_has_same_rules.md) | `(object $timezone, object $other): bool` | `bool` | ✓ | - |
| [`intltz_to_date_time_zone()`](./builtins/intl/intltz_to_date_time_zone.md) | `(object $timezone): object|false` | `object|false` | ✓ | - |
| [`intltz_use_daylight_time()`](./builtins/intl/intltz_use_daylight_time.md) | `(object $timezone): bool` | `bool` | ✓ | - |
| [`locale_accept_from_http()`](./builtins/intl/locale_accept_from_http.md) | `(string $header): string|false` | `string|false` | ✓ | - |
| [`locale_add_likely_subtags()`](./builtins/intl/locale_add_likely_subtags.md) | `(string $locale): string|false` | `string|false` | ✓ | - |
| [`locale_canonicalize()`](./builtins/intl/locale_canonicalize.md) | `(string $locale): ?string` | `?string` | ✓ | - |
| [`locale_compose()`](./builtins/intl/locale_compose.md) | `(array $subtags): string|false` | `string|false` | ✓ | - |
| [`locale_filter_matches()`](./builtins/intl/locale_filter_matches.md) | `(string $languageTag, string $locale, bool $canonicalize = false): ?bool` | `?bool` | ✓ | - |
| [`locale_get_all_variants()`](./builtins/intl/locale_get_all_variants.md) | `(string $locale): ?array` | `?array` | ✓ | - |
| [`locale_get_default()`](./builtins/intl/locale_get_default.md) | `(): string` | `string` | ✓ | - |
| [`locale_get_display_language()`](./builtins/intl/locale_get_display_language.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_display_name()`](./builtins/intl/locale_get_display_name.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_display_region()`](./builtins/intl/locale_get_display_region.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_display_script()`](./builtins/intl/locale_get_display_script.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_display_variant()`](./builtins/intl/locale_get_display_variant.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_keywords()`](./builtins/intl/locale_get_keywords.md) | `(string $locale): array|false|null` | `array|false|null` | ✓ | - |
| [`locale_get_primary_language()`](./builtins/intl/locale_get_primary_language.md) | `(string $locale): ?string` | `?string` | ✓ | - |
| [`locale_get_region()`](./builtins/intl/locale_get_region.md) | `(string $locale): ?string` | `?string` | ✓ | - |
| [`locale_get_script()`](./builtins/intl/locale_get_script.md) | `(string $locale): ?string` | `?string` | ✓ | - |
| [`locale_is_right_to_left()`](./builtins/intl/locale_is_right_to_left.md) | `(string $locale): bool` | `bool` | ✓ | - |
| [`locale_lookup()`](./builtins/intl/locale_lookup.md) | `(array $languageTag, string $locale, bool $canonicalize = false, ?string $defaultLocale = null): ?string` | `?string` | ✓ | - |
| [`locale_minimize_subtags()`](./builtins/intl/locale_minimize_subtags.md) | `(string $locale): string|false` | `string|false` | ✓ | - |
| [`locale_parse()`](./builtins/intl/locale_parse.md) | `(string $locale): ?array` | `?array` | ✓ | - |
| [`locale_set_default()`](./builtins/intl/locale_set_default.md) | `(string $locale): bool` | `bool` | ✓ | - |
| [`msgfmt_create()`](./builtins/intl/msgfmt_create.md) | `(string $locale, string $pattern): ?object` | `?object` | ✓ | - |
| [`msgfmt_format()`](./builtins/intl/msgfmt_format.md) | `(object $formatter, array $values): string|false` | `string|false` | ✓ | - |
| [`msgfmt_format_message()`](./builtins/intl/msgfmt_format_message.md) | `(string $locale, string $pattern, array $values): string|false` | `string|false` | ✓ | - |
| [`msgfmt_get_error_code()`](./builtins/intl/msgfmt_get_error_code.md) | `(object $formatter): int` | `int` | ✓ | - |
| [`msgfmt_get_error_message()`](./builtins/intl/msgfmt_get_error_message.md) | `(object $formatter): string` | `string` | ✓ | - |
| [`msgfmt_get_locale()`](./builtins/intl/msgfmt_get_locale.md) | `(object $formatter): string` | `string` | ✓ | - |
| [`msgfmt_get_pattern()`](./builtins/intl/msgfmt_get_pattern.md) | `(object $formatter): string|false` | `string|false` | ✓ | - |
| [`msgfmt_parse()`](./builtins/intl/msgfmt_parse.md) | `(object $formatter, string $string): array|false` | `array|false` | ✓ | - |
| [`msgfmt_parse_message()`](./builtins/intl/msgfmt_parse_message.md) | `(string $locale, string $pattern, string $message): array|false` | `array|false` | ✓ | - |
| [`msgfmt_set_pattern()`](./builtins/intl/msgfmt_set_pattern.md) | `(object $formatter, string $pattern): bool` | `bool` | ✓ | - |
| [`normalizer_get_raw_decomposition()`](./builtins/intl/normalizer_get_raw_decomposition.md) | `(string $string, int $form = Normalizer::FORM_C): ?string` | `?string` | ✓ | - |
| [`normalizer_is_normalized()`](./builtins/intl/normalizer_is_normalized.md) | `(string $string, int $form = Normalizer::FORM_C): bool` | `bool` | ✓ | - |
| [`normalizer_normalize()`](./builtins/intl/normalizer_normalize.md) | `(string $string, int $form = Normalizer::FORM_C): string|false` | `string|false` | ✓ | - |
| [`numfmt_create()`](./builtins/intl/numfmt_create.md) | `(string $locale, int $style, ?string $pattern = null): ?object` | `?object` | ✓ | - |
| [`numfmt_format()`](./builtins/intl/numfmt_format.md) | `(object $formatter, int|float $num, int $type = NumberFormatter::TYPE_DEFAULT): string|false` | `string|false` | ✓ | - |
| [`numfmt_format_currency()`](./builtins/intl/numfmt_format_currency.md) | `(object $formatter, float $amount, string $currency): string|false` | `string|false` | ✓ | - |
| [`numfmt_get_attribute()`](./builtins/intl/numfmt_get_attribute.md) | `(object $formatter, int $attribute): int|float|false` | `int|float|false` | ✓ | - |
| [`numfmt_get_error_code()`](./builtins/intl/numfmt_get_error_code.md) | `(object $formatter): int` | `int` | ✓ | - |
| [`numfmt_get_error_message()`](./builtins/intl/numfmt_get_error_message.md) | `(object $formatter): string` | `string` | ✓ | - |
| [`numfmt_get_locale()`](./builtins/intl/numfmt_get_locale.md) | `(object $formatter, int $type = 0): string|false` | `string|false` | ✓ | - |
| [`numfmt_get_pattern()`](./builtins/intl/numfmt_get_pattern.md) | `(object $formatter): string|false` | `string|false` | ✓ | - |
| [`numfmt_get_symbol()`](./builtins/intl/numfmt_get_symbol.md) | `(object $formatter, int $symbol): string|false` | `string|false` | ✓ | - |
| [`numfmt_get_text_attribute()`](./builtins/intl/numfmt_get_text_attribute.md) | `(object $formatter, int $attribute): string|false` | `string|false` | ✓ | - |
| [`numfmt_parse()`](./builtins/intl/numfmt_parse.md) | `(object $formatter, string $string, int $type = NumberFormatter::TYPE_DOUBLE, int $offset = null): int|float|false` | `int|float|false` | ✓ | - |
| [`numfmt_parse_currency()`](./builtins/intl/numfmt_parse_currency.md) | `(object $formatter, string $string, mixed $currency, int $offset = null): float|false` | `float|false` | ✓ | - |
| [`numfmt_set_attribute()`](./builtins/intl/numfmt_set_attribute.md) | `(object $formatter, int $attribute, int|float $value): bool` | `bool` | ✓ | - |
| [`numfmt_set_pattern()`](./builtins/intl/numfmt_set_pattern.md) | `(object $formatter, string $pattern): bool` | `bool` | ✓ | - |
| [`numfmt_set_symbol()`](./builtins/intl/numfmt_set_symbol.md) | `(object $formatter, int $symbol, string $value): bool` | `bool` | ✓ | - |
| [`numfmt_set_text_attribute()`](./builtins/intl/numfmt_set_text_attribute.md) | `(object $formatter, int $attribute, string $value): bool` | `bool` | ✓ | - |
| [`resourcebundle_count()`](./builtins/intl/resourcebundle_count.md) | `(object $bundle): int` | `int` | ✓ | - |
| [`resourcebundle_create()`](./builtins/intl/resourcebundle_create.md) | `(?string $locale, ?string $bundle, bool $fallback = true): ?object` | `?object` | ✓ | - |
| [`resourcebundle_get()`](./builtins/intl/resourcebundle_get.md) | `(object $bundle, string|int $index, bool $fallback = true): object|array|string|int|null` | `object|array|string|int|null` | ✓ | - |
| [`resourcebundle_get_error_code()`](./builtins/intl/resourcebundle_get_error_code.md) | `(object $bundle): int` | `int` | ✓ | - |
| [`resourcebundle_get_error_message()`](./builtins/intl/resourcebundle_get_error_message.md) | `(object $bundle): string` | `string` | ✓ | - |
| [`resourcebundle_locales()`](./builtins/intl/resourcebundle_locales.md) | `(string $bundle): array|false` | `array|false` | ✓ | - |
| [`transliterator_create()`](./builtins/intl/transliterator_create.md) | `(string $id, int $direction = Transliterator::FORWARD): ?object` | `?object` | ✓ | - |
| [`transliterator_create_from_rules()`](./builtins/intl/transliterator_create_from_rules.md) | `(string $rules, int $direction = Transliterator::FORWARD): ?object` | `?object` | ✓ | - |
| [`transliterator_create_inverse()`](./builtins/intl/transliterator_create_inverse.md) | `(object $transliterator): ?object` | `?object` | ✓ | - |
| [`transliterator_get_error_code()`](./builtins/intl/transliterator_get_error_code.md) | `(object $transliterator): int` | `int` | ✓ | - |
| [`transliterator_get_error_message()`](./builtins/intl/transliterator_get_error_message.md) | `(object $transliterator): string` | `string` | ✓ | - |
| [`transliterator_list_ids()`](./builtins/intl/transliterator_list_ids.md) | `(): array|false` | `array|false` | ✓ | - |
| [`transliterator_transliterate()`](./builtins/intl/transliterator_transliterate.md) | `(mixed $transliterator, string $string, int $start = 0, int $end = -1): string|false` | `string|false` | ✓ | - |

<!-- elephc:generated:symbols:end -->
