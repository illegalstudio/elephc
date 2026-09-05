---
title: "Date builtins"
description: "Builtins in the Date category."
sidebar:
  order: 107
---

## Date builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`cal_days_in_month()`](./date/cal_days_in_month.md) | `(int $calendar, int $month, int $year): int` | `int` | ✓ | ✓ |
| [`cal_from_jd()`](./date/cal_from_jd.md) | `(int $julian_day, int $calendar): array` | `array` | ✓ | ✓ |
| [`cal_info()`](./date/cal_info.md) | `(int $calendar = -1): array` | `array` | ✓ | ✓ |
| [`cal_to_jd()`](./date/cal_to_jd.md) | `(int $calendar, int $month, int $day, int $year): int` | `int` | ✓ | ✓ |
| [`checkdate()`](./date/checkdate.md) | `(int $month, int $day, int $year): bool` | `bool` | ✓ | ✓ |
| [`date()`](./date/date.md) | `(string $format, ?int $timestamp = null): string` | `string` | ✓ | ✓ |
| [`date_add()`](./date/date_add.md) | `(mixed $object, mixed $interval): mixed` | `mixed` | ✓ | ✓ |
| [`date_create()`](./date/date_create.md) | `(string $datetime = 'now', mixed $timezone = null): mixed` | `mixed` | ✓ | ✓ |
| [`date_create_from_format()`](./date/date_create_from_format.md) | `(string $format, string $datetime, mixed $timezone = null): mixed` | `mixed` | ✓ | ✓ |
| [`date_create_immutable()`](./date/date_create_immutable.md) | `(string $datetime = 'now', mixed $timezone = null): mixed` | `mixed` | ✓ | ✓ |
| [`date_create_immutable_from_format()`](./date/date_create_immutable_from_format.md) | `(string $format, string $datetime, mixed $timezone = null): mixed` | `mixed` | ✓ | ✓ |
| [`date_date_set()`](./date/date_date_set.md) | `(mixed $object, int $year, int $month, int $day): mixed` | `mixed` | ✓ | ✓ |
| [`date_default_timezone_get()`](./date/date_default_timezone_get.md) | `(): string` | `string` | ✓ | ✓ |
| [`date_default_timezone_set()`](./date/date_default_timezone_set.md) | `(string $timezoneId): bool` | `bool` | ✓ | ✓ |
| [`date_diff()`](./date/date_diff.md) | `(mixed $baseObject, mixed $targetObject, bool $absolute = false): mixed` | `mixed` | ✓ | ✓ |
| [`date_format()`](./date/date_format.md) | `(mixed $object, string $format): string` | `string` | ✓ | ✓ |
| [`date_get_last_errors()`](./date/date_get_last_errors.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`date_interval_create_from_date_string()`](./date/date_interval_create_from_date_string.md) | `(string $datetime): mixed` | `mixed` | ✓ | ✓ |
| [`date_interval_format()`](./date/date_interval_format.md) | `(mixed $object, string $format): string` | `string` | ✓ | ✓ |
| [`date_isodate_set()`](./date/date_isodate_set.md) | `(mixed $object, int $year, int $week, int $dayOfWeek = 1): mixed` | `mixed` | ✓ | ✓ |
| [`date_modify()`](./date/date_modify.md) | `(mixed $object, string $modifier): mixed` | `mixed` | ✓ | ✓ |
| [`date_offset_get()`](./date/date_offset_get.md) | `(mixed $object): int` | `int` | ✓ | ✓ |
| [`date_parse()`](./date/date_parse.md) | `(string $datetime): array` | `array` | ✓ | ✓ |
| [`date_parse_from_format()`](./date/date_parse_from_format.md) | `(string $format, string $datetime): array` | `array` | ✓ | ✓ |
| [`date_sub()`](./date/date_sub.md) | `(mixed $object, mixed $interval): mixed` | `mixed` | ✓ | ✓ |
| [`date_sun_info()`](./date/date_sun_info.md) | `(int $timestamp, float $latitude, float $longitude): array` | `array` | ✓ | ✓ |
| [`date_sunrise()`](./date/date_sunrise.md) | `(int $timestamp, int $returnFormat = SUNFUNCS_RET_STRING, ?float $latitude = null, ?float $longitude = null, ?float $zenith = null, ?float $utcOffset = null): mixed` | `mixed` | ✓ | ✓ |
| [`date_sunset()`](./date/date_sunset.md) | `(int $timestamp, int $returnFormat = SUNFUNCS_RET_STRING, ?float $latitude = null, ?float $longitude = null, ?float $zenith = null, ?float $utcOffset = null): mixed` | `mixed` | ✓ | ✓ |
| [`date_time_set()`](./date/date_time_set.md) | `(mixed $object, int $hour, int $minute, int $second = 0, int $microsecond = 0): mixed` | `mixed` | ✓ | ✓ |
| [`date_timestamp_get()`](./date/date_timestamp_get.md) | `(mixed $object): int` | `int` | ✓ | ✓ |
| [`date_timestamp_set()`](./date/date_timestamp_set.md) | `(mixed $object, int $timestamp): mixed` | `mixed` | ✓ | ✓ |
| [`date_timezone_get()`](./date/date_timezone_get.md) | `(mixed $object): mixed` | `mixed` | ✓ | ✓ |
| [`date_timezone_set()`](./date/date_timezone_set.md) | `(mixed $object, mixed $timezone): mixed` | `mixed` | ✓ | ✓ |
| [`easter_date()`](./date/easter_date.md) | `(?int $year = null, int $mode = CAL_EASTER_DEFAULT): int` | `int` | ✓ | ✓ |
| [`easter_days()`](./date/easter_days.md) | `(?int $year = null, int $mode = CAL_EASTER_DEFAULT): int` | `int` | ✓ | ✓ |
| [`frenchtojd()`](./date/frenchtojd.md) | `(int $month, int $day, int $year): int` | `int` | ✓ | ✓ |
| [`getdate()`](./date/getdate.md) | `(?int $timestamp = null): array` | `array` | ✓ | ✓ |
| [`gettimeofday()`](./date/gettimeofday.md) | `(bool $as_float = false): mixed` | `mixed` | ✓ | ✓ |
| [`gmdate()`](./date/gmdate.md) | `(string $format, ?int $timestamp = null): string` | `string` | ✓ | ✓ |
| [`gmmktime()`](./date/gmmktime.md) | `(int $hour, int $minute, int $second, int $month, int $day, int $year): int` | `int` | ✓ | ✓ |
| [`gmstrftime()`](./date/gmstrftime.md) | `(string $format, ?int $timestamp = null): mixed` | `mixed` | ✓ | ✓ |
| [`gregoriantojd()`](./date/gregoriantojd.md) | `(int $month, int $day, int $year): int` | `int` | ✓ | ✓ |
| [`hrtime()`](./date/hrtime.md) | `(bool $as_number = false): mixed` | `mixed` | ✓ | ✓ |
| [`idate()`](./date/idate.md) | `(string $format, ?int $timestamp = null): mixed` | `mixed` | ✓ | ✓ |
| [`jddayofweek()`](./date/jddayofweek.md) | `(int $julian_day, int $mode = CAL_DOW_DAYNO): mixed` | `mixed` | ✓ | ✓ |
| [`jdmonthname()`](./date/jdmonthname.md) | `(int $julian_day, int $mode): string` | `string` | ✓ | ✓ |
| [`jdtofrench()`](./date/jdtofrench.md) | `(int $julian_day): string` | `string` | ✓ | ✓ |
| [`jdtogregorian()`](./date/jdtogregorian.md) | `(int $julian_day): string` | `string` | ✓ | ✓ |
| [`jdtojewish()`](./date/jdtojewish.md) | `(int $julian_day, bool $hebrew = false, int $flags = 0): string` | `string` | ✓ | ✓ |
| [`jdtojulian()`](./date/jdtojulian.md) | `(int $julian_day): string` | `string` | ✓ | ✓ |
| [`jdtounix()`](./date/jdtounix.md) | `(int $julian_day): int` | `int` | ✓ | ✓ |
| [`jewishtojd()`](./date/jewishtojd.md) | `(int $month, int $day, int $year): int` | `int` | ✓ | ✓ |
| [`juliantojd()`](./date/juliantojd.md) | `(int $month, int $day, int $year): int` | `int` | ✓ | ✓ |
| [`localtime()`](./date/localtime.md) | `(int $timestamp = -1, bool $associative = false): array` | `array` | ✓ | ✓ |
| [`microtime()`](./date/microtime.md) | `(bool $as_float = false): mixed` | `mixed` | ✓ | ✓ |
| [`mktime()`](./date/mktime.md) | `(int $hour, int $minute, int $second, int $month, int $day, int $year): int` | `int` | ✓ | ✓ |
| [`strftime()`](./date/strftime.md) | `(string $format, ?int $timestamp = null): mixed` | `mixed` | ✓ | ✓ |
| [`strptime()`](./date/strptime.md) | `(string $timestamp, string $format): mixed` | `mixed` | ✓ | ✓ |
| [`strtotime()`](./date/strtotime.md) | `(string $datetime, ?int $baseTimestamp = null): mixed` | `mixed` | ✓ | ✓ |
| [`time()`](./date/time.md) | `(): int` | `int` | ✓ | ✓ |
| [`timezone_abbreviations_list()`](./date/timezone_abbreviations_list.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`timezone_identifiers_list()`](./date/timezone_identifiers_list.md) | `(int $timezoneGroup = DateTimeZone::ALL, ?string $countryCode = null): array` | `array` | ✓ | ✓ |
| [`timezone_location_get()`](./date/timezone_location_get.md) | `(mixed $object): mixed` | `mixed` | ✓ | ✓ |
| [`timezone_name_from_abbr()`](./date/timezone_name_from_abbr.md) | `(string $abbr, int $utcOffset = -1, int $isDST = -1): mixed` | `mixed` | ✓ | ✓ |
| [`timezone_name_get()`](./date/timezone_name_get.md) | `(mixed $object): string` | `string` | ✓ | ✓ |
| [`timezone_offset_get()`](./date/timezone_offset_get.md) | `(mixed $object, mixed $datetime): int` | `int` | ✓ | ✓ |
| [`timezone_open()`](./date/timezone_open.md) | `(string $timezone): mixed` | `mixed` | ✓ | ✓ |
| [`timezone_transitions_get()`](./date/timezone_transitions_get.md) | `(mixed $object, int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX): mixed` | `mixed` | ✓ | ✓ |
| [`timezone_version_get()`](./date/timezone_version_get.md) | `(): string` | `string` | ✓ | ✓ |
| [`unixtojd()`](./date/unixtojd.md) | `(?int $timestamp = null): mixed` | `mixed` | ✓ | ✓ |
