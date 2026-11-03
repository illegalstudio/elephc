---
title: "Intl builtins"
description: "Builtins in the Intl category."
sidebar:
  order: 123
---

## Intl builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`collator_asort()`](./intl/collator_asort.md) | `(object $object, array $array, int $flags = Collator::SORT_REGULAR): bool` | `bool` | ✓ | - |
| [`collator_compare()`](./intl/collator_compare.md) | `(object $object, string $string1, string $string2): int|false` | `int|false` | ✓ | - |
| [`collator_create()`](./intl/collator_create.md) | `(string $locale): ?object` | `?object` | ✓ | - |
| [`collator_get_attribute()`](./intl/collator_get_attribute.md) | `(object $object, int $attribute): int|false` | `int|false` | ✓ | - |
| [`collator_get_error_code()`](./intl/collator_get_error_code.md) | `(object $object): int|false` | `int|false` | ✓ | - |
| [`collator_get_error_message()`](./intl/collator_get_error_message.md) | `(object $object): string|false` | `string|false` | ✓ | - |
| [`collator_get_locale()`](./intl/collator_get_locale.md) | `(object $object, int $type): string|false` | `string|false` | ✓ | - |
| [`collator_get_sort_key()`](./intl/collator_get_sort_key.md) | `(object $object, string $string): string|false` | `string|false` | ✓ | - |
| [`collator_get_strength()`](./intl/collator_get_strength.md) | `(object $object): int` | `int` | ✓ | - |
| [`collator_set_attribute()`](./intl/collator_set_attribute.md) | `(object $object, int $attribute, int $value): bool` | `bool` | ✓ | - |
| [`collator_set_strength()`](./intl/collator_set_strength.md) | `(object $object, int $strength): bool` | `bool` | ✓ | - |
| [`collator_sort()`](./intl/collator_sort.md) | `(object $object, array $array, int $flags = Collator::SORT_REGULAR): bool` | `bool` | ✓ | - |
| [`collator_sort_with_sort_keys()`](./intl/collator_sort_with_sort_keys.md) | `(object $object, array $array): bool` | `bool` | ✓ | - |
| [`datefmt_create()`](./intl/datefmt_create.md) | `(?string $locale, int $dateType = IntlDateFormatter::FULL, int $timeType = IntlDateFormatter::FULL, object|string|null $timezone = null, object|int|null $calendar = null, ?string $pattern = null): ?object` | `?object` | ✓ | - |
| [`datefmt_format()`](./intl/datefmt_format.md) | `(object $formatter, mixed $datetime): string|false` | `string|false` | ✓ | - |
| [`datefmt_format_object()`](./intl/datefmt_format_object.md) | `(mixed $datetime, mixed $format = null, ?string $locale = null): string|false` | `string|false` | ✓ | - |
| [`datefmt_get_calendar()`](./intl/datefmt_get_calendar.md) | `(object $formatter): int|false` | `int|false` | ✓ | - |
| [`datefmt_get_calendar_object()`](./intl/datefmt_get_calendar_object.md) | `(object $formatter): object|false|null` | `object|false|null` | ✓ | - |
| [`datefmt_get_datetype()`](./intl/datefmt_get_datetype.md) | `(object $formatter): int|false` | `int|false` | ✓ | - |
| [`datefmt_get_error_code()`](./intl/datefmt_get_error_code.md) | `(object $formatter): int` | `int` | ✓ | - |
| [`datefmt_get_error_message()`](./intl/datefmt_get_error_message.md) | `(object $formatter): string` | `string` | ✓ | - |
| [`datefmt_get_locale()`](./intl/datefmt_get_locale.md) | `(object $formatter, int $type = 0): string|false` | `string|false` | ✓ | - |
| [`datefmt_get_pattern()`](./intl/datefmt_get_pattern.md) | `(object $formatter): string|false` | `string|false` | ✓ | - |
| [`datefmt_get_timetype()`](./intl/datefmt_get_timetype.md) | `(object $formatter): int|false` | `int|false` | ✓ | - |
| [`datefmt_get_timezone()`](./intl/datefmt_get_timezone.md) | `(object $formatter): object|false` | `object|false` | ✓ | - |
| [`datefmt_get_timezone_id()`](./intl/datefmt_get_timezone_id.md) | `(object $formatter): string|false` | `string|false` | ✓ | - |
| [`datefmt_is_lenient()`](./intl/datefmt_is_lenient.md) | `(object $formatter): bool` | `bool` | ✓ | - |
| [`datefmt_localtime()`](./intl/datefmt_localtime.md) | `(object $formatter, string $string, int $offset = null): array|false` | `array|false` | ✓ | - |
| [`datefmt_parse()`](./intl/datefmt_parse.md) | `(object $formatter, string $string, int $offset = null): int|float|false` | `int|float|false` | ✓ | - |
| [`datefmt_set_calendar()`](./intl/datefmt_set_calendar.md) | `(object $formatter, object|int|null $calendar): bool` | `bool` | ✓ | - |
| [`datefmt_set_lenient()`](./intl/datefmt_set_lenient.md) | `(object $formatter, bool $lenient): void` | `void` | ✓ | - |
| [`datefmt_set_pattern()`](./intl/datefmt_set_pattern.md) | `(object $formatter, string $pattern): bool` | `bool` | ✓ | - |
| [`datefmt_set_timezone()`](./intl/datefmt_set_timezone.md) | `(object $formatter, object|string|null $timezone): bool` | `bool` | ✓ | - |
| [`grapheme_extract()`](./intl/grapheme_extract.md) | `(string $haystack, int $size, int $type = GRAPHEME_EXTR_COUNT, int $offset = 0, mixed $next = null): string|false` | `string|false` | ✓ | - |
| [`grapheme_levenshtein()`](./intl/grapheme_levenshtein.md) | `(string $string1, string $string2, int $insertion_cost = 1, int $replacement_cost = 1, int $deletion_cost = 1, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_str_split()`](./intl/grapheme_str_split.md) | `(string $string, int $length = 1): array|false` | `array|false` | ✓ | - |
| [`grapheme_stripos()`](./intl/grapheme_stripos.md) | `(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_stristr()`](./intl/grapheme_stristr.md) | `(string $haystack, string $needle, bool $beforeNeedle = false, string $locale = ''): string|false` | `string|false` | ✓ | - |
| [`grapheme_strlen()`](./intl/grapheme_strlen.md) | `(string $string): int|false|null` | `int|false|null` | ✓ | - |
| [`grapheme_strpos()`](./intl/grapheme_strpos.md) | `(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_strripos()`](./intl/grapheme_strripos.md) | `(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_strrpos()`](./intl/grapheme_strrpos.md) | `(string $haystack, string $needle, int $offset = 0, string $locale = ''): int|false` | `int|false` | ✓ | - |
| [`grapheme_strstr()`](./intl/grapheme_strstr.md) | `(string $haystack, string $needle, bool $beforeNeedle = false, string $locale = ''): string|false` | `string|false` | ✓ | - |
| [`grapheme_substr()`](./intl/grapheme_substr.md) | `(string $string, int $offset, ?int $length = null, string $locale = ''): string|false` | `string|false` | ✓ | - |
| [`idn_to_ascii()`](./intl/idn_to_ascii.md) | `(string $domain, int $flags = IDNA_DEFAULT, int $variant = INTL_IDNA_VARIANT_UTS46, mixed $idna_info = null): string|false` | `string|false` | ✓ | - |
| [`idn_to_utf8()`](./intl/idn_to_utf8.md) | `(string $domain, int $flags = IDNA_DEFAULT, int $variant = INTL_IDNA_VARIANT_UTS46, mixed $idna_info = null): string|false` | `string|false` | ✓ | - |
| [`intl_error_name()`](./intl/intl_error_name.md) | `(int $errorCode): string` | `string` | ✓ | - |
| [`intl_get_error_code()`](./intl/intl_get_error_code.md) | `(): int` | `int` | ✓ | - |
| [`intl_get_error_message()`](./intl/intl_get_error_message.md) | `(): string` | `string` | ✓ | - |
| [`intl_is_failure()`](./intl/intl_is_failure.md) | `(int $errorCode): bool` | `bool` | ✓ | - |
| [`intlcal_add()`](./intl/intlcal_add.md) | `(object $calendar, int $field, int $value): bool` | `bool` | ✓ | - |
| [`intlcal_after()`](./intl/intlcal_after.md) | `(object $calendar, object $other): bool` | `bool` | ✓ | - |
| [`intlcal_before()`](./intl/intlcal_before.md) | `(object $calendar, object $other): bool` | `bool` | ✓ | - |
| [`intlcal_clear()`](./intl/intlcal_clear.md) | `(object $calendar, ?int $field = null): bool` | `bool` | ✓ | - |
| [`intlcal_create_instance()`](./intl/intlcal_create_instance.md) | `(object|string|null $timezone = null, ?string $locale = null): ?object` | `?object` | ✓ | - |
| [`intlcal_equals()`](./intl/intlcal_equals.md) | `(object $calendar, object $other): bool` | `bool` | ✓ | - |
| [`intlcal_field_difference()`](./intl/intlcal_field_difference.md) | `(object $calendar, float $timestamp, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_from_date_time()`](./intl/intlcal_from_date_time.md) | `(object|string $datetime, ?string $locale = null): ?object` | `?object` | ✓ | - |
| [`intlcal_get()`](./intl/intlcal_get.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_actual_maximum()`](./intl/intlcal_get_actual_maximum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_actual_minimum()`](./intl/intlcal_get_actual_minimum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_available_locales()`](./intl/intlcal_get_available_locales.md) | `(): array` | `array` | ✓ | - |
| [`intlcal_get_day_of_week_type()`](./intl/intlcal_get_day_of_week_type.md) | `(object $calendar, int $dayOfWeek): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_error_code()`](./intl/intlcal_get_error_code.md) | `(object $calendar): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_error_message()`](./intl/intlcal_get_error_message.md) | `(object $calendar): string|false` | `string|false` | ✓ | - |
| [`intlcal_get_first_day_of_week()`](./intl/intlcal_get_first_day_of_week.md) | `(object $calendar): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_greatest_minimum()`](./intl/intlcal_get_greatest_minimum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_keyword_values_for_locale()`](./intl/intlcal_get_keyword_values_for_locale.md) | `(string $keyword, string $locale, bool $onlyCommon): object|false` | `object|false` | ✓ | - |
| [`intlcal_get_least_maximum()`](./intl/intlcal_get_least_maximum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_locale()`](./intl/intlcal_get_locale.md) | `(object $calendar, int $type): string|false` | `string|false` | ✓ | - |
| [`intlcal_get_maximum()`](./intl/intlcal_get_maximum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_minimal_days_in_first_week()`](./intl/intlcal_get_minimal_days_in_first_week.md) | `(object $calendar): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_minimum()`](./intl/intlcal_get_minimum.md) | `(object $calendar, int $field): int|false` | `int|false` | ✓ | - |
| [`intlcal_get_now()`](./intl/intlcal_get_now.md) | `(): float` | `float` | ✓ | - |
| [`intlcal_get_repeated_wall_time_option()`](./intl/intlcal_get_repeated_wall_time_option.md) | `(object $calendar): int` | `int` | ✓ | - |
| [`intlcal_get_skipped_wall_time_option()`](./intl/intlcal_get_skipped_wall_time_option.md) | `(object $calendar): int` | `int` | ✓ | - |
| [`intlcal_get_time()`](./intl/intlcal_get_time.md) | `(object $calendar): float|false` | `float|false` | ✓ | - |
| [`intlcal_get_time_zone()`](./intl/intlcal_get_time_zone.md) | `(object $calendar): object|false` | `object|false` | ✓ | - |
| [`intlcal_get_type()`](./intl/intlcal_get_type.md) | `(object $calendar): string` | `string` | ✓ | - |
| [`intlcal_get_weekend_transition()`](./intl/intlcal_get_weekend_transition.md) | `(object $calendar, int $dayOfWeek): int|false` | `int|false` | ✓ | - |
| [`intlcal_in_daylight_time()`](./intl/intlcal_in_daylight_time.md) | `(object $calendar): bool` | `bool` | ✓ | - |
| [`intlcal_is_equivalent_to()`](./intl/intlcal_is_equivalent_to.md) | `(object $calendar, object $other): bool` | `bool` | ✓ | - |
| [`intlcal_is_lenient()`](./intl/intlcal_is_lenient.md) | `(object $calendar): bool` | `bool` | ✓ | - |
| [`intlcal_is_set()`](./intl/intlcal_is_set.md) | `(object $calendar, int $field): bool` | `bool` | ✓ | - |
| [`intlcal_is_weekend()`](./intl/intlcal_is_weekend.md) | `(object $calendar, ?float $timestamp = null): bool` | `bool` | ✓ | - |
| [`intlcal_roll()`](./intl/intlcal_roll.md) | `(object $calendar, int $field, mixed $value): bool` | `bool` | ✓ | - |
| [`intlcal_set()`](./intl/intlcal_set.md) | `(object $calendar, int $year, int $month, int $dayOfMonth = null, int $hour = null, int $minute = null, int $second = null): bool` | `bool` | ✓ | - |
| [`intlcal_set_first_day_of_week()`](./intl/intlcal_set_first_day_of_week.md) | `(object $calendar, int $dayOfWeek): bool` | `bool` | ✓ | - |
| [`intlcal_set_lenient()`](./intl/intlcal_set_lenient.md) | `(object $calendar, bool $lenient): bool` | `bool` | ✓ | - |
| [`intlcal_set_minimal_days_in_first_week()`](./intl/intlcal_set_minimal_days_in_first_week.md) | `(object $calendar, int $days): bool` | `bool` | ✓ | - |
| [`intlcal_set_repeated_wall_time_option()`](./intl/intlcal_set_repeated_wall_time_option.md) | `(object $calendar, int $option): bool` | `bool` | ✓ | - |
| [`intlcal_set_skipped_wall_time_option()`](./intl/intlcal_set_skipped_wall_time_option.md) | `(object $calendar, int $option): bool` | `bool` | ✓ | - |
| [`intlcal_set_time()`](./intl/intlcal_set_time.md) | `(object $calendar, float $timestamp): bool` | `bool` | ✓ | - |
| [`intlcal_set_time_zone()`](./intl/intlcal_set_time_zone.md) | `(object $calendar, object|string|null $timezone): bool` | `bool` | ✓ | - |
| [`intlcal_to_date_time()`](./intl/intlcal_to_date_time.md) | `(object $calendar): object|false` | `object|false` | ✓ | - |
| [`intlgregcal_create_instance()`](./intl/intlgregcal_create_instance.md) | `(mixed $timezoneOrYear = null, mixed $localeOrMonth = null, mixed $day = null, mixed $hour = null, mixed $minute = null, mixed $second = null): ?object` | `?object` | ✓ | - |
| [`intlgregcal_get_gregorian_change()`](./intl/intlgregcal_get_gregorian_change.md) | `(object $calendar): float` | `float` | ✓ | - |
| [`intlgregcal_is_leap_year()`](./intl/intlgregcal_is_leap_year.md) | `(object $calendar, int $year): bool` | `bool` | ✓ | - |
| [`intlgregcal_set_gregorian_change()`](./intl/intlgregcal_set_gregorian_change.md) | `(object $calendar, float $timestamp): bool` | `bool` | ✓ | - |
| [`intltz_count_equivalent_ids()`](./intl/intltz_count_equivalent_ids.md) | `(string $timezoneId): int|false` | `int|false` | ✓ | - |
| [`intltz_create_default()`](./intl/intltz_create_default.md) | `(): object` | `object` | ✓ | - |
| [`intltz_create_enumeration()`](./intl/intltz_create_enumeration.md) | `(string|int|null $countryOrRawOffset = null): object|false` | `object|false` | ✓ | - |
| [`intltz_create_time_zone()`](./intl/intltz_create_time_zone.md) | `(string $timezoneId): ?object` | `?object` | ✓ | - |
| [`intltz_create_time_zone_id_enumeration()`](./intl/intltz_create_time_zone_id_enumeration.md) | `(int $type, ?string $region = null, ?int $rawOffset = null): object|false` | `object|false` | ✓ | - |
| [`intltz_from_date_time_zone()`](./intl/intltz_from_date_time_zone.md) | `(object $timezone): ?object` | `?object` | ✓ | - |
| [`intltz_get_canonical_id()`](./intl/intltz_get_canonical_id.md) | `(string $timezoneId, bool $isSystemId = null): string|false` | `string|false` | ✓ | - |
| [`intltz_get_display_name()`](./intl/intltz_get_display_name.md) | `(object $timezone, bool $dst = false, int $style = IntlTimeZone::DISPLAY_LONG, ?string $locale = null): string|false` | `string|false` | ✓ | - |
| [`intltz_get_dst_savings()`](./intl/intltz_get_dst_savings.md) | `(object $timezone): int` | `int` | ✓ | - |
| [`intltz_get_equivalent_id()`](./intl/intltz_get_equivalent_id.md) | `(string $timezoneId, int $offset): string|false` | `string|false` | ✓ | - |
| [`intltz_get_error_code()`](./intl/intltz_get_error_code.md) | `(object $timezone): int|false` | `int|false` | ✓ | - |
| [`intltz_get_error_message()`](./intl/intltz_get_error_message.md) | `(object $timezone): string|false` | `string|false` | ✓ | - |
| [`intltz_get_gmt()`](./intl/intltz_get_gmt.md) | `(): object` | `object` | ✓ | - |
| [`intltz_get_iana_id()`](./intl/intltz_get_iana_id.md) | `(string $timezoneId): string|false` | `string|false` | ✓ | - |
| [`intltz_get_id()`](./intl/intltz_get_id.md) | `(object $timezone): string|false` | `string|false` | ✓ | - |
| [`intltz_get_id_for_windows_id()`](./intl/intltz_get_id_for_windows_id.md) | `(string $timezoneId, ?string $region = null): string|false` | `string|false` | ✓ | - |
| [`intltz_get_offset()`](./intl/intltz_get_offset.md) | `(object $timezone, float $timestamp, bool $local, int $rawOffset, int $dstOffset): bool` | `bool` | ✓ | - |
| [`intltz_get_raw_offset()`](./intl/intltz_get_raw_offset.md) | `(object $timezone): int` | `int` | ✓ | - |
| [`intltz_get_region()`](./intl/intltz_get_region.md) | `(string $timezoneId): string|false` | `string|false` | ✓ | - |
| [`intltz_get_tz_data_version()`](./intl/intltz_get_tz_data_version.md) | `(): string|false` | `string|false` | ✓ | - |
| [`intltz_get_unknown()`](./intl/intltz_get_unknown.md) | `(): object` | `object` | ✓ | - |
| [`intltz_get_windows_id()`](./intl/intltz_get_windows_id.md) | `(string $timezoneId): string|false` | `string|false` | ✓ | - |
| [`intltz_has_same_rules()`](./intl/intltz_has_same_rules.md) | `(object $timezone, object $other): bool` | `bool` | ✓ | - |
| [`intltz_to_date_time_zone()`](./intl/intltz_to_date_time_zone.md) | `(object $timezone): object|false` | `object|false` | ✓ | - |
| [`intltz_use_daylight_time()`](./intl/intltz_use_daylight_time.md) | `(object $timezone): bool` | `bool` | ✓ | - |
| [`locale_accept_from_http()`](./intl/locale_accept_from_http.md) | `(string $header): string|false` | `string|false` | ✓ | - |
| [`locale_add_likely_subtags()`](./intl/locale_add_likely_subtags.md) | `(string $locale): string|false` | `string|false` | ✓ | - |
| [`locale_canonicalize()`](./intl/locale_canonicalize.md) | `(string $locale): ?string` | `?string` | ✓ | - |
| [`locale_compose()`](./intl/locale_compose.md) | `(array $subtags): string|false` | `string|false` | ✓ | - |
| [`locale_filter_matches()`](./intl/locale_filter_matches.md) | `(string $languageTag, string $locale, bool $canonicalize = false): ?bool` | `?bool` | ✓ | - |
| [`locale_get_all_variants()`](./intl/locale_get_all_variants.md) | `(string $locale): ?array` | `?array` | ✓ | - |
| [`locale_get_default()`](./intl/locale_get_default.md) | `(): string` | `string` | ✓ | - |
| [`locale_get_display_language()`](./intl/locale_get_display_language.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_display_name()`](./intl/locale_get_display_name.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_display_region()`](./intl/locale_get_display_region.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_display_script()`](./intl/locale_get_display_script.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_display_variant()`](./intl/locale_get_display_variant.md) | `(string $locale, ?string $displayLocale = null): string|false` | `string|false` | ✓ | - |
| [`locale_get_keywords()`](./intl/locale_get_keywords.md) | `(string $locale): array|false|null` | `array|false|null` | ✓ | - |
| [`locale_get_primary_language()`](./intl/locale_get_primary_language.md) | `(string $locale): ?string` | `?string` | ✓ | - |
| [`locale_get_region()`](./intl/locale_get_region.md) | `(string $locale): ?string` | `?string` | ✓ | - |
| [`locale_get_script()`](./intl/locale_get_script.md) | `(string $locale): ?string` | `?string` | ✓ | - |
| [`locale_is_right_to_left()`](./intl/locale_is_right_to_left.md) | `(string $locale): bool` | `bool` | ✓ | - |
| [`locale_lookup()`](./intl/locale_lookup.md) | `(array $languageTag, string $locale, bool $canonicalize = false, ?string $defaultLocale = null): ?string` | `?string` | ✓ | - |
| [`locale_minimize_subtags()`](./intl/locale_minimize_subtags.md) | `(string $locale): string|false` | `string|false` | ✓ | - |
| [`locale_parse()`](./intl/locale_parse.md) | `(string $locale): ?array` | `?array` | ✓ | - |
| [`locale_set_default()`](./intl/locale_set_default.md) | `(string $locale): bool` | `bool` | ✓ | - |
| [`msgfmt_create()`](./intl/msgfmt_create.md) | `(string $locale, string $pattern): ?object` | `?object` | ✓ | - |
| [`msgfmt_format()`](./intl/msgfmt_format.md) | `(object $formatter, array $values): string|false` | `string|false` | ✓ | - |
| [`msgfmt_format_message()`](./intl/msgfmt_format_message.md) | `(string $locale, string $pattern, array $values): string|false` | `string|false` | ✓ | - |
| [`msgfmt_get_error_code()`](./intl/msgfmt_get_error_code.md) | `(object $formatter): int` | `int` | ✓ | - |
| [`msgfmt_get_error_message()`](./intl/msgfmt_get_error_message.md) | `(object $formatter): string` | `string` | ✓ | - |
| [`msgfmt_get_locale()`](./intl/msgfmt_get_locale.md) | `(object $formatter): string` | `string` | ✓ | - |
| [`msgfmt_get_pattern()`](./intl/msgfmt_get_pattern.md) | `(object $formatter): string|false` | `string|false` | ✓ | - |
| [`msgfmt_parse()`](./intl/msgfmt_parse.md) | `(object $formatter, string $string): array|false` | `array|false` | ✓ | - |
| [`msgfmt_parse_message()`](./intl/msgfmt_parse_message.md) | `(string $locale, string $pattern, string $message): array|false` | `array|false` | ✓ | - |
| [`msgfmt_set_pattern()`](./intl/msgfmt_set_pattern.md) | `(object $formatter, string $pattern): bool` | `bool` | ✓ | - |
| [`normalizer_get_raw_decomposition()`](./intl/normalizer_get_raw_decomposition.md) | `(string $string, int $form = Normalizer::FORM_C): ?string` | `?string` | ✓ | - |
| [`normalizer_is_normalized()`](./intl/normalizer_is_normalized.md) | `(string $string, int $form = Normalizer::FORM_C): bool` | `bool` | ✓ | - |
| [`normalizer_normalize()`](./intl/normalizer_normalize.md) | `(string $string, int $form = Normalizer::FORM_C): string|false` | `string|false` | ✓ | - |
| [`numfmt_create()`](./intl/numfmt_create.md) | `(string $locale, int $style, ?string $pattern = null): ?object` | `?object` | ✓ | - |
| [`numfmt_format()`](./intl/numfmt_format.md) | `(object $formatter, int|float $num, int $type = NumberFormatter::TYPE_DEFAULT): string|false` | `string|false` | ✓ | - |
| [`numfmt_format_currency()`](./intl/numfmt_format_currency.md) | `(object $formatter, float $amount, string $currency): string|false` | `string|false` | ✓ | - |
| [`numfmt_get_attribute()`](./intl/numfmt_get_attribute.md) | `(object $formatter, int $attribute): int|float|false` | `int|float|false` | ✓ | - |
| [`numfmt_get_error_code()`](./intl/numfmt_get_error_code.md) | `(object $formatter): int` | `int` | ✓ | - |
| [`numfmt_get_error_message()`](./intl/numfmt_get_error_message.md) | `(object $formatter): string` | `string` | ✓ | - |
| [`numfmt_get_locale()`](./intl/numfmt_get_locale.md) | `(object $formatter, int $type = 0): string|false` | `string|false` | ✓ | - |
| [`numfmt_get_pattern()`](./intl/numfmt_get_pattern.md) | `(object $formatter): string|false` | `string|false` | ✓ | - |
| [`numfmt_get_symbol()`](./intl/numfmt_get_symbol.md) | `(object $formatter, int $symbol): string|false` | `string|false` | ✓ | - |
| [`numfmt_get_text_attribute()`](./intl/numfmt_get_text_attribute.md) | `(object $formatter, int $attribute): string|false` | `string|false` | ✓ | - |
| [`numfmt_parse()`](./intl/numfmt_parse.md) | `(object $formatter, string $string, int $type = NumberFormatter::TYPE_DOUBLE, int $offset = null): int|float|false` | `int|float|false` | ✓ | - |
| [`numfmt_parse_currency()`](./intl/numfmt_parse_currency.md) | `(object $formatter, string $string, mixed $currency, int $offset = null): float|false` | `float|false` | ✓ | - |
| [`numfmt_set_attribute()`](./intl/numfmt_set_attribute.md) | `(object $formatter, int $attribute, int|float $value): bool` | `bool` | ✓ | - |
| [`numfmt_set_pattern()`](./intl/numfmt_set_pattern.md) | `(object $formatter, string $pattern): bool` | `bool` | ✓ | - |
| [`numfmt_set_symbol()`](./intl/numfmt_set_symbol.md) | `(object $formatter, int $symbol, string $value): bool` | `bool` | ✓ | - |
| [`numfmt_set_text_attribute()`](./intl/numfmt_set_text_attribute.md) | `(object $formatter, int $attribute, string $value): bool` | `bool` | ✓ | - |
| [`resourcebundle_count()`](./intl/resourcebundle_count.md) | `(object $bundle): int` | `int` | ✓ | - |
| [`resourcebundle_create()`](./intl/resourcebundle_create.md) | `(?string $locale, ?string $bundle, bool $fallback = true): ?object` | `?object` | ✓ | - |
| [`resourcebundle_get()`](./intl/resourcebundle_get.md) | `(object $bundle, string|int $index, bool $fallback = true): object|array|string|int|null` | `object|array|string|int|null` | ✓ | - |
| [`resourcebundle_get_error_code()`](./intl/resourcebundle_get_error_code.md) | `(object $bundle): int` | `int` | ✓ | - |
| [`resourcebundle_get_error_message()`](./intl/resourcebundle_get_error_message.md) | `(object $bundle): string` | `string` | ✓ | - |
| [`resourcebundle_locales()`](./intl/resourcebundle_locales.md) | `(string $bundle): array|false` | `array|false` | ✓ | - |
| [`transliterator_create()`](./intl/transliterator_create.md) | `(string $id, int $direction = Transliterator::FORWARD): ?object` | `?object` | ✓ | - |
| [`transliterator_create_from_rules()`](./intl/transliterator_create_from_rules.md) | `(string $rules, int $direction = Transliterator::FORWARD): ?object` | `?object` | ✓ | - |
| [`transliterator_create_inverse()`](./intl/transliterator_create_inverse.md) | `(object $transliterator): ?object` | `?object` | ✓ | - |
| [`transliterator_get_error_code()`](./intl/transliterator_get_error_code.md) | `(object $transliterator): int` | `int` | ✓ | - |
| [`transliterator_get_error_message()`](./intl/transliterator_get_error_message.md) | `(object $transliterator): string` | `string` | ✓ | - |
| [`transliterator_list_ids()`](./intl/transliterator_list_ids.md) | `(): array|false` | `array|false` | ✓ | - |
| [`transliterator_transliterate()`](./intl/transliterator_transliterate.md) | `(mixed $transliterator, string $string, int $start = 0, int $end = -1): string|false` | `string|false` | ✓ | - |
