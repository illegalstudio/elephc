---
title: "System & I/O"
description: "System functions, date/time, JSON, filesystem utilities, process execution, and debugging utilities."
sidebar:
  order: 13
---

## System functions

| Function | Signature | Description |
|---|---|---|
| `exit()` | `exit($code = 0): void` | Terminate program |
| `die()` | `die($code = 0): void` | Alias for `exit()` |
| `time()` | `time(): int` | Unix timestamp |
| `microtime()` | `microtime($as_float = false): string\|float` | Current time with microsecond precision. Returns the `"0.NNNNNNNN SSSSSSSSSS"` string (fractional microseconds as 8 digits, a space, then Unix seconds) by default or when `$as_float` is `false`; returns seconds as a `float` when `$as_float` is `true`. A non-literal flag yields `string\|float` (boxed `Mixed`), resolved at runtime. |
| `hrtime()` | `hrtime($as_number = false): array\|int` | High-resolution monotonic time (`CLOCK_MONOTONIC`). Returns `[seconds, nanoseconds]`, or the total nanoseconds as an int when `$as_number` is true — for benchmarking elapsed time. |
| `sleep()` | `sleep($seconds): int` | Sleep for seconds |
| `usleep()` | `usleep($microseconds): void` | Sleep for microseconds |
| `getenv()` | `getenv($name): string` | Get environment variable |
| `putenv()` | `putenv($assignment): bool` | Set environment variable ("KEY=VALUE") |
| `define()` | `define($name, $value): bool` | Define a compile-time global constant with a string-literal name |
| `defined()` | `defined($name): bool` | Check whether a string-literal constant name is defined |
| `php_uname()` | `php_uname($mode = "a"): string` | Get system information from the target runtime |
| `phpversion()` | `phpversion(): string` | Get the elephc package version from `Cargo.toml` |
| `exec()` | `exec($command): string` | Execute command, return output |
| `shell_exec()` | `shell_exec($command): string` | Execute via shell, return output |
| `system()` | `system($command): string` | Execute, output to stdout |
| `passthru()` | `passthru($command): void` | Execute, pass raw output |

`define()` returns `true` the first time a constant is defined at runtime. Duplicate attempts keep the first value, return `false`, and emit a suppressible runtime warning. `defined()` currently requires a string literal in AOT mode.

`php_uname()` supports PHP's standard one-character modes:

| Mode | Result |
|---|---|
| `"a"` | Full system line: system name, node name, release, version, machine |
| `"s"` | System name, matching `PHP_OS` (`"Darwin"` on macOS targets, `"Linux"` on Linux targets) |
| `"n"` | Network node name |
| `"r"` | Release |
| `"v"` | Version |
| `"m"` | Machine hardware name |

## Date and time

| Function | Signature | Description |
|---|---|---|
| `date()` | `date($format [, $timestamp]): string` | Format a timestamp (or the current time) in the default timezone using the specifiers below. |
| `gmdate()` | `gmdate($format [, $timestamp]): string` | Same as `date()` but formats in UTC, so the result is independent of the configured timezone. |
| `idate()` | `idate($format [, $timestamp]): int` | Like `date()` for a single integer-valued specifier, returning an `int` instead of a string (e.g. `idate("Y")`, `idate("U")`). Equivalent to `(int) date($format, $timestamp)`. |
| `date_default_timezone_set()` | `date_default_timezone_set($timezoneId): bool` | Set the default timezone used by `date()` (and `DateTime::format()`). Accepts any IANA identifier (e.g. `"Europe/Paris"`, `"UTC"`); the UTC offset and daylight-saving transitions are resolved from the system timezone database. Returns `true`. |
| `date_default_timezone_get()` | `date_default_timezone_get(): string` | Return the current default timezone identifier (defaults to `"UTC"` when none has been set). |
| `mktime()` | `mktime($h, $m, $s, $mon, $day, $yr): int` | Create a timestamp from components, interpreted in the default timezone. |
| `gmmktime()` | `gmmktime($h, $m, $s, $mon, $day, $yr): int` | Like `mktime()` but interprets the components as UTC (independent of the default timezone). |
| `checkdate()` | `checkdate($month, $day, $year): bool` | Validate a Gregorian date — leap-year aware; month 1–12, day within the month, year 1–32767. |
| `getdate()` | `getdate($timestamp = time()): array` | Decompose a timestamp into an associative array: `seconds`, `minutes`, `hours`, `mday`, `wday`, `mon`, `year`, `yday`, `weekday`, `month`, and `0` (the timestamp). |
| `localtime()` | `localtime($timestamp = time(), $associative = false): array` | Decompose a timestamp into the raw `struct tm` fields — numeric-indexed `0`–`8` by default, or `tm_sec`…`tm_isdst` keys when `$associative` is true (`tm_mon` is 0-based, `tm_year` is years since 1900). |
| `gettimeofday()` | `gettimeofday($as_float = false): array\|float` | Current time as `['sec' => int, 'usec' => int, 'minuteswest' => int, 'dsttime' => int]` (minuteswest/dsttime from the default zone), or a float of seconds when `$as_float` is true. `usec` is derived from `microtime()`. |
| `strftime()` / `gmstrftime()` | `strftime($format [, $timestamp]): string` | Format a timestamp with C `strftime` `%`-specifiers (deprecated in PHP 8.1). Specifiers match PHP exactly, including the week numbers `%U` (Sunday-based), `%W` (Monday-based) and `%V` (ISO), the space-padded `%e`/`%k`/`%l`, `%c` (with its space-padded day), and the two-digit ISO year `%g`. `%x`/`%X` use the default C-locale forms (`%m/%d/%y`, `%H:%M:%S`). One known difference: `%P` yields lowercase `am`/`pm` here, whereas PHP's `strftime` emits a literal `P` (it never implemented the GNU lowercase extension). `gmstrftime()` formats in UTC. |
| `strptime()` | `strptime($timestamp, $format): array\|false` | Inverse of `strftime()`: parse a string against C `strftime` `%`-specifiers (`%Y %y %m %d %e %H %M %S %j %B %b %h %A %a %p %P`, the week specifiers `%u %w %U %W %V` and the timezone specifiers `%z`/`%Z` (consumed but not used to build the instant), plus `%n`/`%t` and `%%`) into a `struct tm` array (`tm_sec`/`tm_min`/`tm_hour`/`tm_mday`/`tm_mon` 0-based/`tm_year` since 1900/`tm_wday`/`tm_yday`/`unparsed`), or `false` on mismatch. Deprecated in PHP 8.1. |
| `strtotime()` | `strtotime($datetime [, $baseTimestamp]): int\|false` | Parse a date/time string into a Unix timestamp. Supports ISO dates, time-only, relative offsets, named weekdays, and bare keywords. When `$baseTimestamp` is given, relative/keyword/time-only forms resolve against it instead of the current time. An ISO field outside its range (month > 12, day > 31, hour > 24, minute > 59, second > 60) returns `false`, matching PHP; in-range calendar overflow such as `"2026-02-30"` is still normalized. Returns `false` on failure (use `=== false`; `-1` is the valid timestamp one second before the epoch). |

When `date_default_timezone_set()` has not been called, the default timezone is **UTC** (matching PHP), so `date()`, `strtotime()`, `mktime()`, and `DateTime` produce host-independent output rather than following the build machine's local zone.

Procedural date/time aliases (`date_create`, `date_create_immutable`, `date_create_from_format`, `date_create_immutable_from_format`, `date_diff`, `date_format`, `date_add`, `date_sub`, `date_modify`, `date_timestamp_get`, `date_timestamp_set`, `date_timezone_get`, `date_timezone_set`, `date_offset_get`, `date_date_set`, `date_isodate_set`, `date_time_set`, `date_interval_format`, `date_interval_create_from_date_string`, `date_parse`, `date_parse_from_format`, `date_get_last_errors`, `date_sun_info`, `date_sunrise`, `date_sunset`, `strptime`, `idate`, `gettimeofday`, `strftime`, `gmstrftime`, `timezone_open`, `timezone_identifiers_list`, `timezone_name_get`, `timezone_offset_get`, `timezone_name_from_abbr`, `timezone_location_get`, `timezone_transitions_get`, `timezone_abbreviations_list`, `timezone_version_get`) are recognized by `function_exists()` even though the name resolver rewrites them into OOP calls or built-in expressions; comparison is case-insensitive on the last namespace segment.

`date()` and `gmdate()` support these format specifiers (any other character is copied through verbatim):

| Specifier | Output |
|---|---|
| `Y` / `y` | 4-digit year / 2-digit year |
| `m` / `n` | month 01–12 / 1–12 |
| `d` / `j` | day of month 01–31 / 1–31 |
| `D` / `l` | short weekday name (`Mon`) / full weekday name (`Monday`) |
| `N` / `w` | ISO weekday 1–7 (Mon=1) / numeric weekday 0–6 (Sun=0) |
| `F` / `M` | full month name (`January`) / short month name (`Jan`) |
| `H` / `G` | hour 00–23 / 0–23 |
| `h` / `g` | hour 01–12 / 1–12 |
| `i` / `s` | minutes 00–59 / seconds 00–59 |
| `A` / `a` | `AM`/`PM` / `am`/`pm` |
| `S` | English ordinal suffix for the day of month (`st`, `nd`, `rd`, `th`) |
| `z` | day of year, 0–365 |
| `W` / `o` | ISO-8601 week number (01–53) / ISO-8601 week-numbering year |
| `t` | number of days in the month, 28–31 |
| `L` | leap year flag, `1` or `0` |
| `U` | Unix timestamp |
| `O` / `P` | UTC offset `±hhmm` (`+0200`) / `±hh:mm` (`+02:00`) |
| `p` | like `P`, but the literal `Z` when the offset is zero |
| `Z` | UTC offset in seconds, `-43200`–`50400` (`7200`, `-18000`) |
| `B` | Swatch Internet Time, `000`–`999` beats of the UTC+1 day |
| `T` / `e` | timezone abbreviation (`CEST`) / identifier (`Europe/Paris`); `gmdate()` reports `UTC` |
| `I` | daylight-saving flag, `1` if DST is in effect at the instant, else `0` |
| `c` / `r` | ISO 8601 datetime (`2024-07-01T14:00:00+02:00`) / RFC 2822 datetime (`Mon, 01 Jul 2024 14:00:00 +0200`) |
| `u` / `v` | microseconds / milliseconds; always `000000` / `000` (whole-second timestamps) |
| `X` / `x` | expanded ISO-8601 year — `X` is always signed (`+2024`); `x` is signed only outside `[0, 9999]` (`1970`, `+10000`, `-0005`) |

A backslash escapes the next character so it is emitted literally instead of being treated as a specifier (e.g. `date('Y-m-d\TH:i:s')` → `2023-11-14T22:13:20`). Use single-quoted format strings so PHP's double-quoted escapes (`\t`, `\n`, …) don't interfere. (A lone trailing backslash emits nothing, rather than PHP's NUL byte.)

`strtotime()` accepts the following shapes (input is case-insensitive for keywords/unit names/weekday names, and leading/trailing ASCII whitespace is trimmed):

- **ISO date / datetime** — `YYYY-MM-DD`, `YYYY-MM-DD HH:MM`, `YYYY-MM-DD HH:MM:SS`, `YYYY-MM-DDTHH:MM`, or `YYYY-MM-DDTHH:MM:SS`. Lowercase `t` is also accepted as the date/time separator.
- **`@<timestamp>`** — a UNIX timestamp (e.g. `"@1700000000"`); an optional sign and a truncated fractional part are accepted. Returned verbatim (UTC), independent of the current time.
- **American `M/D/Y`** — `MM/DD/YYYY` or single-digit `M/D/Y`, with an optional `HH:MM[:SS]` time suffix (e.g. `"12/25/2024"`, `"6/15/2024 8:05"`). A 2-digit year windows to 2000–2069 (`0`–`69`) or 1970–1999 (`70`–`99`). Month must be `<= 12` and day `<= 31`.
- **Textual dates** — `D Month Y` (`"25 December 2024"`) or `Month D[,] Y` (`"December 25, 2024"`, `"July 4 2024"`), with full or 3-letter month names (case-insensitive) and an optional `HH:MM[:SS]` time suffix. The day is not range-checked, so `"31 feb 2024"` normalizes to March 2 as in PHP. A 2-digit year follows the same windowing as slash dates.
- **Bare keywords** — `now`, `today`, `tomorrow`, `yesterday`, `midnight`, `noon`. (`midnight` is an alias for `today`.)
- **Time-only** — `H:MM`, `HH:MM`, `H:MM:SS`, `HH:MM:SS` — combined with today's date.
- **Relative offsets** — `[+-]?N unit [N unit ...]`, `a/an unit`, and `N unit ago` / `a/an unit ago` (negates the whole expression). Units: `sec(s)`, `second(s)`, `min(s)`, `minute(s)`, `hour(s)`, `day(s)`, `week(s)`, `month(s)`, `year(s)`. Composite forms like `"+1 day 2 hours"`, `"an hour"`, and `"a day ago"` are supported. Day/week offsets honor DST through libc `mktime` normalization.
- **Relative units** — `this <unit>` (no change), `next <unit>` (+1), and `last <unit>` (-1) for the units `second`, `minute`, `hour`, `day`, `week`, `month`, `year` (e.g. `"next month"`, `"last year"`, `"this hour"`, `"next week"`), preserving the time of day with calendar arithmetic. The `week` unit is Monday-anchored, matching PHP.
- **Named weekdays** — `Monday`..`Sunday` and 3-letter abbreviations `Mon`..`Sun`. Modifiers: `next <weekday>` (next future occurrence; today + 7 if today matches), `last <weekday>` (most recent past; today - 7 if today matches), `this <weekday>` (delta may be zero when today matches). Result is midnight of the target day.
- **`first/last day of <modifier> month`** — `this`, `next`, `last`/`previous`/`prev` select the month; `first day of` sets the 1st and `last day of` the final day. The time of day is preserved (e.g. `"last day of next month"`).
- **`<ordinal> <weekday> of <modifier> month`** — `first`..`fifth` or `last` of a named weekday (e.g. `"first monday of next month"`, `"last friday of this month"`, `"third tuesday of next month"`). A `fifth` occurrence that overflows rolls into the next month; the time resets to midnight.

An ISO 8601 datetime may carry a trailing explicit timezone: a **numeric UTC offset** `+HH:MM`, `-HH:MM`, `+HHMM` (optionally space-separated), `Z`, or the words `UTC`/`GMT` (e.g. `"2024-06-15T12:00:00+02:00"`, `"2024-06-15 12:00:00 +0200"`, `"...Z"`, `"... UTC"`). The wall-clock is then interpreted at that offset (`Z`/`UTC`/`GMT` = UTC offset 0), overriding the configured default zone. A trailing **IANA zone name** is also accepted (e.g. `"2024-06-15 12:00:00 America/New_York"`): the date/time is interpreted in that zone with full daylight-saving handling resolved from the system timezone database (so the example is `12:00` EDT = `16:00` UTC), and the previous default zone is restored afterwards. The zone name is the final space-separated token, distinguished from the time by containing a letter, so a bare `"YYYY-MM-DD HH:MM:SS"` is unaffected. Bare ordinal weekdays without `of <month>` (`"first monday"`) are not accepted (use `next monday` or the `of <month>` form). Malformed input returns `false` (use `=== false` to detect failure). Pre-1900 year handling is described under [Date and Time](datetime.md).

## JSON

| Function | Signature | Description |
|---|---|---|
| `json_encode()` | `json_encode($value, $flags = 0, $depth = 512): string\|false` | Encode as JSON. Supports int, float, string, bool, null, arrays, mixed payloads, and objects (public properties + `JsonSerializable::jsonSerialize()` dispatch). Multibyte UTF-8 characters are escaped to `\uXXXX` by default (and to surrogate pairs for codepoints ≥ U+10000). `$flags` observes `JSON_UNESCAPED_SLASHES`, `JSON_UNESCAPED_UNICODE`, `JSON_PRETTY_PRINT`, `JSON_FORCE_OBJECT`, `JSON_NUMERIC_CHECK`, `JSON_PRESERVE_ZERO_FRACTION`, `JSON_HEX_TAG`, `JSON_HEX_AMP`, `JSON_HEX_APOS`, `JSON_HEX_QUOT`, `JSON_PARTIAL_OUTPUT_ON_ERROR`, and `JSON_THROW_ON_ERROR` (Inf/NaN trigger `JSON_ERROR_INF_OR_NAN`, and `$depth` overrun triggers `JSON_ERROR_DEPTH`; the throw flag promotes both to `JsonException`, while partial-output keeps the substituted JSON string). `$depth` defaults to 512 and is enforced for every container encoder (assoc arrays, indexed arrays, objects). The remaining `JSON_INVALID_UTF8_*` flags are accepted and observed for malformed UTF-8 strings. |
| `json_decode()` | `json_decode($json, $associative = null, $depth = 512, $flags = 0): mixed` | Full structural decoder. Returns a boxed `Mixed` cell whose runtime tag matches the decoded JSON value (null/bool/int/float/string/array/object). For JSON objects, `$associative` selects the shape: the PHP default (`null`/`false`) returns a `stdClass` instance whose properties are accessible with `$obj->name`; `true` returns an associative array indexable with `$obj["name"]`. Property access on the decoded `Mixed` is supported directly — codegen unboxes the cell, checks the stdClass class_id, and routes through the dynamic-property hash. `$depth` is enforced (`JSON_ERROR_DEPTH` on overflow). Failed decodes record PHP 8.6-style one-based line/column data for `json_last_error_msg()` and `JSON_THROW_ON_ERROR`. `$flags` observes `JSON_THROW_ON_ERROR` (raises `JsonException` on syntax/depth failure) and `JSON_BIGINT_AS_STRING` (integer tokens overflowing PHP_INT return as preserved-digit strings instead of wrapping through `__rt_atoi`). |
| `json_last_error()` | `json_last_error(): int` | Returns the runtime's last JSON error code (`JSON_ERROR_*`). |
| `json_last_error_msg()` | `json_last_error_msg(): string` | Returns the PHP-compatible message for `json_last_error()` (e.g. `"No error"`, `"Syntax error"`). After `json_decode()` failures, syntax/control-character/depth/UTF-8/UTF-16 messages include a `" near location line:column"` suffix while numeric error codes stay unchanged. |
| `json_validate()` | `json_validate($json, $depth = 512, $flags = 0): bool` | RFC 8259 validator. Returns whether `$json` is syntactically valid, sets `json_last_error()` on failure, and accepts only `0` or `JSON_INVALID_UTF8_IGNORE` for `$flags` (matching PHP 8.3). |

### Constants

The full PHP `JSON_*` family is exposed and can be combined with the bitwise OR operator to build flag arguments.

| Encoding flags | Value | Decoding flags | Value |
|---|---|---|---|
| `JSON_HEX_TAG` | 1 | `JSON_OBJECT_AS_ARRAY` | 1 |
| `JSON_HEX_AMP` | 2 | `JSON_BIGINT_AS_STRING` | 2 |
| `JSON_HEX_APOS` | 4 | | |
| `JSON_HEX_QUOT` | 8 | | |
| `JSON_FORCE_OBJECT` | 16 | | |
| `JSON_NUMERIC_CHECK` | 32 | | |
| `JSON_UNESCAPED_SLASHES` | 64 | | |
| `JSON_PRETTY_PRINT` | 128 | | |
| `JSON_UNESCAPED_UNICODE` | 256 | | |
| `JSON_PARTIAL_OUTPUT_ON_ERROR` | 512 | | |
| `JSON_PRESERVE_ZERO_FRACTION` | 1024 | | |
| `JSON_INVALID_UTF8_IGNORE` | 1048576 | | |
| `JSON_INVALID_UTF8_SUBSTITUTE` | 2097152 | | |
| `JSON_THROW_ON_ERROR` | 4194304 | | |

| Error code | Value | Error code | Value |
|---|---|---|---|
| `JSON_ERROR_NONE` | 0 | `JSON_ERROR_RECURSION` | 6 |
| `JSON_ERROR_DEPTH` | 1 | `JSON_ERROR_INF_OR_NAN` | 7 |
| `JSON_ERROR_STATE_MISMATCH` | 2 | `JSON_ERROR_UNSUPPORTED_TYPE` | 8 |
| `JSON_ERROR_CTRL_CHAR` | 3 | `JSON_ERROR_INVALID_PROPERTY_NAME` | 9 |
| `JSON_ERROR_SYNTAX` | 4 | `JSON_ERROR_UTF16` | 10 |
| `JSON_ERROR_UTF8` | 5 | | |

### Classes and interfaces

| Symbol | Kind | Description |
|---|---|---|
| `JsonSerializable` | Interface | Implementing classes can override `jsonSerialize(): mixed`; `json_encode()` dispatches to it instead of walking public properties. |
| `Error` | Class | Base PHP error throwable with `message: string`, `code: int`, `previous: ?Throwable`, `__construct(string $message = "", int $code = 0, ?Throwable $previous = null)`, and the standard `Throwable` methods. `FiberError` extends this class. |
| `UnhandledMatchError` | Class | `extends Error` and inherits its constructor and standard `Throwable` API. Explicit instances can be thrown and caught; an implicit unmatched `match` currently remains a fatal runtime error. |
| `Exception` | Class | Base PHP exception with `message: string`, `code: int`, `previous: ?Throwable`, `__construct(string $message = "", int $code = 0, ?Throwable $previous = null)`, and the standard `Throwable` methods. |
| `RuntimeException` | Class | `extends Exception`. Standard PHP "runtime errors" base class. |
| `JsonException` | Class | `extends RuntimeException`. Carries the originating `JSON_ERROR_*` code; `getCode()` returns it (e.g. 4 = SYNTAX, 1 = DEPTH, 10 = UTF16, 7 = INF_OR_NAN). |
| `stdClass` | Class | Dynamic-property container. `$obj = new stdClass(); $obj->name = "x";` works for any property name; storage is a backing hash on the instance. `json_decode($json)` returns stdClass by default (PHP semantics); pass `assoc: true` to get an associative array. |

Encoding rules for objects:

- Classes that **implement `JsonSerializable`** dispatch to `$this->jsonSerialize()` and the returned value is encoded recursively.
- Classes that **do not** implement `JsonSerializable` are encoded as a JSON object whose keys are the **public** properties (private and protected properties are skipped), in declaration order, including inherited public properties.

### Current limitations

- `json_decode()` is a full checked structural decoder: every JSON value type round-trips through a real recursive-descent parser into a boxed `Mixed` cell, and malformed input records the JSON error inside the decode walk instead of running a separate full-buffer validation pass first. Decode failures also record the source offset and format the PHP 8.6 location suffix (`near location line:column`) for `json_last_error_msg()` and `JsonException` messages without changing `json_last_error()` codes. `null` → `Mixed(null)`, `true`/`false` → `Mixed(bool)`, integers → `Mixed(int)`, floats → `Mixed(float)`, strings → `Mixed(str)` with full escape decoding (`\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uXXXX` including surrogate pairs), arrays → `Mixed(array<Mixed>)` with each element recursively decoded, and objects → either a `stdClass` instance (PHP default, `assoc=false`/`null`) or `Mixed(assoc)` (`assoc=true`). The associativity flag is threaded through `_json_decode_assoc` so nested objects share the caller's choice. Container parsing uses a depth-and-string-aware boundary scanner so commas and brackets inside string values never confuse the element/pair detection. Property access (`$obj->name`), `[]` indexing (`$arr["k"]`, `$arr[0]`), and `count()` all work directly on Mixed-typed `json_decode` results: codegen routes through `__rt_mixed_property_get` / `__rt_mixed_array_get` / `__rt_mixed_count` which unbox the cell, dispatch by runtime tag (indexed array / assoc / stdClass), and re-box typed payloads back into a Mixed cell. Missing keys, out-of-bounds indices, and unknown properties all return `Mixed(null)` instead of erroring, mirroring PHP's quiet "undefined index" / "property on non-object" warnings. Use `intval()` / `floatval()` or explicit `(int)` / `(float)` / `(string)` casts to lift a `Mixed` payload back to a typed value before arithmetic since elephc's type system requires numeric operands for `+` and Mixed alone does not satisfy the contract.
- `json_encode()` observes `JSON_UNESCAPED_SLASHES` (default escapes `/` as `\/`), `JSON_UNESCAPED_UNICODE` (default escapes multibyte UTF-8 to `\uXXXX`, surrogate pairs for codepoints ≥ U+10000), `JSON_PRETTY_PRINT` (4-space indentation, newlines between elements, single space after `:`), `JSON_FORCE_OBJECT` (indexed arrays encode as `{"0":val,"1":val,...}`), `JSON_NUMERIC_CHECK` (numeric-looking strings encode as raw JSON numbers per RFC 8259 grammar), `JSON_PRESERVE_ZERO_FRACTION` (integer-valued floats stay `1.0` instead of collapsing to `1`), the full `JSON_HEX_TAG/AMP/APOS/QUOT` family (replaces `<`/`>`, `&`, `'`, `"` with their `\uXXXX` form), Inf/NaN detection (sets `JSON_ERROR_INF_OR_NAN`; under `JSON_THROW_ON_ERROR` raises `JsonException`, otherwise returns `false` unless `JSON_PARTIAL_OUTPUT_ON_ERROR` is set), and **malformed UTF-8 detection**: every multibyte byte is validated (lead-byte range, continuation bytes, truncated sequences). Without sanitization flags this sets `JSON_ERROR_UTF8` and returns `false`; `JSON_INVALID_UTF8_IGNORE` drops malformed bytes silently without raising the error code; `JSON_INVALID_UTF8_SUBSTITUTE` replaces malformed bytes with `�` (or the U+FFFD UTF-8 bytes when `JSON_UNESCAPED_UNICODE` is also set). `JSON_PARTIAL_OUTPUT_ON_ERROR` keeps the partial output for errors that can be substituted.

- `JSON_THROW_ON_ERROR` is observed by `json_encode()` for non-finite floats (Inf/NaN trigger `JSON_ERROR_INF_OR_NAN`), for malformed UTF-8 input (`JSON_ERROR_UTF8`), and by `json_decode()` (`JSON_ERROR_SYNTAX`, `JSON_ERROR_DEPTH`, `JSON_ERROR_UTF16`). Decode exceptions use the same location-aware message string as `json_last_error_msg()` when the failing byte offset is known. PHP does not allow this flag for `json_validate()`; elephc rejects it at compile time when the flag expression is static. The throw helper records the error code in `_json_last_error` so `json_last_error()` / `json_last_error_msg()` keep working when the flag is clear.
- `JSON_ERROR_UTF16` is set by `json_decode()` and `json_validate()` whenever a `\uXXXX` escape in the high-surrogate range (`0xD800..0xDBFF`) is not immediately followed by a low-surrogate `\uYYYY` (`0xDC00..0xDFFF`), or when a low surrogate appears without a preceding high surrogate. The detector walks the surrogate-pair handshake byte by byte, so any malformed second escape (truncated `\u`, non-hex digit, or out-of-range codepoint) routes to UTF16 instead of SYNTAX, matching PHP's exact behavior.
- The `$depth` argument is observed by all three JSON entry points but with the PHP-faithful split: `json_encode()` allows up to `$depth` levels of nesting (`active <= limit`), while `json_decode()` and `json_validate()` reject when the active nesting depth reaches `$depth` (`active >= limit`). For example, `json_decode("[1]", false, 1)` sets `JSON_ERROR_DEPTH` even though the input only nests one level deep, matching PHP. For `json_encode()` and `json_decode()`, `JSON_THROW_ON_ERROR` promotes the error to `JsonException`.
- `JSON_BIGINT_AS_STRING` is observed by `json_decode()`. When set, integer-grammar JSON tokens (no `.`, no `e`/`E`) whose magnitude exceeds `PHP_INT_MAX` (`9223372036854775807`) are returned as a `Mixed(string)` preserving the original digits; in-range integers and any token containing `.`/`e`/`E` are unaffected. Detection is a length-then-lex compare against the threshold strings `9223372036854775807` (positive) / `-9223372036854775808` (negative), which is safe because the fused number validator rejects RFC 8259 leading zeros — equal-length leading-zero-free decimal strings compare lexicographically the same as numerically. The flag threads through nested arrays and objects via the global `_json_active_flags` slot, so a bigint inside a decoded array is also returned as a string.
- `json_validate()` is a recursive-descent RFC 8259 validator: it matches the literals `null`/`true`/`false`, validates the full number grammar (`-?(0|[1-9][0-9]*)(.[0-9]+)?([eE][+-]?[0-9]+)?`), checks every string escape (`\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uHHHH` with four hex digits), verifies bracket pairing in arrays/objects, requires colons between keys and values, and rejects trailing content after the value. Recursion depth is enforced against the `$depth` argument (default 512); on overflow it records `JSON_ERROR_DEPTH`. Every other malformed token records `JSON_ERROR_SYNTAX`.
- `catch (Throwable $e)` supports dispatch to the standard `Throwable` method surface: `getMessage()`, `getCode()`, `getFile()`, `getLine()`, `getTrace()`, `getTraceAsString()`, `getPrevious()`, and `__toString()`.
- Associative arrays whose keys form a sequential `0..count-1` sequence in insertion order encode as JSON arrays (`[...]`) — matching PHP's runtime detection. `__rt_json_encode_assoc` tracks that shape during the main hash walk, emits a provisional object form, and compacts the finished buffer in-place to array form only when every key matched. `JSON_FORCE_OBJECT` disables compaction so that flag still wins. Empty associative arrays also encode as `[]` (PHP's `json_encode([])` semantics).
- Floats encode at PHP's `serialize_precision = -1` — the shortest decimal that round-trips back to the same `double` (so `json_encode(1.0/3.0)` is `0.3333333333333333`, not the 14-digit `echo`/`(string)` form, and `json_encode(0.1 + 0.2)` is `0.30000000000000004`). The JSON number layout differs from `var_export`: integer-valued floats drop the fraction (`json_encode(100.0)` is `100`, not `100.0`) unless `JSON_PRESERVE_ZERO_FRACTION` is set, and exponential magnitudes use a lowercase `e` with a `d.d` mantissa and a no-leading-zero exponent (`1.0e+17`, `1.0e-6`). The decimal/exponential boundary matches PHP (`zend_gcvt`): exponential when `decpt < -3` or `decpt > 17`. The dedicated `__rt_json_ftoa` runtime helper finds the shortest precision by probing `snprintf("%.*e", p, x)` against a `strtod` re-parse, independent of the default `precision` used elsewhere.
- JSON helpers are emitted through the shared runtime surface on every supported target. Structural decode into `Mixed`, stdClass dynamic-property helpers, JsonSerializable-aware object encoding, validation, pretty-printing, depth tracking, and JSON error-message lookup are all part of that target-aware runtime path.

## Serialization

| Function | Signature | Notes |
|---|---|---|
| `serialize()` | `serialize($value): string` | Produces PHP's `serialize()` wire format, byte-for-byte: `N;` (null), `b:0;`/`b:1;` (bool), `i:<int>;` (int), `d:<float>;` (float, shortest round-trip at `serialize_precision = -1`, with `INF`/`-INF`/`NAN` for non-finite values), `s:<bytelen>:"<raw>";` (string, raw bytes with the exact byte length and no escaping), `a:<count>:{<key><value>...}` (indexed and associative arrays, nested, with int keys as `i:K;` and string keys as `s:N:"...";`, in insertion order), and `O:<len>:"<Class>":<count>:{...}` (objects). |
| `unserialize()` | `unserialize($data, $options = []): mixed` | Parses the `serialize()` wire format back into a boxed `Mixed` value. Scalars, arrays, and objects round-trip exactly. Malformed or unsupported input returns `false`, matching PHP's failure indicator. The `$options` argument is accepted for signature compatibility and currently ignored. |

`serialize()`/`unserialize()` round-trip the scalar, array, and object subset exactly,
and the produced bytes are interchangeable with the PHP interpreter. They share the same
runtime walker family as `json_encode`/`json_decode` and reuse the shortest-float
formatter, so float output matches `json_encode`'s precision.

### Objects

Objects serialize as `O:<len>:"<Class>":<count>:{...}` with PHP's exact property-key
mangling: public properties use the bare name, protected use `\0*\0name`, and private
use `\0Class\0name`. Properties are emitted in declaration order (inherited first).

Serialization magic methods are honoured:

- **`__serialize(): array`** — when defined, the object body is the returned array's
  `key;value;` pairs instead of the raw properties.
- **`__unserialize(array $data): void`** — when defined, the parsed body is passed to it
  to restore the object (instead of injecting properties by name). Its `$data` parameter
  is treated as a string/int-keyed array so `$data['key']` works (a bare `array` hint
  otherwise resolves to an integer-indexed array).
- **`__sleep(): array`** — serializes only the named properties, in `__sleep()`'s order,
  using their mangled keys.
- **`__wakeup(): void`** — runs after properties are injected (when `__unserialize()` is
  not defined).

Repeated objects within a single `serialize()` call are emitted as `r:<index>;`
back-references (PHP's global value counter: every value consumes the next index, array
keys do not), and `unserialize()` rebuilds them as a single shared instance so `===`
identity is preserved. This is the same machinery used to persist `Phar` global metadata
(see [Streams](streams.md)).

**Limitations:** a cyclic reference *inside an object's own properties* resolves to
`null` on `unserialize()` (serialization itself handles cycles correctly), and the
deprecated `Serializable` interface (`C:` wire form) is not supported.

## Regex

Regex functions and SPL regex iterators are documented in [Regex](regex.md),
including the PCRE2 native library requirements for compiling programs that use
them.

## Streams

Stream resources, standard streams, wrappers (`php://`, `data://`, `phar://`,
`http://`, `https://`, `ftp://`, `ftps://`, compression wrappers, and
`glob://`), stream contexts, filters, sockets, TLS, process pipes, and user
wrappers are documented in [Streams](streams.md).

## File system

| Function | Signature | Description |
|---|---|---|
| `file_get_contents()` | `file_get_contents($filename): string\|false` | Read an entire file, or `false` if it cannot be opened. A literal `phar://` URL is decoded at compile time; non-literal `phar://` is read at runtime. Native PHAR, tar-based PHAR, and zip-based PHAR containers are readable; native gzip/bzip2 entries and ZIP deflate entries are decoded transparently. Literal and runtime-string `http://`, `https://`, `ftp://`, and `ftps://` URLs open the matching wrapper, read the whole body, and return it (`false` on a failed open). |
| `file_put_contents()` | `file_put_contents($filename, $data): int` | Write file |
| `file()` | `file($filename): array` | Read into array of lines |
| `file_exists()` | `file_exists($filename): bool` | Check exists |
| `is_file()` | `is_file($filename): bool` | Is regular file |
| `is_dir()` | `is_dir($filename): bool` | Is directory |
| `is_readable()` | `is_readable($filename): bool` | Is readable |
| `is_writable()` | `is_writable($filename): bool` | Is writable |
| `filesize()` | `filesize($filename): int` | File size in bytes |
| `filemtime()` | `filemtime($filename): int` | Modification time |
| `disk_free_space()` | `disk_free_space($directory): float` | Free bytes of the filesystem holding `$directory`; `0.0` on failure |
| `disk_total_space()` | `disk_total_space($directory): float` | Total bytes of the filesystem holding `$directory`; `0.0` on failure |
| `copy()` | `copy($source, $dest): bool` | Copy file |
| `rename()` | `rename($old, $new): bool` | Rename/move |
| `unlink()` | `unlink($filename): bool` | Delete file |
| `mkdir()` | `mkdir($pathname): bool` | Create directory |
| `rmdir()` | `rmdir($pathname): bool` | Remove directory |
| `scandir()` | `scandir($directory): array` | List files |
| `opendir()` | `opendir($directory): resource\|false` | Open a directory stream for iteration with `readdir()`; returns a stream resource, or `false` on failure |
| `readdir()` | `readdir($dir_handle): string\|false` | Read the next entry name from a directory handle (including `.` and `..`); returns `false` once every entry has been read |
| `closedir()` | `closedir($dir_handle): void` | Close a directory handle opened by `opendir()` |
| `rewinddir()` | `rewinddir($dir_handle): void` | Rewind a directory handle back to its first entry |
| `glob()` | `glob($pattern): array` | Find matching files |
| `getcwd()` | `getcwd(): string` | Current working directory |
| `chdir()` | `chdir($directory): bool` | Change directory |
| `tempnam()` | `tempnam($dir, $prefix): string` | Create temp filename |
| `sys_get_temp_dir()` | `sys_get_temp_dir(): string` | System temp directory |

## Symbolic links

| Function | Signature | Description |
|---|---|---|
| `symlink()` | `symlink($target, $link): bool` | Create a symbolic link at `$link` pointing at `$target`. |
| `link()` | `link($target, $link): bool` | Create a hard link `$link` for an existing path `$target`. |
| `readlink()` | `readlink($path): string\|false` | Read the target of a symbolic link. Returns `false` on failure. |
| `linkinfo()` | `linkinfo($path): int` | Returns the device id (`st_dev`) of the link, or `-1` on failure. |

## File metadata

| Function | Signature | Description |
|---|---|---|
| `fileatime()` | `fileatime($filename): int\|false` | Last access time as Unix timestamp, or `false` on failure |
| `filectime()` | `filectime($filename): int\|false` | Inode-change time as Unix timestamp, or `false` on failure |
| `fileperms()` | `fileperms($filename): int\|false` | Full `st_mode` (file-type bits + permissions), or `false` on failure |
| `fileowner()` | `fileowner($filename): int\|false` | Owner UID, or `false` on failure |
| `filegroup()` | `filegroup($filename): int\|false` | Group GID, or `false` on failure |
| `fileinode()` | `fileinode($filename): int\|false` | Inode number, or `false` on failure |
| `filetype()` | `filetype($filename): string\|false` | One of `"file"`, `"dir"`, `"link"`, `"char"`, `"block"`, `"fifo"`, `"socket"`, `"unknown"` for stated paths, or `false` on `lstat()` failure. Uses `lstat()` semantics. |
| `is_executable()` | `is_executable($filename): bool` | `access(path, X_OK)` |
| `is_link()` | `is_link($filename): bool` | True for symlinks (uses `lstat()`) |
| `is_writeable()` | `is_writeable($filename): bool` | Alias of `is_writable()` |
| `stat()` | `stat($filename): array\|false` | Associative array with both numeric (0..=12) and string keys (`dev`, `ino`, `mode`, `nlink`, `uid`, `gid`, `rdev`, `size`, `atime`, `mtime`, `ctime`, `blksize`, `blocks`), or `false` on failure. |
| `lstat()` | `lstat($filename): array\|false` | Same shape as `stat()` but does not follow symlinks, or `false` on failure |
| `fstat()` | `fstat(resource $handle): array\|false` | Same shape as `stat()` but operates on an open stream resource, or `false` on failure |
| `clearstatcache()` | `clearstatcache($clear_realpath_cache = false, $filename = ""): void` | No-op (elephc does not cache `stat()` results). Arguments are still evaluated. |

> The 13 `stat()` / `lstat()` / `fstat()` fields are inserted in PHP's documented order. Check the return value against `false` before reading fields when the path or stream may be invalid.

## Path manipulation

| Function | Signature | Description |
|---|---|---|
| `basename()` | `basename($path [, $suffix]): string` | Trailing name component. `$suffix` is trimmed when it is a strict suffix of the result. |
| `dirname()` | `dirname($path [, $levels = 1]): string` | Parent directory. Repeats the parent lookup when `$levels` is greater than 1. |
| `pathinfo()` | `pathinfo($path [, $flag]): array\|string` | Without a flag, or with `PATHINFO_ALL`: associative array with keys `dirname`, `basename`, `extension` (when the basename contains a dot), `filename`. With component flags (`DIRNAME`, `BASENAME`, `EXTENSION`, `FILENAME`): the corresponding string. Runtime-computed flags are supported. |
| `realpath()` | `realpath($path): string\|false` | Canonicalized absolute path, or `false` when the path does not exist. |
| `realpath_cache_get()` | `realpath_cache_get(): array` | Empty array; elephc does not maintain a realpath cache. |
| `realpath_cache_size()` | `realpath_cache_size(): int` | `0`; elephc does not maintain a realpath cache. |
| `fnmatch()` | `fnmatch($pattern, $filename [, $flags = 0]): bool` | Shell-glob match. Supports `*`, `?`, `[abc]`, `[a-z]`, `[!abc]`/`[^abc]`, `\\<char>`, and PHP flags. |

> `pathinfo()` accepts `PATHINFO_DIRNAME` (1), `PATHINFO_BASENAME` (2), `PATHINFO_EXTENSION` (4), `PATHINFO_FILENAME` (8), and `PATHINFO_ALL` (15) constants, integer literals, variables, and bitmasks such as `PATHINFO_DIRNAME | PATHINFO_EXTENSION`. Component bitmasks follow PHP priority: dirname, basename, extension, then filename. The component-flag form returns the requested component as a string (or empty string when it is absent, for example `pathinfo("foo", PATHINFO_EXTENSION)` returns `""`). The no-flag and exact `PATHINFO_ALL` forms return an associative array; the `extension` key is omitted only when the basename has no dot, matching PHP's behaviour.

> `fnmatch()` supports PHP's `FNM_NOESCAPE`, `FNM_PATHNAME`, `FNM_PERIOD`, and `FNM_CASEFOLD` flags, including runtime-computed bitmasks such as `FNM_PATHNAME | FNM_CASEFOLD`. The numeric values are target-specific and follow the selected platform's PHP/libc constants.

## File modification

| Function | Signature | Description |
|---|---|---|
| `touch()` | `touch($filename [, $mtime [, $atime]]): bool` | Set access/modification times. Creates the file with permissions `0666 & umask` if missing. With no `$mtime`, or `$mtime = null`, uses the current time; with no `$atime`, or `$atime = null`, defaults to `$mtime`. On a registered `scheme://` path it dispatches to the wrapper's `stream_metadata($path, STREAM_META_TOUCH, [$mtime, $atime])` with a 2-element int array. |
| `chmod()` | `chmod($filename, $mode): bool` | Change file mode. On a registered `scheme://` path it dispatches to the wrapper's `stream_metadata($path, STREAM_META_ACCESS, $mode)` and returns its `bool` result (false when the wrapper does not implement `stream_metadata`). |
| `chown()` | `chown($filename, $user): bool` | Change owner by UID or user name. The group is left unchanged. On a registered `scheme://` path it dispatches to the wrapper's `stream_metadata($path, STREAM_META_OWNER, $uid)` (integer `$user`) or `stream_metadata($path, STREAM_META_OWNER_NAME, $name)` (string `$user`). |
| `chgrp()` | `chgrp($filename, $group): bool` | Change group by GID or group name. The owner is left unchanged. On a registered `scheme://` path it dispatches to the wrapper's `stream_metadata($path, STREAM_META_GROUP, $gid)` (integer `$group`) or `stream_metadata($path, STREAM_META_GROUP_NAME, $name)` (string `$group`). |
| `lchown()` | `lchown($filename, $user): bool` | Change a symlink's owner by UID or user name without following the link. The group is left unchanged. |
| `lchgrp()` | `lchgrp($filename, $group): bool` | Change a symlink's group by GID or group name without following the link. The owner is left unchanged. |
| `umask()` | `umask([$mask]): int` | Set the process umask and return the previous value. With no argument, returns the current umask without changing it (implemented by setting `umask(0)` and immediately restoring the original). |

> Except for `umask()`, file-modification functions return `true` on success and `false` on failure.

> `touch()` accepts integer Unix timestamps or `null` for `$mtime` / `$atime`. Numeric values, including `-1`, are treated as explicit timestamps; `null` and omitted arguments select PHP's default/current-time behaviour.

## Network utilities

| Function | Signature | Description |
|---|---|---|
| `gethostname()` | `gethostname(): string` | Return the host name of the machine running the program |
| `gethostbyname()` | `gethostbyname(string $hostname): string` | Resolve a host name to its IPv4 dotted-quad address through the system resolver; returns the host name unchanged when it cannot be resolved |
| `gethostbyaddr()` | `gethostbyaddr(string $ip): string\|false` | Reverse-resolve an IPv4 dotted-quad address to a host name; returns the address unchanged when no record exists, or `false` when it is malformed |
| `getprotobyname()` | `getprotobyname(string $protocol): int\|false` | Look up an IP protocol number by name or alias in the system protocols database; `false` when no entry matches |
| `getprotobynumber()` | `getprotobynumber(int $protocol): string\|false` | Look up an IP protocol name by number in the system protocols database; `false` when no entry matches |
| `getservbyname()` | `getservbyname(string $service, string $protocol): int\|false` | Look up an internet service port by service name or alias and protocol in the system services database; `false` when no entry matches |
| `getservbyport()` | `getservbyport(int $port, string $protocol): string\|false` | Look up an internet service name by port number and protocol in the system services database; `false` when no entry matches |

## Debugging

| Function | Signature | Description |
|---|---|---|
| `var_dump()` | `var_dump(mixed ...$values): void` | Output the type and value of each argument in source order. Homogeneous indexed arrays of `int`, `string`, `bool`, or `float` and associative arrays (hashes) print full per-element bodies (`[N]=>\n  int(V)\n`, `["key"]=>\n  string(…)\n`, etc.). Nested arrays/objects inside a Mixed-element array or hash print `NULL` (recursive nesting into those layouts is still pending). |
| `print_r()` | `print_r(mixed $value, bool $return = false): string\|true` | Human-readable output. Indexed arrays, associative arrays, and arbitrarily nested arrays print PHP's recursive `Array\n(\n    [key] => value\n)\n` layout (unquoted keys, 4 spaces of indentation per level, `1`/empty for bool `true`/`false`, empty for `null`). With `$return = true`, the rendering is returned instead of printed; captures are limited to 64 KiB. With `$return = false`, the function prints the rendering and returns `true`. |
| `var_export()` | `var_export($value, $return = false): ?string` | Parsable representation. Renders scalars (`'…'`-quoted strings with `\\`/`\'` escaping, `true`/`false`, `NULL`, integers, and floats — an integer-valued float gains a `.0`) and arbitrarily nested arrays in PHP's `array (\n  key => value,\n)` layout (2 spaces of indentation per level, integer keys bare and string keys quoted, nested arrays on their own line). With `$return = true` the rendering is returned instead of printed. Objects are not yet rendered (PHP emits `\Class::__set_state(...)`). Floats use PHP's `serialize_precision = -1` semantics — the shortest decimal that round-trips back to the same `double` (so `1/3` renders as `0.3333333333333333`, not the 14-digit `(string)` form), with the same scientific layout as PHP (`1.0E+17`, `1.0E-6`), an integer-valued float gaining `.0` (`1.0`, `100.0`), and `-0.0`, `INF`, `-INF`, `NAN` preserved. This is independent of the default `precision` used by `echo`/`(string)`. |

```php
<?php
$arr = [1, 2, 3];
var_dump($arr);
// array(3) {
//   [0]=> int(1)
//   [1]=> int(2)
//   [2]=> int(3)
// }

var_export(['a' => 1, 'b' => [2, 3]]);
// array (
//   'a' => 1,
//   'b' =>
//   array (
//     0 => 2,
//     1 => 3,
//   ),
// )
```

## Output buffering

Output buffering captures everything a piece of code prints — `echo`, `print`,
`printf()`, `print_r()`, `var_dump()`, `readfile()`, `fpassthru()` — into an
in-memory buffer instead of writing it to stdout. Buffers nest: flushing an
inner buffer folds its contents into the enclosing one. Buffers still active at
script end (including `exit()`/`die()`) are flushed to stdout automatically,
matching PHP.

| Function | Signature | Description |
|---|---|---|
| `ob_start()` | `ob_start(?callable $callback = null, int $chunk_size = 0, int $flags = 112): bool` | Start a new output buffer (nesting supported, up to 64 levels). Output-handler callbacks are supported: closures, first-class callables, function-name strings, and boxed callables run on flush/clean with PHP's phase bits (`WRITE`/`CLEAN`/`FLUSH`/`FINAL`, plus `START` on the first run); returning `false` passes the raw contents through, any other return is cast to a string. A non-zero `$chunk_size` auto-flushes the buffer whenever it reaches that many bytes, and `$flags` gate cleanability/flushability/removability exactly like PHP (refusals raise PHP's notices). An unknown function name raises PHP's warning and returns `false`; array-pair callables (`[$obj, 'method']`) are rejected at compile time |
| `ob_get_contents()` | `ob_get_contents(): string\|false` | Return the active buffer's contents without consuming them; `false` when no buffer is active |
| `ob_get_clean()` | `ob_get_clean(): string\|false` | Return the active buffer's contents, then discard the buffer and pop it |
| `ob_get_flush()` | `ob_get_flush(): string\|false` | Return the active buffer's contents, then flush them to the parent sink and pop the buffer |
| `ob_get_length()` | `ob_get_length(): int\|false` | Byte length of the active buffer's contents; `false` when no buffer is active |
| `ob_get_level()` | `ob_get_level(): int` | Current nesting depth (0 = no buffering) |
| `ob_clean()` | `ob_clean(): bool` | Erase the active buffer's contents while keeping the buffer active |
| `ob_end_clean()` | `ob_end_clean(): bool` | Discard the active buffer's contents and pop the buffer |
| `ob_end_flush()` | `ob_end_flush(): bool` | Flush the active buffer's contents to the parent sink (enclosing buffer or stdout) and pop the buffer |
| `ob_flush()` | `ob_flush(): bool` | Flush the active buffer's contents to the parent sink while keeping the buffer active |
| `ob_get_status()` | `ob_get_status(bool $full_status = false): array` | Status of the active buffer (or an empty array without one): `name`, `type`, `flags`, `level`, `chunk_size`, `buffer_size`, `buffer_used`. With `$full_status = true`, one entry per nesting level |
| `ob_implicit_flush()` | `ob_implicit_flush(bool $enable = true): bool` | Stores the flag and returns `true`. Semantically inert: elephc terminal writes are unbuffered syscalls, so implicit flushing is always effectively on |
| `ob_list_handlers()` | `ob_list_handlers(): array` | One `"default output handler"` entry per active buffer level |

```php
<?php
ob_start();
echo "hello";
$captured = ob_get_clean();
echo strtoupper($captured);   // HELLO

ob_start();
echo "outer:";
ob_start();
echo "inner";
ob_end_flush();               // "inner" flows into the outer buffer
echo ob_get_clean(), "\n";    // outer:inner
```

Output buffering state is shared between statically compiled code and
`eval()`'d code: a buffer started inside `eval()` captures static echoes and
vice versa, and handlers registered inside `eval()` run on flushes triggered
from static code or at script end. Output produced inside a handler is
discarded and calling `ob_start()` from inside a handler is a fatal error,
matching PHP. `fwrite(STDOUT, ...)` writes to the real file descriptor and
bypasses output buffers, matching PHP. Known divergences: the first-class
callable of a named function reports that function's name (PHP reports
`Closure::__invoke`), and `ob_get_status()` omits PHP's internal 0x2000
disabled bit.
