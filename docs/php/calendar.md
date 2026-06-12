---
title: "Calendar"
description: "The ext/calendar functions: Julian Day conversions for the Gregorian, Julian, French Republican and Jewish calendars, plus Easter and calendar metadata."
sidebar:
  order: 19
---

elephc implements PHP's `ext/calendar` extension. Every function is pure integer Julian-Day-Number arithmetic (a faithful port of PHP's `sdncal` algorithms), so results are byte-identical to PHP.

## Julian Day conversions

The **Julian Day Number** (JD) is a continuous day count used as the common pivot between calendars. Each calendar has a `*tojd` function and an inverse `jdto*` function that returns an `"m/d/y"` string (`"0/0/0"` for out-of-range input).

| Calendar | To JD | From JD |
|---|---|---|
| Gregorian | `gregoriantojd($month, $day, $year)` | `jdtogregorian($jd)` |
| Julian | `juliantojd($month, $day, $year)` | `jdtojulian($jd)` |
| French Republican | `frenchtojd($month, $day, $year)` | `jdtofrench($jd)` |
| Jewish | `jewishtojd($month, $day, $year)` | `jdtojewish($jd)` |

```php
$jd = gregoriantojd(1, 1, 2000);   // 2451545
echo jdtogregorian($jd);           // "1/1/2000"
echo jdtojewish($jd);              // "4/23/5760"
```

`jdtojewish()` accepts the PHP `$hebrew` and `$flags` arguments for signature compatibility, but always returns the numeric `"m/d/y"` form — Hebrew-script output is not produced.

## Generic dispatch

`cal_to_jd()`, `cal_from_jd()`, `cal_days_in_month()` and `cal_info()` take a calendar selector: `CAL_GREGORIAN` (0), `CAL_JULIAN` (1), `CAL_JEWISH` (2), `CAL_FRENCH` (3).

```php
cal_to_jd(CAL_GREGORIAN, 1, 1, 2000);        // 2451545
cal_days_in_month(CAL_GREGORIAN, 2, 2024);   // 29
cal_days_in_month(CAL_JEWISH, 1, 5784);      // 30 (Tishri)

$info = cal_from_jd(2451545, CAL_GREGORIAN);
// ["date" => "1/1/2000", "month" => 1, "day" => 1, "year" => 2000,
//  "dow" => 6, "abbrevdayname" => "Sat", "dayname" => "Saturday",
//  "abbrevmonth" => "Jan", "monthname" => "January"]

cal_info(CAL_JEWISH)["calname"];             // "Jewish"
count(cal_info());                            // 4 (all calendars when no argument)
```

## Day and month names

```php
jddayofweek($jd, CAL_DOW_DAYNO);   // 0-6 (Sunday = 0)
jddayofweek($jd, CAL_DOW_LONG);    // "Saturday"
jddayofweek($jd, CAL_DOW_SHORT);   // "Sat"

jdmonthname($jd, CAL_MONTH_GREGORIAN_LONG);   // "January"
jdmonthname($jd, CAL_MONTH_JULIAN_SHORT);     // "Jan"
jdmonthname($jd, CAL_MONTH_JEWISH);           // "Tishri"
jdmonthname($jd, CAL_MONTH_FRENCH);           // "Vendemiaire"
```

## Easter

```php
easter_days(2024);                              // 10  (days after March 21)
easter_days(1750, CAL_EASTER_ALWAYS_JULIAN);    // 25
easter_date(2024);                              // Unix timestamp of Easter midnight
```

`easter_days()`/`easter_date()` default the year to the current year. The `$mode` selector is one of `CAL_EASTER_DEFAULT` (0), `CAL_EASTER_ROMAN` (1), `CAL_EASTER_ALWAYS_GREGORIAN` (2), `CAL_EASTER_ALWAYS_JULIAN` (3). `easter_date()` returns a timestamp built with `mktime()`, so it follows the default timezone (UTC unless changed).

## Unix time

```php
unixtojd(0);            // 2440588  (Julian Day of the Unix epoch)
unixtojd();             // Julian Day of the current date
jdtounix(2440588);      // 0
```

`unixtojd()` with no argument uses the current time; passing an explicit `0` means the epoch.
