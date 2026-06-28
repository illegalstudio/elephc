---
title: "Date and Time"
description: "Built-in DateTime, DateTimeImmutable, DateTimeZone, and DateInterval classes for working with dates and time intervals."
sidebar:
  order: 18
---

elephc provides the object-oriented date/time classes from PHP's standard library: `DateTime`, `DateTimeImmutable`, `DateTimeInterface`, `DateTimeZone`, and `DateInterval`. They are compiled, not interpreted, and are built on top of the same `date()`, `mktime()`, and `strtotime()` builtins documented in [System & I/O](system-and-io.md).

## DateTimeZone

Holds a time-zone identifier.

| Method | Description |
|---|---|
| `__construct(string $name)` | Store the zone identifier (e.g. `"UTC"`, `"Europe/Paris"`). |
| `getName(): string` | Return the stored identifier. |
| `getOffset(DateTimeInterface $datetime): int` | UTC offset (seconds) of this zone at the given instant — positive east of UTC, negative west — resolved daylight-saving aware from the system timezone database. |
| `static listIdentifiers(int $timezoneGroup = DateTimeZone::ALL, ?string $countryCode = null): array` | IANA timezone identifiers as an indexed array. With no arguments returns every zone in `ALL`. `$timezoneGroup` filters by region-group bitmask (e.g. `DateTimeZone::EUROPE`, or several OR'd together); `ALL_WITH_BC` additionally includes the backward-compat zones. With `DateTimeZone::PER_COUNTRY` set, results are filtered to `$countryCode` (an uppercase ISO 3166-1 code, matched case-sensitively); a missing country code then throws `ValueError`, as in PHP. |

`DateTimeZone` defines the region/group constants used as `listIdentifiers()` selectors, with PHP's bitmask values: `AFRICA` (1), `AMERICA` (2), `ANTARCTICA` (4), `ARCTIC` (8), `ASIA` (16), `ATLANTIC` (32), `AUSTRALIA` (64), `EUROPE` (128), `INDIAN` (256), `PACIFIC` (512), `UTC` (1024), `ALL` (2047), `ALL_WITH_BC` (4095), and `PER_COUNTRY` (4096).

```php
$europe  = DateTimeZone::listIdentifiers(DateTimeZone::EUROPE);            // 58 zones
$euAsia  = DateTimeZone::listIdentifiers(DateTimeZone::EUROPE | DateTimeZone::ASIA);
$withBc  = DateTimeZone::listIdentifiers(DateTimeZone::ALL_WITH_BC);       // incl. US/Eastern, GMT…
$france  = DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY, "FR"); // ["Europe/Paris"]
$all     = timezone_identifiers_list();                                    // alias, same filtering
```
| `getLocation(): array\|false` | The zone's `country_code`, `latitude`, `longitude`, and `comments`, or `false` for zones without a location (e.g. the legacy `CET`/`GMT` abbreviation-zones). |
| `getTransitions(int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX): array\|false` | The zone's daylight-saving transitions (each row has `ts`, `time`, `offset`, `isdst`, `abbr`). With no arguments returns the full list; with a window returns the state active at `$timestampBegin` followed by the transitions inside it. `false` for zones with no transitions. |
| `static listAbbreviations(): array` | The full abbreviation → `[dst, offset, timezone_id]` table, keyed by lowercase abbreviation. |

```php
$tz = new DateTimeZone("Europe/Paris");
echo $tz->getName();                                // Europe/Paris
echo $tz->getOffset(new DateTime("@1719835200"));   // 7200  (CEST, +02:00, summer)

$loc = $tz->getLocation();
echo $loc["country_code"];                          // FR

$t = $tz->getTransitions();
echo count($t), " ", $t[0]["abbr"];                 // 185 LMT  (row 0 is at PHP_INT_MIN)

$abbr = DateTimeZone::listAbbreviations();
echo $abbr["cest"][0]["timezone_id"];               // Europe/Berlin
```

The identifier is resolved against the system timezone database, so `getOffset()` is DST-correct and `DateTime::format()` honors the zone (see [`setTimezone()`](#datetime--datetimeimmutable) and [Limitations](#limitations)).

The `getLocation()`, `getTransitions()`, and `listAbbreviations()` tables are baked from PHP's own timelib data (so they are byte-for-byte identical to PHP) and ship in a small bridge that is linked **only** into programs that call one of these three methods (or their `timezone_location_get()` / `timezone_transitions_get()` / `timezone_abbreviations_list()` procedural aliases).

## DateTime and DateTimeImmutable

Both implement `DateTimeInterface`. `DateTime` mutates the object in place and returns `$this`; `DateTimeImmutable` leaves the receiver untouched and returns a **new** instance from every modifier — so its methods can be chained safely.

| Method | Description |
|---|---|
| `__construct(string $datetime = "now", ?DateTimeZone $timezone = null)` | Build from `"now"` or a date/time string such as `"2024-01-15 10:30:45"`. With a `$timezone`, the wall-clock string is interpreted in that zone (unless the string carries its own), and that zone becomes the display zone. |
| `getTimestamp(): int` | UNIX timestamp. |
| `setTimestamp(int $ts): static` | Set the moment from a UNIX timestamp. |
| `getMicrosecond(): int` | Sub-second component (0–999999). |
| `setMicrosecond(int $us): static` | Set the sub-second component; surfaced by the `u`/`v` format specifiers. |
| `setDate(int $y, int $m, int $d): static` | Replace the calendar date, keeping the time of day. |
| `setISODate(int $y, int $week, int $dayOfWeek = 1): static` | Set the date from an ISO 8601 week date (day 1 = Monday), keeping the time of day. |
| `setTime(int $h, int $i, int $s = 0): static` | Replace the time of day, keeping the date. |
| `getTimezone(): DateTimeZone` | The associated `DateTimeZone`. |
| `setTimezone(DateTimeZone $tz): static` | Change the display zone (the absolute instant is unchanged). |
| `getOffset(): int` | UTC offset (seconds) of the object's own zone at its instant, daylight-saving aware. |
| `format(string $format): string` | Format via the `date()` specifiers. |
| `add(DateInterval $interval): static` | Add a calendar interval. |
| `sub(DateInterval $interval): static` | Subtract a calendar interval. |
| `modify(string $modifier): static` | Apply a relative modifier (e.g. `"+1 day"`) parsed by `strtotime()`. |
| `diff(DateTimeInterface $target): DateInterval` | Difference between two moments. |
| `static createFromFormat(string $format, string $datetime, ?DateTimeZone $timezone = null): static\|false` | Parse a string per a `date()`-style format, returning a new instance or `false` on mismatch. With a `$timezone`, the wall-clock is interpreted in that zone and it becomes the display zone. |
| `static createFromTimestamp(int\|float $timestamp): static` | Build a new instance set to a UNIX timestamp (the fractional part is dropped — second resolution). |
| `static createFromInterface(DateTimeInterface $object): static` | Copy any date object's instant and timezone into this class (mutable ↔ immutable). |
| `static getLastErrors(): array` | `['warning_count', 'warnings', 'error_count', 'errors']` for the last `createFromFormat()` on this class — `error_count` is 1 after a format mismatch, 0 after success. |
| `DateTime::createFromImmutable(DateTimeImmutable $object): DateTime` | Mutable copy of an immutable date. |
| `DateTimeImmutable::createFromMutable(DateTime $object): DateTimeImmutable` | Immutable copy of a mutable date. |

```php
$dt = new DateTime();
$dt->setDate(2024, 1, 15);
$dt->setTime(9, 30, 0);
echo $dt->format("Y-m-d H:i:s");      // 2024-01-15 09:30:00

$dt->add(new DateInterval("P1Y2M10D"));
echo $dt->format("Y-m-d");            // 2025-03-25

// DateTimeImmutable returns a fresh instance; the original is unchanged.
$base  = (new DateTimeImmutable())->setDate(2024, 1, 15)->setTime(0, 0, 0);
$later = $base->add(new DateInterval("PT2H30M"));
echo $base->format("H:i:s");          // 00:00:00
echo $later->format("H:i:s");         // 02:30:00
```

`add()` and `sub()` are calendar operations: they decompose the date, apply the interval component by component, and recompose with `mktime()`, so month/day overflow normalizes exactly as in PHP (for example, `2020-01-31` plus one month becomes `2020-03-02`). An interval whose `invert` flag is `1` is applied in the opposite direction.

`modify()` re-parses a string modifier against the object's current time, equivalent to `setTimestamp(strtotime($modifier, $this->getTimestamp()))`. It accepts whatever [`strtotime()`](system-and-io.md) supports — relative offsets (`"+1 day"`, `"-2 weeks"`, `"+1 month"`), time-only resets (`"23:45"`), bare keywords, and `"first/last day of …"` / `"first/last <weekday> of …"` phrases:

```php
$dt = new DateTime();
$dt->setDate(2024, 1, 15);
$dt->setTime(10, 0, 0);
$dt->modify("+1 day");      // 2024-01-16 10:00:00
$dt->modify("-2 weeks");    // 2024-01-02 10:00:00
$dt->modify("23:45");       // 2024-01-02 23:45:00
$dt->modify("first day of next month");   // 2024-02-01 10:00:00
$dt->modify("last day of this month");    // 2024-01-31 10:00:00
$dt->modify("first monday of next month");// first Monday of February 2024
```

`createFromFormat()` is the inverse of `format()`: it parses a string according to a `date()`-style format and returns a new instance, or `false` when the subject does not match. Unspecified fields default to the current date and time; a leading `!` resets every field to the Unix epoch, and `|` resets the fields not parsed up to that point.

```php
$dt = DateTime::createFromFormat("d/m/Y H:i", "15/03/2024 14:30");
echo $dt->format("Y-m-d H:i:s");        // 2024-03-15 14:30:00

// '!' zeroes the unspecified time fields:
$day = DateTime::createFromFormat("!Y-m-d", "2024-03-15");
echo $day->format("H:i:s");             // 00:00:00

$bad = DateTime::createFromFormat("Y-m-d", "not a date");
var_dump($bad === false);               // bool(true)
```

The supported format characters are `Y y m n d j D l S F M z H G h g i s u v A a U O P Z T e X x`, plus the metas `! | # ? * + \`. In detail:

- `D` / `l` parse a weekday name (abbreviated or full, case-insensitive) and shift the result forward 0–6 days to land on that weekday, matching PHP/timelib's relative-weekday behavior.
- `F` / `M` parse a month name (full, abbreviated, or `sept`, case-insensitive).
- `S` consumes an English ordinal suffix (`st`, `nd`, `rd`, `th`).
- `z` is the 0-based day of the year: it requires an already-parsed year, overrides the month/day, and overflows into following years through `mktime` normalization.
- `v` parses up to three milliseconds digits; `u` parses up to six microseconds digits.
- `#` matches one separator from `;:/.,-`, `?` skips one subject byte, `*` skips bytes until the next digit or separator, `+` tolerates trailing subject data, `\` escapes the next format character, and `!` / `|` reset fields.
- Any other character must match the subject literally, and (without a trailing `+`) leftover subject data after the format is exhausted is a parse failure, matching PHP.

The timezone specifiers `O` (`±hhmm`), `P` (`±hh:mm`), `Z` (UTC offset in seconds, signed or unsigned, no leading `+` for positive), `T` (greedy alpha abbreviation like `CEST`/`EDT`/`UTC`), and `e` (IANA zone name) consume the corresponding substring from the subject and cross-validate it against the constructed instant's zone (a mismatch returns `false`, matching PHP). The optional third `DateTimeZone` argument is accepted: the parsed wall-clock is interpreted in that zone and it becomes the display zone.

### Format constants

The standard `DateTimeInterface` format constants are available on the interface and on both classes, with PHP-identical values:

```php
echo DateTime::ATOM;                         // Y-m-d\TH:i:sP
$d = new DateTime("2024-07-01 14:30:00", new DateTimeZone("Europe/Paris"));
echo $d->format(DateTime::ATOM);             // 2024-07-01T14:30:00+02:00
echo $d->format(DateTimeInterface::RFC2822); // Mon, 01 Jul 2024 14:30:00 +0200
```

Available: `ATOM`, `COOKIE`, `ISO8601`, `ISO8601_EXPANDED`, `RFC822`, `RFC850`, `RFC1036`, `RFC1123`, `RFC7231`, `RFC2822`, `RFC3339`, `RFC3339_EXTENDED`, `RSS`, `W3C`. `ISO8601_EXPANDED` uses the expanded-year `X`/`x` format specifiers, which `format()` renders like `date()`/`gmdate()` (see the specifier table in [System and I/O](system-and-io.md)).

## DateInterval

Represents a span of time. The constructor parses an ISO 8601 duration string `P[nY][nM][nW][nD][T[nH][nM][nS]]`; a `W` (week) component contributes seven days each.

| Property | Meaning |
|---|---|
| `y`, `m`, `d` | Calendar years, months, days. |
| `h`, `i`, `s` | Hours, minutes, seconds. |
| `f` | Fraction of a second (float). Set by `diff()`, applied by `add()`/`sub()`, and rendered by `format()`'s `%f`/`%F`. |
| `invert` | `1` when the interval is negative, else `0`. |
| `days` | Total whole days (set by `diff()`). |

```php
$iv = new DateInterval("P1Y2M3DT4H5M6S");
echo $iv->y, " ", $iv->m, " ", $iv->d;   // 1 2 3
echo $iv->h, " ", $iv->i, " ", $iv->s;   // 4 5 6

$weeks = new DateInterval("P2W");
echo $weeks->d;                          // 14
```

`DateInterval::format(string $format)` renders an interval using PHP's `%` specifiers:

| Specifier | Output |
|---|---|
| `%y` `%m` `%d` `%h` `%i` `%s` | years / months / days / hours / minutes / seconds, no padding |
| `%Y` `%M` `%D` `%H` `%I` `%S` | the same, zero-padded to at least two digits |
| `%a` | total days (from `diff()`), or `(unknown)` for a directly built interval |
| `%f` / `%F` | whole microseconds from `$f`, unpadded / zero-padded to six digits |
| `%R` / `%r` | sign: `-`/`+` and `-`/empty |
| `%%` | a literal `%` |

```php
$iv = new DateInterval("P1Y2M3DT4H5M6S");
echo $iv->format("%y years, %m months, %d days");  // 1 years, 2 months, 3 days
echo $iv->format("%H:%I:%S");                       // 04:05:06

$d = (new DateTime("2020-01-01"))->diff(new DateTime("2021-03-15"));
echo $d->format("%a days");                         // 439 days
```

An unrecognized specifier is copied through unchanged.

`DateInterval::createFromDateString(string $datetime)` builds an interval from a relative date string instead of an ISO duration. Counts are stored verbatim — they are **not** normalized (so `"90 seconds"` yields `s = 90`) — weeks fold into days (×7), `fortnight` adds 14 days, and a negative count is stored in the component (`"-1 day"` yields `d = -1`, `invert = 0`). The keywords `tomorrow` and `yesterday` map to ±1 day:

```php
$a = DateInterval::createFromDateString("2 weeks 3 days");  // d = 17
$b = DateInterval::createFromDateString("1 year 2 months"); // y = 1, m = 2
$d = new DateTime("2024-01-31");
$d->add(DateInterval::createFromDateString("1 month"));     // 2024-03-02 (calendar overflow)
```

The procedural alias `date_interval_create_from_date_string($datetime)` is equivalent. Unknown words are ignored rather than raising an error.

## Differences with `diff()`

`DateTimeInterface::diff()` returns a `DateInterval` carrying both the **calendar breakdown** (`y`/`m`/`d`) and the **total** elapsed days (`days`), plus the `h`/`i`/`s` remainder and the `invert` direction flag:

```php
$a = new DateTime(); $a->setDate(2020, 1, 1); $a->setTime(0, 0, 0);
$b = new DateTime(); $b->setDate(2021, 3, 15); $b->setTime(0, 0, 0);
$d = $a->diff($b);
echo $d->y, "y ", $d->m, "m ", $d->d, "d";   // 1y 2m 14d
echo " (", $d->days, " days)";               // (439 days)
```

The calendar components are counted by advancing whole years, then months, then days, so edge cases match PHP — for instance `2020-01-31` to `2020-03-01` is `0y 0m 30d` (not a partial month).

The `days` property is `int|false`: an interval produced by `diff()` carries the real total-day count, while an interval constructed directly (`new DateInterval("P2W")`) has `days === false` (so it echoes as empty and `format("%a")` renders `(unknown)`), exactly as in PHP:

```php
$w = new DateInterval("P2W");
var_dump($w->days);              // bool(false)
echo $w->format("%a");           // (unknown)
echo $a->diff($b)->days;         // 439
```

## Comparing DateTime values

`DateTime` and `DateTimeImmutable` values can be compared with `==`, `!=`, `<`, `<=`, `>`, `>=`, and the spaceship `<=>` operator. As in PHP, the comparison is by the absolute instant — the timestamp plus the microsecond component — so it is independent of the stored timezone and works across the two classes:

```php
$a = new DateTime("2024-06-15 12:00:00", new DateTimeZone("UTC"));
$b = new DateTime("2024-06-15 08:00:00", new DateTimeZone("America/New_York"));
var_dump($a == $b);              // bool(true)  — same instant, different zones
var_dump($a < new DateTime("2024-06-15 12:00:01")); // bool(true)
echo (new DateTime("2024-01-01")) <=> (new DateTime("2024-06-01")); // -1
```

Identity operators `===`/`!==` are unchanged: they compare object references, so two distinct instances are never identical even when they represent the same instant.

## DatePeriod

`DatePeriod` implements `Iterator`, so it can be walked with `foreach` to visit each step of a date range. Construct it from a start date, a `DateInterval` step, and an end date:

```php
$period = new DatePeriod(
    new DateTime("2024-01-01"),
    new DateInterval("P1M"),
    new DateTime("2024-04-01")
);
foreach ($period as $i => $dt) {
    echo $i, ": ", $dt->format("Y-m-d"), "\n";
}
// 0: 2024-01-01
// 1: 2024-02-01
// 2: 2024-03-01
```

The end date is **exclusive** by default. Two option flags adjust the bounds:

| Constant | Effect |
|---|---|
| `DatePeriod::EXCLUDE_START_DATE` | Skip the start date (begin one interval later). |
| `DatePeriod::INCLUDE_END_DATE` | Include the end date when a step lands exactly on it. |

```php
$p = new DatePeriod($start, $interval, $end, DatePeriod::EXCLUDE_START_DATE);
```

Alternatively, pass an **integer recurrence count** as the third argument instead of an end date. The period then yields the start plus that many further steps (so `recurrences + 1` dates), or exactly `recurrences` dates when `EXCLUDE_START_DATE` is set:

```php
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), 3);
foreach ($p as $dt) {
    echo $dt->format("m-d"), " ";
}
// 01-01 01-02 01-03 01-04
echo $p->getRecurrences(); // 3
```

Each iteration yields a fresh `DateTime`, so collecting the values keeps every step distinct. Steps advance with the same calendar arithmetic as `DateTime::add()`, so month/day overflow normalizes like PHP. `getStartDate()`, `getEndDate()`, and `getDateInterval()` return the values the period was built from.

| Method | Description |
|---|---|
| `__construct(DateTimeInterface $start, DateInterval $interval, DateTimeInterface\|int $end, int $options = 0)` | Build a period either over `[start, end)` or for an integer recurrence count. |
| `getStartDate(): DateTime` | The start date. |
| `getEndDate(): DateTime` | The end date. |
| `getDateInterval(): DateInterval` | The step interval. |
| `getRecurrences(): ?int` | The recurrence count for the count form, or `null` for the end-date form. |
| `getIterator(): Iterator` | An iterator over the period's dates, for `foreach ($p->getIterator() ...)` / `iterator_to_array($p->getIterator())`. (PHP exposes `DatePeriod` as an `IteratorAggregate`; elephc's `DatePeriod` is itself an `Iterator`, so `getIterator()` returns the rewound period — a single live iterator rather than an independent copy.) |
| `createFromISO8601String(string $isostr): static` | Build a period from an RFC 5545 repeating-interval specification (`R<n>/start[/interval[/end]]`). Forwards to the regular constructor; throws `DateMalformedPeriodStringException` on a malformed specification (PHP 8.3+) — a recurrence-count message for `R0/...` and `Unknown or bad format (...)` for `R/...`, `R-1/...`, or anything not matching `R<digits>/`. The deprecated `new DatePeriod(string)` constructor is not registered; use this static factory instead. |

Both the `(start, interval, end)` and `(start, interval, recurrences)` constructor forms are supported, plus `createFromISO8601String()` for the RFC 5545 string form.

## Exceptions

The PHP 8.3 date/time exception hierarchy is available and integrates with the standard `Error`/`Exception` types, so a thrown subclass is catchable at every ancestor level:

```php
try {
    throw new DateMalformedStringException("bad input");
} catch (DateException $e) {        // also catchable as Exception
    echo $e->getMessage();
}
```

| Class | Extends |
|---|---|
| `DateError` | `Error` |
| `DateObjectError`, `DateRangeError` | `DateError` |
| `DateException` | `Exception` |
| `DateInvalidTimeZoneException`, `DateInvalidOperationException` | `DateException` |
| `DateMalformedStringException`, `DateMalformedIntervalStringException`, `DateMalformedPeriodStringException` | `DateException` |

## Procedural API

The procedural date/time functions are supported as aliases for the OOP API:

| Procedural | Equivalent |
|---|---|
| `date_create($s)` / `date_create_immutable($s)` | `new DateTime($s)` / `new DateTimeImmutable($s)` |
| `date_create_from_format($f, $s [, $tz])` / `date_create_immutable_from_format($f, $s [, $tz])` | `DateTime::createFromFormat(…)` / `DateTimeImmutable::createFromFormat(…)` |
| `date_format($d, $f)`, `date_diff($a, $b)`, `date_modify($d, $m)` | `$d->format($f)`, `$a->diff($b)`, `$d->modify($m)` |
| `date_add($d, $i)`, `date_sub($d, $i)` | `$d->add($i)`, `$d->sub($i)` |
| `date_date_set($d, $y, $m, $day)`, `date_time_set($d, $h, $m, $s)` | `$d->setDate(…)`, `$d->setTime(…)` |
| `date_timestamp_get/set`, `date_timezone_get/set`, `date_offset_get` | the matching `getTimestamp`/`setTimestamp`/`getTimezone`/`setTimezone`/`getOffset` |
| `timezone_open($id)`, `timezone_name_get($tz)`, `timezone_offset_get($tz, $d)` | `new DateTimeZone($id)`, `$tz->getName()`, `$tz->getOffset($d)` |
| `timezone_identifiers_list()` | `DateTimeZone::listIdentifiers()` |
| `timezone_name_from_abbr($abbr)` | the IANA zone for a common abbreviation, or `false` |
| `timezone_location_get($tz)`, `timezone_transitions_get($tz [, $begin, $end])`, `timezone_abbreviations_list()` | `$tz->getLocation()`, `$tz->getTransitions(…)`, `DateTimeZone::listAbbreviations()` |
| `timezone_version_get()` | `"2026.1"` (the IANA release the bundled introspection data was baked from) |
| `date_interval_format($i, $f)` | `$i->format($f)` |

`timezone_name_from_abbr()` recognizes the common abbreviations (`UTC`, `GMT`, `EST`/`EDT`, `CET`/`CEST`, `JST`, `MSK`, `AEST`, …) and returns the same IANA zone PHP does; the optional `$utcOffset`/`$isDST` arguments are accepted but offset-based disambiguation is not performed. Abbreviations outside the common set return `false`.

### Parsing to components

`date_parse_from_format(string $format, string $datetime)` and `date_parse(string $datetime)` return PHP's parse-result array — the `year`, `month`, `day`, `hour`, `minute`, `second`, and `fraction` components (each an integer when present, or `false` when the input did not specify it), plus `warning_count`, `warnings`, `error_count`, `errors`, and `is_localtime`:

```php
$r = date_parse_from_format("Y-m-d H:i:s", "2024-03-15 14:30:45");
echo $r["year"], "-", $r["month"], "-", $r["day"];   // 2024-3-15

$d = date_parse("2024-03-15");
var_dump($d["hour"]);                                 // bool(false) — not specified
```

`date_parse_from_format` uses the same format characters as [`createFromFormat()`](#datetime-and-datetimeimmutable). `date_parse` does not implement PHP's full free-form grammar — it recognizes the common formats (`Y-m-d H:i:s`, `Y-m-d`, `H:i`, and similar) by trying them in turn.

### Sunrise, sunset, and twilight

`date_sun_info(int $timestamp, float $latitude, float $longitude): array` returns the nine-key solar array for the day — `sunrise`, `sunset`, `transit`, and the `civil_`/`nautical_`/`astronomical_twilight_begin`/`_end` bounds. Each rise/set value is a Unix timestamp, or `true`/`false` when the sun stays above/below that altitude for the whole day:

```php
$i = date_sun_info(mktime(0, 0, 0, 6, 21, 2024), 48.8566, 2.3522);  // Paris, summer solstice
echo gmdate("H:i", $i["sunrise"]);                  // 03:47
var_dump($i["astronomical_twilight_begin"]);        // bool(true) — never fully dark
```

The deprecated `date_sunrise()` / `date_sunset()` are also available:

```php
date_sunrise($ts, SUNFUNCS_RET_STRING, $lat, $lon, $zenith, $utcOffset);  // "05:45"
date_sunrise($ts, SUNFUNCS_RET_TIMESTAMP, $lat, $lon);                    // Unix timestamp
date_sunset($ts, SUNFUNCS_RET_DOUBLE, $lat, $lon);                        // hour-of-day float
```

`$returnFormat` is one of `SUNFUNCS_RET_TIMESTAMP` (0), `SUNFUNCS_RET_STRING` (1, the default), or `SUNFUNCS_RET_DOUBLE` (2); omitted latitude/longitude/zenith fall back to PHP's defaults. Both functions return `false` when the sun does not reach the requested altitude that day. The implementation is a faithful port of PHP's timelib astronomical algorithm, so `date_sun_info()` and the `RET_TIMESTAMP`/`RET_STRING` formats match PHP exactly; `RET_DOUBLE` may differ only in the last floating-point digit.

## Limitations

- `format()` renders the stored instant in the object's **own timezone**: the zone captured from `date_default_timezone_get()` at construction, or one assigned later with `setTimezone()`. Offsets and daylight-saving transitions are resolved from the system timezone database (see [System & I/O](system-and-io.md)). `setTimezone()` changes only the display zone, not the absolute instant. (`gmdate()` remains available for explicit UTC formatting.)
- The constructor accepts `"now"`, absolute date/time strings, and the `@<timestamp>` epoch form — anything [`strtotime()`](system-and-io.md) parses. For relative expressions like `"+1 day"`, you can also build the object and call `modify()`.
- `modify()` and the 2-argument `strtotime()` support relative offsets, time-only, and keyword forms, plus the `"first/last day of"` and `"first/last <weekday> of"` phrases (e.g. `"first monday of next month"`, `"last day of this month"`). `DatePeriod` supports the `(start, interval, end)` and `(start, interval, recurrences)` forms, plus the RFC 5545 string form via `DatePeriod::createFromISO8601String()` (the deprecated `new DatePeriod("R4/...")` constructor is not registered).
- **Microseconds**: `getMicrosecond()`/`setMicrosecond()` store a sub-second component, and `format()`'s `u` (six digits) and `v` (three digits) specifiers render it. `createFromFormat()` parses the `u` specifier (e.g. `"Y-m-d H:i:s.u"`, `"U.u"`). The component is preserved across mutable and immutable operations and participates in arithmetic: `diff()` reports the fractional-second difference in `DateInterval::$f` (with a one-second borrow and a microsecond-aware ordering, so `00.750 → 01.250` is `s=0, f=0.5`), and `add()`/`sub()` apply an interval's `$f` with carry across the second. The constructor captures a trailing fractional second from a date string (`new DateTime("2020-01-01 12:00:00.5")` → microsecond `500000`, padded/truncated to six digits); a `.` that is not a fractional second (e.g. a `DD.MM.YYYY` separator) is left untouched. `modify()` also accepts a `microsecond[s]`/`usec[s]` relative unit (e.g. `modify("+500000 microseconds")`, alone or combined with other clauses), carrying into the whole second. So the sub-second component can be set via the constructor, `createFromFormat()`, or `setMicrosecond()`, and participates in `add()`/`sub()`/`diff()`/`modify()`; everything else stays at libc second resolution.
- `createFromFormat()` supports the format characters `Y y m n d j D l S F M z H G h g i s u v A a U O P Z T e X x`, `\` escapes, and the `!`/`|` resets. The timezone specifiers `O` (`±hhmm`), `P` (`±hh:mm`), `Z` (UTC offset in seconds, signed or unsigned, no leading `+` for positive), `T` (greedy alpha abbreviation like `CEST`/`EDT`/`UTC`), and `e` (IANA zone name) consume the corresponding substring from the subject and cross-validate it against the constructed instant's zone (a mismatch returns `false`, matching PHP). Its optional third `DateTimeZone` argument is accepted: the parsed wall-clock is interpreted in that zone and it becomes the display zone. The third argument can be passed inline (`new DateTimeZone(...)`) or via a variable.
- Dates before 1900: `mktime()`, `gmmktime()`, `strtotime()`, and the `DateTime`/`DateTimeImmutable` constructors handle years 101–1899 (libc rejects them, so elephc shifts the year forward by whole 400-year Gregorian cycles into libc's range and corrects the result).
- Two-digit years: `mktime()` and `gmmktime()` apply PHP's two-digit-year shorthand to their `year` argument — years 0–69 map to 2000–2069 and years 70–100 map to 1970–2000, while years ≥ 101 are taken literally (so `mktime(0, 0, 0, 1, 1, 99)` is 1999 and `mktime(0, 0, 0, 1, 1, 101)` is year 101). The same shorthand is applied to the year field of a `M/D/YY` (or `MM/DD/YY`) date string parsed by `strtotime()`/`DateTime` — so `strtotime("1/1/99")` is 1999 and `strtotime("1/1/50")` is 2050. The shorthand is **not** applied to 2-digit years written in the ISO `YY-MM-DD` form — those are rejected as malformed (PHP accepts them and remaps, so `strtotime("99-01-01")` in PHP is 1999-01-01; the ISO entry in `__rt_strtotime_iso_entry` requires a 4-digit year).

### Not currently supported

A few corners of PHP's date/time API are not implemented:

- **Detailed parse warnings**: `getLastErrors()`/`date_get_last_errors()` report whether the last `createFromFormat()` succeeded or failed (`error_count` 0/1) but do not retain PHP's per-character warning/error positions — only the pass/fail count, which covers the common `if (DateTime::getLastErrors()['error_count'])` guard.
- **Serialization hooks**: the `__serialize()`/`__unserialize()`/`__wakeup()`/`__set_state()` magic methods are not defined on the date classes, because elephc has no object `serialize()`/`unserialize()`/`var_export()` round-trip for any class.

A complete runnable program is in [`examples/datetime/main.php`](https://github.com/illegalstudio/elephc/tree/main/examples/datetime).
