//! Purpose:
//! Integration tests for the builtin date/time classes (`DateTimeInterface`, `DateTimeZone`,
//! `DateTimeImmutable`). Covers construction, the timezone name round-trip, timestamp access,
//! `format()` delegation to `date()`, and the `DateTimeInterface` contract.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Assertions stay deterministic: `"now"` timestamps are only range-checked, and `format()`
//!   output is length-checked rather than compared against a wall-clock value.

use super::*;

/// Reproduces php-src bug #62500 for DateInterval subclasses with user properties.
#[test]
fn test_dateinterval_subclass_untyped_default_and_numeric_property_warning() {
    let output = compile_and_run_capture(
        r#"<?php
class Crasher extends DateInterval {
    public $foo;
    public function __construct($time_spec) {
        var_dump($this->foo);
        $this->foo = 3;
        var_dump($this->foo);
        var_dump($this->{2});
        parent::__construct($time_spec);
    }
}
try { new Crasher('blah'); }
catch (Exception $e) { var_dump($e->getMessage()); }
"#,
    );
    assert!(output.success, "DateInterval subclass fixture failed: {}", output.stderr);
    assert_eq!(
        output.stdout,
        "NULL\nint(3)\nNULL\nstring(28) \"Unknown or bad format (blah)\"\n"
    );
    assert!(
        output.stderr.contains("\nWarning: Undefined property: Crasher::$2 in ")
            && output.stderr.ends_with(" on line 8\n"),
        "unexpected numeric-property warning: {}",
        output.stderr
    );
}

/// Verifies `DateTimeZone` stores and returns its identifier via `getName()`.
#[test]
fn test_datetime_zone_get_name() {
    let out = compile_and_run(
        r#"<?php
$tz = new DateTimeZone("Europe/Paris");
echo $tz->getName();
"#,
    );
    assert_eq!(out, "Europe/Paris");
}

/// Reproduces php-src `timezone_open_warning.phpt`: valid identifiers return
/// objects while invalid scalar identifiers warn at the call site and return false.
#[test]
fn test_timezone_open_invalid_identifier_warns_and_returns_false() {
    let output = compile_and_run_capture(
        r#"<?php
echo timezone_open("+02:30")->getName(), "\n";
var_dump(timezone_open(2.5));
var_dump(timezone_open(timezone: "Europe/Lviv"));
"#,
    );
    assert!(output.success, "timezone_open fixture failed: {}", output.stderr);
    assert_eq!(output.stdout, "+02:30\nbool(false)\nbool(false)\n");
    assert!(
        output.stderr.contains(
            "\nWarning: timezone_open(): Unknown or bad timezone (2.5) in "
        ),
        "missing numeric timezone warning: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains(
            "\nWarning: timezone_open(): Unknown or bad timezone (Europe/Lviv) in "
        ),
        "missing identifier timezone warning: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains(" on line 3\n")
            && output.stderr.ends_with(" on line 4\n"),
        "unexpected timezone warning source lines: {}",
        output.stderr
    );
}

/// Verifies timezone entry points reject embedded NUL bytes with php-src's
/// `ValueError` messages before identifier parsing or procedural warning conversion.
#[test]
fn test_timezone_entry_points_reject_embedded_null_bytes() {
    let out = compile_and_run(
        r#"<?php
$timezone = "Europe/Zurich" . chr(0) . "Foo";
try { timezone_open($timezone); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { new DateTimeZone($timezone); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "timezone_open(): Argument #1 ($timezone) must not contain any null bytes\n\
DateTimeZone::__construct(): Argument #1 ($timezone) must not contain any null bytes\n"
    );
}

/// Verifies PER_COUNTRY rejects missing and non-two-letter country codes while
/// preserving the procedural entry-point name in `ValueError` diagnostics.
#[test]
fn test_timezone_identifiers_list_validates_country_code() {
    let out = compile_and_run(
        r#"<?php
foreach ([null, "A"] as $country) {
    try { timezone_identifiers_list(DateTimeZone::PER_COUNTRY, $country); }
    catch (ValueError $e) { echo $e->getMessage(), "\n"; }
}
"#,
    );
    let message = "timezone_identifiers_list(): Argument #2 ($countryCode) must be a two-letter ISO 3166-1 compatible country code when argument #1 ($timezoneGroup) is DateTimeZone::PER_COUNTRY\n";
    assert_eq!(out, format!("{message}{message}"));
}

/// Reproduces php-src's public timezone setter gate: canonical and legacy
/// identifiers succeed, fixed offsets and unknown names return false without
/// changing the active timezone, and unsuppressed failures emit a call-site notice.
#[test]
fn test_date_default_timezone_set_validates_php_src_identifiers() {
    let output = compile_and_run_capture(
        r#"<?php
date_default_timezone_set("Europe/Paris");
foreach (["UTC", "Zulu", "GMT0", "EST5EDT", "GMT+0"] as $zone) {
    var_dump(date_default_timezone_set($zone));
}
date_default_timezone_set("Europe/Paris");
foreach (["+02:00", "UTC-2", "foo"] as $zone) {
    var_dump(@date_default_timezone_set($zone));
    echo date_default_timezone_get(), "\n";
}
var_dump(date_default_timezone_set("Not/AZone"));
"#,
    );
    assert!(
        output.success,
        "date_default_timezone_set fixture failed: {}",
        output.stderr
    );
    assert_eq!(
        output.stdout,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n\
         bool(false)\nEurope/Paris\nbool(false)\nEurope/Paris\nbool(false)\nEurope/Paris\n\
         bool(false)\n"
    );
    assert!(
        output.stderr.contains(
            "\nNotice: date_default_timezone_set(): Timezone ID 'Not/AZone' is invalid in "
        ),
        "missing invalid-timezone notice: {}",
        output.stderr
    );
    assert!(
        output.stderr.ends_with(" on line 11\n"),
        "unexpected invalid-timezone source line: {}",
        output.stderr
    );
}

/// Verifies a `DateTimeZone` round-trips through a typed parameter and a typed object property.
#[test]
fn test_datetime_zone_typed_param_and_property() {
    let out = compile_and_run(
        r#"<?php
class Wrapper {
    public DateTimeZone $tz;
    public function __construct(DateTimeZone $tz) { $this->tz = $tz; }
}
function pick(DateTimeZone $z): DateTimeZone { return $z; }
$w = new Wrapper(pick(new DateTimeZone("UTC")));
echo $w->tz->getName();
"#,
    );
    assert_eq!(out, "UTC");
}

/// Verifies `new DateTimeImmutable("now")` stores a plausible positive UNIX timestamp.
#[test]
fn test_datetime_immutable_now_timestamp_positive() {
    let out = compile_and_run(
        r#"<?php
$dt = new DateTimeImmutable("now");
echo $dt->getTimestamp() > 1000000000 ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies the default timezone is UTC, re-materialized through `getTimezone()`.
#[test]
fn test_datetime_immutable_default_timezone_is_utc() {
    let out = compile_and_run(
        r#"<?php
$dt = new DateTimeImmutable();
echo $dt->getTimezone()->getName();
"#,
    );
    assert_eq!(out, "UTC");
}

/// Verifies `format("Y")` delegates to `date()` and yields a four-digit year string.
#[test]
fn test_datetime_immutable_format_year_length() {
    let out = compile_and_run(
        r#"<?php
$dt = new DateTimeImmutable("now");
echo strlen($dt->format("Y"));
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies `DateTimeImmutable` satisfies `instanceof DateTimeInterface`.
#[test]
fn test_datetime_immutable_implements_datetime_interface() {
    let out = compile_and_run(
        r#"<?php
$dt = new DateTimeImmutable("now");
echo $dt instanceof DateTimeInterface ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies the mutable `DateTime::setTimestamp()` stores the value and `getTimestamp()` reads it
/// back (timezone-independent). Chaining returns the same object.
#[test]
fn test_datetime_mutable_set_get_timestamp() {
    let out = compile_and_run(
        r#"<?php
$dt = new DateTime();
$dt->setTimestamp(1700000000);
echo $dt->getTimestamp();
"#,
    );
    assert_eq!(out, "1700000000");
}

/// Verifies `DateTime` also satisfies `instanceof DateTimeInterface`.
#[test]
fn test_datetime_mutable_implements_datetime_interface() {
    let out = compile_and_run(
        r#"<?php
$dt = new DateTime("now");
echo $dt instanceof DateTimeInterface ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies `DateTimeInterface` declares `diff()` and the serialization hooks from php-src.
#[test]
fn test_datetime_interface_diff_and_serialize_contract() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
function distance(DateTimeInterface $from, DateTimeInterface $to): DateInterval {
    return $from->diff($to, true);
}
function state(DateTimeInterface $date) {
    return $date->__serialize();
}
$a = new DateTime("2024-01-01");
$b = new DateTimeImmutable("2024-01-04");
$state = state($a);
echo distance($a, $b)->days, "|", $state["timezone"];
"#,
    );
    assert_eq!(out, "3|UTC");
}

/// Verifies `DateTime::setTime()` replaces the time-of-day while keeping the date (mutates `$this`).
#[test]
fn test_datetime_mutable_set_time() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setTimestamp(1700000000);
$d->setTime(10, 30, 45);
echo $d->format("H:i:s");
"#,
    );
    assert_eq!(out, "10:30:45");
}

/// Verifies `DateTime::setDate()` replaces the calendar date while keeping the time-of-day.
#[test]
fn test_datetime_mutable_set_date() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setTimestamp(1700000000);
$d->setTime(0, 0, 0);
$d->setDate(2020, 6, 15);
echo $d->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2020-06-15");
}

/// Verifies `DateTimeImmutable` setters return a NEW instance and leave the original untouched.
#[test]
fn test_datetime_immutable_setters_return_new() {
    let out = compile_and_run(
        r#"<?php
$a = (new DateTimeImmutable())->setTimestamp(1700000000);
$b = $a->setTime(8, 0, 0);
echo $b->format("H:i:s"), "|", $a->getTimestamp();
"#,
    );
    assert_eq!(out, "08:00:00|1700000000");
}

/// Verifies `DateTime::setTimezone()` stores the zone, readable back via `getTimezone()->getName()`.
#[test]
fn test_datetime_set_timezone_round_trip() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setTimezone(new DateTimeZone("America/New_York"));
echo $d->getTimezone()->getName();
"#,
    );
    assert_eq!(out, "America/New_York");
}

/// Verifies `DateTimeImmutable::setTimezone()` returns a new instance; the original keeps UTC.
#[test]
fn test_datetime_immutable_set_timezone_returns_new() {
    let out = compile_and_run(
        r#"<?php
$a = new DateTimeImmutable();
$b = $a->setTimezone(new DateTimeZone("Asia/Tokyo"));
echo $b->getTimezone()->getName(), "|", $a->getTimezone()->getName();
"#,
    );
    assert_eq!(out, "Asia/Tokyo|UTC");
}

/// Verifies `diff()` returns a DateInterval with exact total days and the H:i:s remainder.
/// 1700200000 - 1700000000 = 200000s = 2 days, 7h, 33m, 20s.
#[test]
fn test_datetime_diff_components() {
    let out = compile_and_run(
        r#"<?php
$a = (new DateTimeImmutable())->setTimestamp(1700000000);
$b = (new DateTimeImmutable())->setTimestamp(1700200000);
$iv = $a->diff($b);
echo $iv->days, " ", $iv->h, ":", $iv->i, ":", $iv->s, " inv=", $iv->invert;
"#,
    );
    assert_eq!(out, "2 7:33:20 inv=0");
}

/// Verifies `diff()` sets `invert = 1` when the target precedes `$this`.
#[test]
fn test_datetime_diff_invert() {
    let out = compile_and_run(
        r#"<?php
$a = (new DateTimeImmutable())->setTimestamp(1700200000);
$b = (new DateTimeImmutable())->setTimestamp(1700000000);
$iv = $a->diff($b);
echo $iv->days, " inv=", $iv->invert;
"#,
    );
    assert_eq!(out, "2 inv=1");
}

/// Reproduces php-src's massive-date diff fixture: civil setters must keep expanded negative
/// years and timelib must report the exact 666666-year interval in both directions.
#[test]
fn test_datetime_diff_massive_expanded_years() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("America/New_York");
$end = new DateTime();
$end->setDate(333333, 1, 1);
$end->setTime(16, 18, 2);
$start = new DateTime();
$start->setDate(-333333, 1, 1);
$start->setTime(16, 18, 2);
$positive = $start->diff($end);
$negative = $end->diff($start);
echo $start->format("Y-m-d H:i:s T"), "|", $end->format("Y-m-d H:i:s T"), "|",
     $positive->format("%R%yY%mM%dD%hH%iM%sS"), "|", $positive->days, "|",
     $negative->format("%R%yY%mM%dD%hH%iM%sS"), "|", $negative->days;
"#,
    );
    assert_eq!(
        out,
        "-333333-01-01 16:18:02 LMT|333333-01-01 16:18:02 EST|\
+666666Y0M0D0H0M0S|243494757|-666666Y0M0D0H0M0S|243494757"
    );
}

/// Verifies `diff()` works across the two classes through the DateTimeInterface contract.
#[test]
fn test_datetime_diff_cross_class() {
    let out = compile_and_run(
        r#"<?php
$a = new DateTime();
$a->setTimestamp(1700000000);
$b = (new DateTimeImmutable())->setTimestamp(1700086400);
echo $a->diff($b)->days;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies `diff()` fills the calendar `y/m/d` breakdown (not just total `days`), matching PHP —
/// computed by advancing whole years/months/days through `mktime()`. Covers the multi-borrow case
/// (2020-01-31 -> 2020-03-01 = 0y 0m 30d, NOT a partial month) and the inverted direction. The diff
/// of two same-zone timestamps is timezone-independent, so no explicit TZ is needed.
#[test]
fn test_datetime_diff_calendar_components() {
    let out = compile_and_run(
        r#"<?php
function d($ts1, $ts2) {
    $a = new DateTime(); $a->setTimestamp($ts1);
    $b = new DateTime(); $b->setTimestamp($ts2);
    $i = $a->diff($b);
    return $i->y . "/" . $i->m . "/" . $i->d;
}
echo d(mktime(0, 0, 0, 1, 1, 2020), mktime(0, 0, 0, 3, 15, 2021)), " ";
echo d(mktime(0, 0, 0, 1, 31, 2020), mktime(0, 0, 0, 3, 1, 2020)), " ";
echo d(mktime(0, 0, 0, 3, 15, 2021), mktime(0, 0, 0, 1, 1, 2020));
"#,
    );
    assert_eq!(out, "1/2/14 0/0/30 1/2/14");
}

/// Verifies `DateInterval` parses an ISO 8601 duration into its components.
/// Fixture: "P1Y2M3DT4H5M6S" → y=1,m=2,d=3,h=4,i=5,s=6.
#[test]
fn test_date_interval_parses_iso8601() {
    let out = compile_and_run(
        r#"<?php
$iv = new DateInterval("P1Y2M3DT4H5M6S");
echo $iv->y, ",", $iv->m, ",", $iv->d, ",", $iv->h, ",", $iv->i, ",", $iv->s;
"#,
    );
    assert_eq!(out, "1,2,3,4,5,6");
}

/// Verifies `DateInterval` ISO parsing: weeks contribute 7 days each and "M" before "T" is months.
#[test]
fn test_date_interval_weeks_and_minutes() {
    let out = compile_and_run(
        r#"<?php
$w = new DateInterval("P2W");
$t = new DateInterval("PT90M");
echo $w->d, "|", $t->i;
"#,
    );
    assert_eq!(out, "14|90");
}

/// Verifies `DateInterval::createFromDateString()` parses relative strings into components:
/// weeks fold into days (×7), counts are kept verbatim (no normalization), multi-unit strings
/// accumulate, and a negative count is stored in the component (invert stays 0).
#[test]
fn test_date_interval_create_from_date_string() {
    let out = compile_and_run(
        r#"<?php
$a = DateInterval::createFromDateString("2 weeks 3 days");
$b = DateInterval::createFromDateString("1 year 2 months 10 days");
$c = DateInterval::createFromDateString("90 seconds");
$d = DateInterval::createFromDateString("-1 day");
echo $a->d, "|", $b->y, ",", $b->m, ",", $b->d, "|", $c->s, "|", $d->d;
"#,
    );
    assert_eq!(out, "17|1,2,10|90|-1");
}

/// Verifies a `createFromDateString()` interval (and the `date_interval_create_from_date_string()`
/// procedural alias) drives `DateTime::add()`, with a symbolic "1 month" normalizing per calendar.
#[test]
fn test_date_interval_create_from_date_string_add() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime("2024-01-31");
$d->add(DateInterval::createFromDateString("1 month"));
$e = new DateTime("2024-06-01");
$e->add(date_interval_create_from_date_string("3 days 4 hours"));
echo $d->format("Y-m-d"), "|", $e->format("Y-m-d H:i");
"#,
    );
    assert_eq!(out, "2024-03-02|2024-06-04 04:00");
}

/// Verifies `DateInterval::format()` across PHP's `%` specifiers: lowercase no-pad, uppercase
/// 2-digit zero-pad, `%R`/`%r` sign, literal `%%`, an unknown specifier passed through verbatim,
/// and `%a` yielding `(unknown)` for a manually built interval (whose `days` property is `false`).
#[test]
fn test_date_interval_format() {
    let out = compile_and_run(
        r#"<?php
$iv = new DateInterval("P1Y2M3DT4H5M6S");
echo $iv->format("%y-%m-%d %h:%i:%s"), "|";
echo $iv->format("%Y-%M-%D %H:%I:%S"), "|";
echo $iv->format("%R %r 100%% %x"), "|";
echo $iv->format("%a");
"#,
    );
    assert_eq!(out, "1-2-3 4:5:6|01-02-03 04:05:06|+  100% %x|(unknown)");
}

/// Verifies `%a` renders the real total-day count when the interval came from `diff()`.
#[test]
fn test_date_interval_format_a_from_diff() {
    let out = compile_and_run(
        r#"<?php
$a = new DateTime("2020-01-01");
$b = new DateTime("2021-03-15");
echo $a->diff($b)->format("%a days, %h:%I:%S");
"#,
    );
    assert_eq!(out, "439 days, 0:00:00");
}

/// Regression: `DateInterval::$days` is PHP's `int|false`. A directly-constructed interval has
/// `days === false` (echoing as empty, `%a` rendering `(unknown)`), while an interval from
/// `diff()` carries the real whole-day count. This exercises the EIR object_new boxed-`false`
/// default and the `=== false` (not `< 0`) sentinel check in `%a`.
#[test]
fn test_date_interval_days_false_for_constructed() {
    let out = compile_and_run(
        r#"<?php
$w = new DateInterval("P2W");
echo ($w->days === false) ? "F" : "T";
echo "[", $w->days, "]";
echo $w->format("%a");
echo "|";
$d = (new DateTime("2024-01-01"))->diff(new DateTime("2026-01-01"));
echo ($d->days === false) ? "F" : "T";
echo $d->days;
echo $d->format("%a");
"#,
    );
    assert_eq!(out, "F[](unknown)|T731731");
}

/// Regression: relational, equality, and spaceship operators on `DateTime` objects compare by the
/// absolute instant (timestamp seconds), matching PHP. Covers `<`, `>`, `<=`, `>=`, `==`, `!=` and
/// `<=>` across distinct seconds, plus the equal-instant case where ordering is `0`/`==` is true.
#[test]
fn test_datetime_comparison_operators() {
    let out = compile_and_run(
        r#"<?php
$c = new DateTime("2024-06-15 12:00:00");
$d = new DateTime("2024-06-15 12:00:01");
echo ($c < $d) ? "T" : "F";
echo ($c > $d) ? "T" : "F";
echo ($c <= $d) ? "T" : "F";
echo ($c >= $d) ? "T" : "F";
echo ($c == $d) ? "T" : "F";
echo ($c != $d) ? "T" : "F";
echo $c <=> $d;
echo "|";
$e = new DateTime("2024-06-15 12:00:00");
echo ($c == $e) ? "T" : "F";
echo ($c <=> $e);
"#,
    );
    assert_eq!(out, "TFTFFT-1|T0");
}

/// Regression: `DateTime` equality compares the instant including microseconds and works across
/// timezones (same UTC instant) and across the `DateTime`/`DateTimeImmutable` classes, while `===`
/// stays an object-identity comparison (distinct instances are never identical).
#[test]
fn test_datetime_comparison_instant_and_identity() {
    let out = compile_and_run(
        r#"<?php
$utc = new DateTime("2024-06-15 12:00:00", new DateTimeZone("UTC"));
$ny  = new DateTime("2024-06-15 08:00:00", new DateTimeZone("America/New_York"));
echo ($utc == $ny) ? "T" : "F";
echo $utc <=> $ny;
$a = new DateTime("2024-06-15 12:00:00.100000");
$b = new DateTime("2024-06-15 12:00:00.200000");
echo ($a < $b) ? "T" : "F";
echo ($a == $b) ? "T" : "F";
echo "|";
$e = new DateTime("2024-06-15 12:00:00");
$f = new DateTimeImmutable("2024-06-15 12:00:00");
echo ($e == $f) ? "T" : "F";
echo $e <=> $f;
echo "|";
$g = new DateTime("2024-06-15 12:00:00");
$h = new DateTime("2024-06-15 12:00:00");
echo ($g === $h) ? "T" : "F";
echo ($g == $h) ? "T" : "F";
"#,
    );
    assert_eq!(out, "T0TF|T0|FT");
}

/// Verifies `DateTime::add()` shifts the date by whole days, mutating `$this`.
/// The wall clock is fixed via `setDate`/`setTime` first, so the result is timezone-independent
/// (decompose + recompose round-trips through the same local zone). This is the regression that
/// motivated the `mktime` Mixed-operand unbox fix — the `(int)date(...) + $interval->d` components
/// produce boxed Mixed values that `mktime` must unbox instead of treating as raw pointers.
#[test]
fn test_datetime_add_days() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setTimestamp(1700000000);
$d->setDate(2020, 6, 15);
$d->setTime(10, 30, 45);
$d->add(new DateInterval("P3D"));
echo $d->format("Y-m-d H:i:s");
"#,
    );
    assert_eq!(out, "2020-06-18 10:30:45");
}

/// Verifies `DateTime::add()` applies every component of a full ISO interval at once.
#[test]
fn test_datetime_add_full_interval() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setTimestamp(1700000000);
$d->setDate(2020, 6, 15);
$d->setTime(10, 30, 45);
$d->add(new DateInterval("P1Y2M3DT4H5M6S"));
echo $d->format("Y-m-d H:i:s");
"#,
    );
    assert_eq!(out, "2021-08-18 14:35:51");
}

/// Verifies `DateTime::sub()` shifts the date backwards by whole days.
#[test]
fn test_datetime_sub_days() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setTimestamp(1700000000);
$d->setDate(2020, 6, 15);
$d->setTime(10, 30, 45);
$d->sub(new DateInterval("P10D"));
echo $d->format("Y-m-d H:i:s");
"#,
    );
    assert_eq!(out, "2020-06-05 10:30:45");
}

/// Verifies `add()` recomposes via `mktime()`, inheriting PHP's calendar overflow normalization:
/// 2020-01-31 + P1M lands on 2020-03-02 (Feb 31 rolls forward), matching PHP exactly.
#[test]
fn test_datetime_add_month_overflow() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setTimestamp(1700000000);
$d->setDate(2020, 1, 31);
$d->setTime(0, 0, 0);
$d->add(new DateInterval("P1M"));
echo $d->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2020-03-02");
}

/// Verifies `DateTimeImmutable::add()` returns a NEW instance and leaves the original unchanged.
#[test]
fn test_datetime_immutable_add_returns_new() {
    let out = compile_and_run(
        r#"<?php
$a = (new DateTimeImmutable())->setTimestamp(1700000000)->setDate(2020, 6, 15)->setTime(8, 0, 0);
$b = $a->add(new DateInterval("PT2H30M"));
echo $b->format("H:i:s"), "|", $a->format("H:i:s");
"#,
    );
    assert_eq!(out, "10:30:00|08:00:00");
}

/// Verifies `add()` honors `$interval->invert`: an inverted interval subtracts instead of adding.
#[test]
fn test_datetime_add_inverted_interval_subtracts() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setTimestamp(1700000000);
$d->setDate(2020, 6, 15);
$d->setTime(10, 30, 45);
$iv = new DateInterval("P5D");
$iv->invert = 1;
$d->add($iv);
echo $d->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2020-06-10");
}

/// Verifies `foreach` over a `DatePeriod` yields each calendar step from start up to (but not
/// including) the end. Date-only formatting round-trips through the local zone, so the output
/// is machine-independent.
#[test]
fn test_date_period_monthly() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1M"), new DateTime("2024-04-01"));
$out = "";
foreach ($p as $dt) {
    $out .= $dt->format("Y-m-d") . ",";
}
echo $out;
"#,
    );
    assert_eq!(out, "2024-01-01,2024-02-01,2024-03-01,");
}

/// Verifies `DatePeriod` exposes zero-based integer keys during iteration.
#[test]
fn test_date_period_keys() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1M"), new DateTime("2024-04-01"));
$out = "";
foreach ($p as $k => $dt) {
    $out .= $k . ":" . $dt->format("m") . " ";
}
echo $out;
"#,
    );
    assert_eq!(out, "0:01 1:02 2:03 ");
}

/// Verifies `DatePeriod::EXCLUDE_START_DATE` skips the start date in iteration.
#[test]
fn test_date_period_exclude_start() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(
    new DateTime("2024-01-01"),
    new DateInterval("P1M"),
    new DateTime("2024-04-01"),
    DatePeriod::EXCLUDE_START_DATE
);
$out = "";
foreach ($p as $dt) {
    $out .= $dt->format("Y-m-d") . ",";
}
echo $out;
"#,
    );
    assert_eq!(out, "2024-02-01,2024-03-01,");
}

/// Verifies `DatePeriod::INCLUDE_END_DATE` includes the end date when it lands on a step.
#[test]
fn test_date_period_include_end() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(
    new DateTime("2024-01-01"),
    new DateInterval("P1M"),
    new DateTime("2024-04-01"),
    DatePeriod::INCLUDE_END_DATE
);
$out = "";
foreach ($p as $dt) {
    $out .= $dt->format("Y-m-d") . ",";
}
echo $out;
"#,
    );
    assert_eq!(out, "2024-01-01,2024-02-01,2024-03-01,2024-04-01,");
}

/// Verifies the `(start, interval, recurrences)` count form: an int third argument
/// yields `recurrences + 1` dates (the start plus that many steps), and
/// `getRecurrences()` reports the count.
#[test]
fn test_date_period_recurrences() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), 3);
$out = "";
foreach ($p as $dt) {
    $out .= $dt->format("m-d") . ",";
}
echo $out . "rec=" . $p->getRecurrences();
"#,
    );
    assert_eq!(out, "01-01,01-02,01-03,01-04,rec=3");
}

/// Verifies count-form periods apply INCLUDE_END_DATE independently of the explicit recurrence
/// count, including its interaction with EXCLUDE_START_DATE and the virtual recurrences property.
#[test]
fn test_date_period_count_options_change_iteration_bounds() {
    let out = compile_and_run(
        r#"<?php
$start = new DateTime("2024-01-01", new DateTimeZone("UTC"));
$interval = new DateInterval("P1D");
foreach ([0, DatePeriod::EXCLUDE_START_DATE, DatePeriod::INCLUDE_END_DATE,
          DatePeriod::EXCLUDE_START_DATE | DatePeriod::INCLUDE_END_DATE] as $option) {
    $period = new DatePeriod($start, $interval, 2, $option);
    foreach ($period as $date) { echo $date->format("m-d"), ","; }
    echo "|", $period->recurrences, "\n";
}
"#,
    );
    assert_eq!(
        out,
        "01-01,01-02,01-03,|3\n\
         01-02,01-03,|2\n\
         01-01,01-02,01-03,01-04,|4\n\
         01-02,01-03,01-04,|3\n"
    );
}

/// Verifies the count form honors `EXCLUDE_START_DATE` (the start is dropped, leaving
/// exactly `recurrences` dates) and that `getRecurrences()` is `null` for the end-date
/// form (which echoes as the empty string).
#[test]
fn test_date_period_recurrences_exclude_start() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(
    new DateTime("2024-01-01"),
    new DateInterval("P1D"),
    3,
    DatePeriod::EXCLUDE_START_DATE
);
$out = "";
foreach ($p as $dt) {
    $out .= $dt->format("m-d") . ",";
}
$end = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), new DateTime("2024-01-03"));
echo $out . "endRec=[" . $end->getRecurrences() . "]";
"#,
    );
    assert_eq!(out, "01-02,01-03,01-04,endRec=[]");
}

/// Verifies a weekly interval (`P1W` = 7 days) advances by whole weeks.
#[test]
fn test_date_period_weekly() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(new DateTime("2024-03-01"), new DateInterval("P1W"), new DateTime("2024-03-29"));
$out = "";
foreach ($p as $dt) {
    $out .= $dt->format("Y-m-d") . ",";
}
echo $out;
"#,
    );
    assert_eq!(out, "2024-03-01,2024-03-08,2024-03-15,2024-03-22,");
}

/// Verifies the `DatePeriod` getters return the start, end, and interval that were supplied.
#[test]
fn test_date_period_getters() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1M"), new DateTime("2024-04-01"));
echo $p->getStartDate()->format("Y-m-d") . "|"
    . $p->getEndDate()->format("Y-m-d") . "|"
    . $p->getDateInterval()->m;
"#,
    );
    assert_eq!(out, "2024-01-01|2024-04-01|1");
}

/// Verifies each yielded value is a distinct snapshot: collecting them and formatting after the
/// loop preserves the per-step dates rather than all showing the final cursor.
#[test]
fn test_date_period_yields_distinct_snapshots() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1M"), new DateTime("2024-04-01"));
$collected = [];
foreach ($p as $dt) {
    $collected[] = $dt;
}
echo count($collected) . ":"
    . $collected[0]->format("Y-m-d") . ","
    . $collected[1]->format("Y-m-d") . ","
    . $collected[2]->format("Y-m-d");
"#,
    );
    assert_eq!(out, "3:2024-01-01,2024-02-01,2024-03-01");
}

/// Verifies `DateTime::modify()` applies a relative modifier in place against the object's
/// current time. The wall clock is fixed via setDate/setTime, and January dates avoid DST, so
/// the result is timezone-independent.
#[test]
fn test_datetime_modify_relative() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setDate(2024, 1, 15);
$d->setTime(10, 0, 0);
$d->modify("+1 day");
$out = $d->format("Y-m-d H:i:s");
$d->modify("-2 weeks");
$out .= "|" . $d->format("Y-m-d");
$d->modify("+1 month");
$out .= "|" . $d->format("Y-m-d");
echo $out;
"#,
    );
    assert_eq!(out, "2024-01-16 10:00:00|2024-01-02|2024-02-02");
}

/// Verifies a time-only `modify("23:45")` resets the clock while keeping the calendar date.
#[test]
fn test_datetime_modify_time_only() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime();
$d->setDate(2024, 6, 15);
$d->setTime(8, 30, 0);
$d->modify("23:45");
echo $d->format("Y-m-d H:i:s");
"#,
    );
    assert_eq!(out, "2024-06-15 23:45:00");
}

/// Reproduces GH-9891: an epoch-literal modifier replaces the timezone
/// representation with php-src's fixed `+00:00`, for mutable and immutable dates.
#[test]
fn test_datetime_modify_epoch_literal_resets_timezone() {
    let out = compile_and_run(
        r#"<?php
$mutable = new DateTime("2022-11-01 13:30:00", new DateTimeZone("America/Lima"));
$mutable->modify("@" . $mutable->getTimestamp());
$base = new DateTimeImmutable("2022-11-01 13:30:00", new DateTimeZone("America/Lima"));
$immutable = $base->modify("@" . $base->getTimestamp());
echo $mutable->format(DateTime::ATOM), "|", $immutable->format(DateTime::ATOM), "|",
     $base->format(DateTime::ATOM);
"#,
    );
    assert_eq!(
        out,
        "2022-11-01T18:30:00+00:00|2022-11-01T18:30:00+00:00|2022-11-01T13:30:00-05:00"
    );
}

/// Verifies `DateTimeImmutable::modify()` returns a new instance and leaves the receiver
/// unchanged (so the original and the modified value differ).
#[test]
fn test_datetime_immutable_modify_returns_new() {
    let out = compile_and_run(
        r#"<?php
$base = (new DateTimeImmutable())->setDate(2024, 1, 15)->setTime(0, 0, 0);
$later = $base->modify("+3 days");
echo $base->format("Y-m-d"), "|", $later->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2024-01-15|2024-01-18");
}

/// Verifies `modify()` accepts the `first/last day of …` and `first/last <weekday> of …` phrases
/// (forwarded to strtotime). All examples are pinned in January or February to avoid DST drift.
#[test]
fn test_datetime_modify_first_last_day_of() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime("2024-01-15 10:00:00");
$d->modify("first day of next month");
$out = $d->format("Y-m-d");

$d = new DateTime("2024-01-15 10:00:00");
$d->modify("last day of this month");
$out .= "|" . $d->format("Y-m-d");

$d = new DateTime("2024-01-15 10:00:00");
$d->modify("first monday of next month");
$out .= "|" . $d->format("Y-m-d");

$d = new DateTime("2024-01-31 10:00:00");
$d->modify("last friday of this month");
$out .= "|" . $d->format("Y-m-d");

echo $out;
"#,
    );
    assert_eq!(out, "2024-02-01|2024-01-31|2024-02-05|2024-01-26");
}

/// Verifies `DateTime::format()` renders the stored instant in the zone set via `setTimezone()`:
/// an absolute epoch shown in Europe/Paris is the CEST wall clock (UTC+2 in summer).
#[test]
fn test_datetime_format_honors_set_timezone() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime("@1719835200");
$d->setTimezone(new DateTimeZone("Europe/Paris"));
echo $d->format("Y-m-d H:i");
"#,
    );
    assert_eq!(out, "2024-07-01 14:00");
}

/// Verifies a new `DateTime` adopts the configured default timezone for both construction and
/// formatting: with Europe/Paris set, a local-time string round-trips to the same wall clock.
#[test]
fn test_datetime_construct_uses_configured_default_zone() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
$e = new DateTime("2024-07-01 12:00:00");
echo $e->format("H:i"), "|", $e->getTimezone()->getName();
"#,
    );
    assert_eq!(out, "12:00|Europe/Paris");
}

/// Verifies `setTimezone()` changes only the display zone, not the absolute instant: the same
/// epoch reads as 12:00 UTC then 08:00 in New York (EDT), and getTimestamp() is unchanged.
#[test]
fn test_datetime_set_timezone_shifts_display_keeps_instant() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime("@1719835200");
$before = $d->format("H:i");
$d->setTimezone(new DateTimeZone("America/New_York"));
echo $before, "|", $d->format("H:i"), "|", $d->getTimestamp();
"#,
    );
    assert_eq!(out, "12:00|08:00|1719835200");
}

/// Verifies `DateTimeImmutable` carries its per-object zone through a modifier: a Paris-zoned
/// instant formats as CEST, and the +1h derived instance stays in the same zone.
#[test]
fn test_datetime_immutable_format_honors_timezone() {
    let out = compile_and_run(
        r#"<?php
$im = (new DateTimeImmutable("@1719835200"))->setTimezone(new DateTimeZone("Europe/Paris"));
echo $im->format("H:i"), "|", $im->add(new DateInterval("PT1H"))->format("H:i");
"#,
    );
    assert_eq!(out, "14:00|15:00");
}

/// Verifies `DateTimeZone::getOffset()` returns the zone's UTC offset in seconds for a given instant,
/// daylight-saving aware: Europe/Paris is +7200 in summer and +3600 in winter, New York -14400 (EDT).
#[test]
fn test_datetimezone_get_offset() {
    let out = compile_and_run(
        r#"<?php
$paris = new DateTimeZone("Europe/Paris");
$ny = new DateTimeZone("America/New_York");
$summer = new DateTime("@1719835200");
$winter = new DateTime("@1704110400");
echo $paris->getOffset($summer), "|", $ny->getOffset($summer), "|", $paris->getOffset($winter);
"#,
    );
    assert_eq!(out, "7200|-14400|3600");
}

/// Verifies `DateTime::getOffset()` returns the object's own UTC offset (seconds) for its instant,
/// daylight-saving aware: UTC is 0, Europe/Paris +7200 (CEST), New York -14400 (EDT).
#[test]
fn test_datetime_get_offset() {
    let out = compile_and_run(
        r#"<?php
$d = new DateTime("@1719835200");
$utc = $d->getOffset();
$d->setTimezone(new DateTimeZone("Europe/Paris"));
$paris = $d->getOffset();
$d->setTimezone(new DateTimeZone("America/New_York"));
echo $utc, "|", $paris, "|", $d->getOffset();
"#,
    );
    assert_eq!(out, "0|7200|-14400");
}

/// Verifies the procedural date aliases desugar to the OOP API: date_create/date_format/
/// date_timezone_set/timezone_open/timezone_name_get/date_timezone_get/date_offset_get/date_diff.
#[test]
fn test_procedural_date_aliases() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = date_create("@1719835200");
$r = date_format($d, "Y-m-d H:i");
date_timezone_set($d, timezone_open("Europe/Paris"));
echo $r, "|", date_format($d, "H:i"), "|", timezone_name_get(date_timezone_get($d)), "|",
     date_offset_get($d), "|",
     date_diff(new DateTime("@1704067200"), new DateTime("@1719835200"))->days;
"#,
    );
    assert_eq!(out, "2024-07-01 12:00|14:00|Europe/Paris|7200|182");
}

/// Verifies the mutating procedural aliases desugar to the OOP API: date_date_set/date_time_set/
/// date_add/date_sub plus date_interval_format.
#[test]
fn test_procedural_date_mutation_aliases() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = new DateTime();
date_date_set($d, 2024, 1, 15);
date_time_set($d, 9, 30, 0);
date_add($d, new DateInterval("P1M"));
date_sub($d, new DateInterval("P3D"));
echo date_format($d, "Y-m-d H:i:s"), "|", date_interval_format(new DateInterval("P1Y2M3D"), "%y-%m-%d");
"#,
    );
    assert_eq!(out, "2024-02-12 09:30:00|1-2-3");
}

/// Verifies `DateTime::createFromFormat` parses a full date/time string per the format and that the
/// Verifies the two-argument constructor `new DateTime($time, $tz)`: the wall-clock string is
/// interpreted in the given zone (so the stored instant is offset accordingly) and that zone
/// becomes the display zone, for both `DateTime` and `DateTimeImmutable`. The one-argument form
/// still uses the default timezone.
#[test]
fn test_datetime_constructor_with_timezone() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = new DateTime("2024-06-15 12:00:00", new DateTimeZone("Europe/Paris"));
$im = new DateTimeImmutable("2024-01-15 08:00:00", new DateTimeZone("America/New_York"));
$plain = new DateTime("2024-03-01 10:00:00");
echo $d->format("H:i"), " ", $d->getTimezone()->getName(), " ", $d->getTimestamp(), "|",
     $im->format("H:i"), " ", $im->getTimezone()->getName(), "|",
     $plain->format("H:i"), " ", $plain->getTimezone()->getName();
"#,
    );
    assert_eq!(
        out,
        "12:00 Europe/Paris 1718445600|08:00 America/New_York|10:00 UTC"
    );
}

/// Regression: the two-argument constructor must work when the program never calls
/// `DateTimeZone::getName()` explicitly. The constructor invokes `$timezone->getName()`
/// internally on its `?DateTimeZone` parameter, whose codegen repr is `Mixed`; the demand-lowering
/// reference scan previously inspected only the collapsed repr, so `getName()` was never emitted
/// and the internal call dispatched to a missing symbol (SIGSEGV). Constructing with an explicit
/// timezone and reading only the timestamp/format must not crash. Uses `UTC` so the assertion is
/// portable (named IANA zones resolve differently under the Alpine tzdata used by the Linux CI
/// images); the demand-lowering path it exercises is identical for any explicit zone.
#[test]
fn test_datetime_constructor_timezone_internal_getname_emitted() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = new DateTime("2024-07-01 14:30:00", new DateTimeZone("UTC"));
$i = new DateTimeImmutable("2024-07-01 14:30:00", new DateTimeZone("UTC"));
echo $d->getTimestamp(), "|", $d->format("H:i:sP"), "|", $i->getTimestamp();
"#,
    );
    assert_eq!(out, "1719844200|14:30:00+00:00|1719844200");
}

/// Verifies timezone-only type-1/2 strings preserve the default zone's wall
/// clock, type-3 identifiers preserve the current instant, and every spelling
/// retains php-src's timezone discriminator after changing the timestamp.
#[test]
fn test_datetime_constructor_timezone_only_string() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/London");
$before = time();
$offset = intval(date("Z", $before));
$abbr = new DateTime("GMT");
$named = new DateTime("UTC");
echo abs($abbr->getTimestamp() - ($before + $offset)) <= 1 ? "abbr-wall|" : "bad-abbr|";
echo abs($named->getTimestamp() - $before) <= 1 ? "named-instant|" : "bad-named|";
foreach (["GMT", "CET", "UTC", "Europe/Paris"] as $source) {
    $date = new DateTime($source);
    $date->setTimestamp(0);
    $serialized = $date->__serialize();
    echo $source, ":", $serialized["timezone_type"], ":", $serialized["timezone"], ":",
         $date->format("Y-m-d H:i:s e T"), "|";
}
"#,
    );
    assert_eq!(
        out,
        "abbr-wall|named-instant|\
GMT:2:GMT:1970-01-01 00:00:00 GMT GMT|\
CET:2:CET:1970-01-01 01:00:00 CET CET|\
UTC:3:UTC:1970-01-01 00:00:00 UTC UTC|\
Europe/Paris:3:Europe/Paris:1970-01-01 01:00:00 Europe/Paris CET|"
    );
}

/// Verifies the cross-conversion factories preserve the source instant and display timezone
/// while switching mutability: `createFromInterface`/`createFromImmutable` build a `DateTime`,
/// `createFromMutable`/`createFromInterface` build a `DateTimeImmutable`.
#[test]
fn test_datetime_create_from_object_conversions() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$src = new DateTime("2024-06-15 12:00:00");
$src->setMicrosecond(123456);
$src->setTimezone(new DateTimeZone("Europe/Paris"));
$im = DateTimeImmutable::createFromMutable($src);
$back = DateTime::createFromInterface($im);
$plain = new DateTimeImmutable("2024-03-10 08:30:00");
$dt = DateTime::createFromImmutable($plain);
echo $im->format("Y-m-d H:i"), " ", $im->getTimezone()->getName(), "|",
     $src->getMicrosecond(), ":", $im->getMicrosecond(), ":", $back->format("H:i.u"), "|",
     $dt->format("Y-m-d H:i:s");
"#,
    );
    assert_eq!(
        out,
        "2024-06-15 14:00 Europe/Paris|123456:123456:14:00.123456|2024-03-10 08:30:00"
    );
}

/// Verifies php-src's late-static allocation for cross-conversion and formatted factories:
/// inherited calls allocate the invoked subclass without running its constructor and preserve
/// date state.
#[test]
fn test_datetime_create_from_object_factories_preserve_called_subclass() {
    let out = compile_and_run(
        r#"<?php
class ConvertedMutable extends DateTime {}
class ConvertedImmutable extends DateTimeImmutable {}
$mutable = ConvertedMutable::createFromInterface(
    new DateTimeImmutable("2024-03-02T16:24:08 Europe/London")
);
$immutable = ConvertedImmutable::createFromMutable($mutable);
$formattedMutable = ConvertedMutable::createFromFormat(
    "!Y-m-d H:i:s.u",
    "2011-01-01 01:23:45.123456",
    new DateTimeZone("UTC")
);
$formattedImmutable = ConvertedImmutable::createFromFormat(
    "!Y-m-d H:i:s.u",
    "2011-01-01 01:23:45.123456",
    new DateTimeZone("UTC")
);
echo get_class($mutable), ":", $mutable->format("Y-m-d H:i:s e"), "|",
     get_class($immutable), ":", $immutable->format("Y-m-d H:i:s e"), "|",
     get_class($formattedMutable), ":", $formattedMutable->format("Y-m-d H:i:s.u e"), "|",
     get_class($formattedImmutable), ":", $formattedImmutable->format("Y-m-d H:i:s.u e");
"#,
    );
    assert_eq!(
        out,
        "ConvertedMutable:2024-03-02 16:24:08 Europe/London|\
ConvertedImmutable:2024-03-02 16:24:08 Europe/London|\
ConvertedMutable:2011-01-01 01:23:45.123456 UTC|\
ConvertedImmutable:2011-01-01 01:23:45.123456 UTC"
    );
}

/// Verifies the narrow mutable/immutable factories reject the opposite DateTime family at runtime.
#[test]
fn test_datetime_create_from_object_factories_validate_source_family() {
    let out = compile_and_run(
        r#"<?php
try {
    DateTimeImmutable::createFromMutable(new DateTimeImmutable());
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    DateTime::createFromImmutable(new DateTime());
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "DateTimeImmutable::createFromMutable(): Argument #1 ($object) must be of type DateTime, DateTimeImmutable given\n\
DateTime::createFromImmutable(): Argument #1 ($object) must be of type DateTimeImmutable, DateTime given\n"
    );
}

/// Verifies php-src's timezone-offset range check uses the normalized total rather than rejecting
/// minute overflow that still produces an offset below the 100-hour limit.
#[test]
fn test_datetime_timezone_offset_range_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
foreach (["+01:99", "-01:00:99", "+99:59", "+99:60", "+9960"] as $timezone) {
    echo $timezone, ":";
    try {
        echo (new DateTimeZone($timezone))->getName(), "|";
    } catch (Throwable $e) {
        echo get_class($e), ":", $e->getMessage(), "|";
    }
}
"#,
    );
    assert_eq!(
        out,
        "+01:99:+02:39|-01:00:99:-01:01:39|+99:59:+99:59|\
+99:60:DateInvalidTimeZoneException:DateTimeZone::__construct(): Timezone offset is out of range (+99:60)|\
+9960:DateInvalidTimeZoneException:DateTimeZone::__construct(): Timezone offset is out of range (+9960)|"
    );
}

/// Verifies `DatePeriod::createFromISO8601String()` honors late-static allocation and restores
/// the parsed period into the invoked subclass without running a user constructor.
#[test]
fn test_dateperiod_iso_factory_preserves_called_subclass() {
    let out = compile_and_run(
        r#"<?php
class ParsedPeriod extends DatePeriod {}
$period = ParsedPeriod::createFromISO8601String("R4/2012-07-01T00:00:00Z/P7D");
echo get_class($period), ":", $period->recurrences, ":",
     $period->start->format("Y-m-d"), ":", $period->interval->format("%d");
"#,
    );
    assert_eq!(out, "ParsedPeriod:5:2012-07-01:7");
}

/// Verifies DatePeriod's private backing clones consume no PHP handles and remain GC-owned.
#[test]
fn test_dateperiod_iso_factory_handleless_storage_matches_php_identity() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class ParsedHandlePeriod extends DatePeriod {}
$period = ParsedHandlePeriod::createFromISO8601String("R4/2012-07-01T00:00:00Z/P7D");
$startOne = $period->getStartDate();
$intervalOne = $period->getDateInterval();
$startTwo = $period->getStartDate();
$intervalTwo = $period->getDateInterval();
echo spl_object_id($period), ",", spl_object_id($startOne), ",",
     spl_object_id($intervalOne), ",", spl_object_id($startTwo), ",",
     spl_object_id($intervalTwo), ":",
     (int) ($startOne !== $startTwo), (int) ($intervalOne !== $intervalTwo);
$left = new stdClass();
$right = new stdClass();
echo ":", (int) (spl_object_id($left) !== spl_object_id($right));
unset($right, $left, $intervalTwo, $startTwo, $intervalOne, $startOne, $period);
"#,
    );
    assert_eq!(output.stdout, "1,2,3,4,5:11:1");
    let summary = output
        .stderr
        .lines()
        .find(|line| line.starts_with("HEAP DEBUG: leak summary:"))
        .unwrap_or_else(|| panic!("missing heap-debug summary: {}", output.stderr));
    let live_blocks = if summary.ends_with("clean") {
        0
    } else {
        summary
            .split("live_blocks=")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| panic!("invalid heap-debug summary: {summary}"))
    };
    assert!(
        live_blocks <= 16,
        "handleless DatePeriod backing retained {live_blocks} blocks after release: {}",
        output.stderr
    );
}

/// Verifies both direct and by-reference writes to DatePeriod's virtual properties raise the
/// catchable readonly-property `Error` required by php-src.
#[test]
fn test_dateperiod_virtual_properties_reject_direct_and_reference_writes() {
    let out = compile_and_run(
        r#"<?php
$period = new DatePeriod(
    new DateTimeImmutable("2023-01-13"),
    DateInterval::createFromDateString("+1 month"),
    new DateTimeImmutable("2023-12-31"),
);
try {
    $period->interval = "invalid";
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
try {
    $alias =& $period->interval;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "Cannot modify readonly property DatePeriod::$interval\n\
Cannot modify readonly property DatePeriod::$interval\n"
    );
}

/// Verifies `setISODate()` maps an ISO 8601 week date to the Gregorian date while keeping the
/// time-of-day: week 1 day 1 is the Monday of the week containing Jan 4, week 53 of a 52-week
/// year overflows into the next year, and ISO year 2026 week 1 begins in December 2025. The
/// `date_isodate_set()` procedural alias and the immutable (returns-new) variant are covered too.
#[test]
fn test_datetime_set_isodate() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = new DateTime("2024-01-01 09:30:15");
$d->setMicrosecond(123456);
$d->setISODate(2024, 10, 3);
$im = new DateTimeImmutable("2020-06-15 12:00:00");
$im = $im->setMicrosecond(654321);
$im2 = $im->setISODate(2026, 1, 1);
$e = new DateTime("2024-06-15 00:00:00");
date_isodate_set($e, 2024, 53, 1);
echo $d->format("Y-m-d H:i:s.u"), "|", $im2->format("Y-m-d.u"), "|", $im->format("Y-m-d"), "|",
     $e->format("Y-m-d");
"#,
    );
    assert_eq!(
        out,
        "2024-03-06 09:30:15.123456|2025-12-29.654321|2020-06-15|2024-12-30"
    );
}

/// resulting object formats back identically.
#[test]
fn test_create_from_format_basic() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = DateTime::createFromFormat("Y-m-d H:i:s", "2024-03-15 14:30:45");
echo $d->format("Y-m-d H:i:s");
"#,
    );
    assert_eq!(out, "2024-03-15 14:30:45");
}

/// Verifies format parsers reject embedded NUL bytes before timelib parsing and
/// report the public entry point plus the correct argument name and position.
#[test]
fn test_create_and_parse_from_format_reject_embedded_null_bytes() {
    let out = compile_and_run(
        r#"<?php
$datetime = "8/8/2016" . chr(0) . "tail";
try { DateTime::createFromFormat("!m/d/Y", $datetime); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { DateTimeImmutable::createFromFormat("!m/d/Y", $datetime); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { date_parse_from_format("m/d/Y", $datetime); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "DateTime::createFromFormat(): Argument #2 ($datetime) must not contain any null bytes\n\
DateTimeImmutable::createFromFormat(): Argument #2 ($datetime) must not contain any null bytes\n\
date_parse_from_format(): Argument #2 ($datetime) must not contain any null bytes\n"
    );
}

/// Verifies `DateTimeImmutable::createFromFormat` builds an immutable instance and that `!` resets
/// the unspecified time fields to the Unix epoch (00:00:00).
#[test]
fn test_create_from_format_immutable_epoch_reset() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$im = DateTimeImmutable::createFromFormat("!Y-m-d", "2020-06-15");
echo $im->format("Y-m-d H:i:s");
"#,
    );
    assert_eq!(out, "2020-06-15 00:00:00");
}

/// Verifies a range of format specifiers: two-digit year `y`, no-leading-zero `n`/`j`, the `U`
/// timestamp specifier, 12-hour `h` with am/pm `A`, and a literal `/` separator.
#[test]
fn test_create_from_format_specifiers() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = DateTime::createFromFormat("!y-n-j", "99-3-5");
$b = DateTime::createFromFormat("U", "1000000000");
$c = DateTime::createFromFormat("!h:i A", "12:00 PM");
$f = DateTime::createFromFormat("!d/m/Y", "15/03/2024");
echo $a->format("Y-m-d"), "|", $b->format("Y-m-d H:i:s"), "|", $c->format("H:i"), "|", $f->format("Y-m-d");
"#,
    );
    assert_eq!(out, "1999-03-05|2001-09-09 01:46:40|12:00|2024-03-15");
}

/// Verifies `createFromFormat` returns `false` when the subject does not match the format, and that
/// the `=== false` check works on the result.
#[test]
fn test_create_from_format_mismatch_returns_false() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$bad = DateTime::createFromFormat("Y-m-d", "not-a-date");
echo ($bad === false) ? "false" : "??";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies the timezone format specifiers `O` (`+hhmm`), `P` (`+hh:mm`), `Z` (offset in seconds),
/// `T` (greedy abbreviation), and `e` (IANA name) select the parsed timezone and override the
/// optional third argument. `Z` is a literal rather than a supported parser token.
#[test]
fn test_create_from_format_tz_specifiers() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
// Paris 2024-07-15 12:00:00 = UTC 10:00, offset +02:00.
$paris = new DateTimeZone("Europe/Paris");
$ts = (new DateTime("2024-07-15 12:00:00", $paris))->getTimestamp();

// O/P embedded offsets override the optional Paris timezone; different offsets remain valid.
$a = DateTime::createFromFormat("Y-m-d H:i:s O", "2024-07-15 12:00:00 +0200", $paris);
echo ($a === false) ? "?" : "O:" . $a->format("Y-m-d H:i:s O") . "|";
$otherO = DateTime::createFromFormat("Y-m-d H:i:s O", "2024-07-15 12:00:00 +0500", $paris);
echo ($otherO === false) ? "?" : "O5:" . $otherO->format("Y-m-d H:i:s O") . "|";

$b = DateTime::createFromFormat("Y-m-d H:i:s P", "2024-07-15 12:00:00 +02:00", $paris);
echo ($b === false) ? "?" : "P:" . $b->format("Y-m-d H:i:s P") . "|";
$otherP = DateTime::createFromFormat("Y-m-d H:i:s P", "2024-07-15 12:00:00 -05:00", $paris);
echo ($otherP === false) ? "?" : "P5:" . $otherP->format("Y-m-d H:i:s P") . "|";

// Z is not a createFromFormat token in php-src.
$c = DateTime::createFromFormat("Y-m-d H:i:s Z", "2024-07-15 12:00:00 +7200", $paris);
echo ($c === false) ? "Z-false|" : "?";

// T: 3- or 4-letter abbreviation. libc resolves this to "CEST" for Paris summer.
$e = DateTime::createFromFormat("Y-m-d H:i:s T", "2024-07-15 12:00:00 CEST", $paris);
echo ($e === false) ? "T-false|" : "T:" . $e->format("Y-m-d H:i:s T") . "|";

// e: IANA name. Round-trip with the same zone.
$f = DateTime::createFromFormat("Y-m-d H:i:s e", "2024-07-15 12:00:00 Europe/Paris", $paris);
echo ($f === false) ? "?" : "e:" . $f->format("Y-m-d H:i:s e");
"#,
    );
    assert_eq!(
        out,
        "O:2024-07-15 12:00:00 +0200|O5:2024-07-15 12:00:00 +0500|P:2024-07-15 12:00:00 +02:00|P5:2024-07-15 12:00:00 -05:00|Z-false|T:2024-07-15 12:00:00 CEST|e:2024-07-15 12:00:00 Europe/Paris"
    );
}

/// Verifies `createFromFormat`'s optional third `DateTimeZone` argument interprets the parsed
/// wall-clock in that zone and sets it as the display zone when the zone is passed via a variable.
#[test]
fn test_create_from_format_timezone_arg_mutable() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$ny = new DateTimeZone("America/New_York");
$d = DateTime::createFromFormat("Y-m-d H:i:s", "2024-06-15 12:00:00", $ny);
echo $d->getTimestamp(), "|", gmdate("H:i", $d->getTimestamp()), "|", $d->format("H:i");
"#,
    );
    // 12:00 in New York is EDT/UTC-4 in June = 16:00 UTC; display in NY = 12:00.
    assert_eq!(out, "1718467200|16:00|12:00");
}

/// Reproduces bug #81565: fixed timezone offsets with seconds retain the full
/// spelling and format the parsed wall clock without losing the seconds remainder.
#[test]
fn test_create_from_format_timezone_offset_with_seconds() {
    let out = compile_and_run(
        r#"<?php
$date = DateTime::createFromFormat(
    "Y-m-d H:i:sO",
    "0021-08-21 00:00:00+00:49:56"
);
$state = $date->__serialize();
echo $state["date"], "|", $state["timezone_type"], "|", $state["timezone"], "|",
     (new DateTimeZone("+01:45:30"))->getName();
"#,
    );
    assert_eq!(
        out,
        "0021-08-21 00:00:00.000000|1|+00:49:56|+01:45:30"
    );
}

/// Verifies `date_create_immutable_from_format` desugars to the immutable factory with the same
/// optional timezone argument handling as `DateTimeImmutable::createFromFormat`.
#[test]
fn test_create_from_format_timezone_arg_immutable_function() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$paris = new DateTimeZone("Europe/Paris");
$i = date_create_immutable_from_format("Y-m-d H:i:s", "2024-06-15 12:00:00", $paris);
echo $i->getTimestamp();
"#,
    );
    // Paris 12:00 CEST = 10:00 UTC = 1718445600.
    assert_eq!(out, "1718445600");
}

/// Verifies that an inline `new DateTimeZone(...)` expression works as the third
/// `createFromFormat` argument (and as the second `DateTime`/`DateTimeImmutable` constructor
/// argument) — not just the variable form. This was previously flagged as a known miscompile
/// in the docs; the CFF callee-ownership fix (narrow class-aware `borrowed_alias_for_type`
/// for `DateTime`/`DateTimeImmutable`) closed the gap. The 12:00 Paris instant
/// (CEST = UTC+2) is 10:00 UTC = 1718445600.
#[test]
fn test_create_from_format_inline_tz_arg() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
// CFF (static|false) with inline tz
$d = DateTime::createFromFormat("Y-m-d H:i:s", "2024-06-15 12:00:00", new DateTimeZone("Europe/Paris"));
echo $d === false ? "false" : $d->getTimestamp();
// DateTime ctor with inline tz
$d2 = new DateTime("2024-06-15 12:00:00", new DateTimeZone("Europe/Paris"));
echo "|", $d2->getTimestamp();
// DateTimeImmutable ctor with inline tz
$d3 = new DateTimeImmutable("2024-06-15 12:00:00", new DateTimeZone("Europe/Paris"));
echo "|", $d3->getTimestamp();
"#,
    );
    assert_eq!(out, "1718445600|1718445600|1718445600");
}

/// Verifies the PHP 8.4 static factory `createFromTimestamp()` builds an instance set to the given
/// UNIX timestamp, on both the mutable and immutable classes (the fraction would be dropped — elephc
/// keeps second resolution).
#[test]
fn test_create_from_timestamp() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/London");
echo DateTime::createFromTimestamp(1718452800)->format("Y-m-d H:i:s"), "|",
     DateTimeImmutable::createFromTimestamp(1718452800)->getTimestamp(), "|",
     DateTime::createFromTimestamp(0)->format("Y-m-d|e|T|P");
"#,
    );
    assert_eq!(
        out,
        "2024-06-15 12:00:00|1718452800|1970-01-01|+00:00|GMT+0000|+00:00"
    );
}

/// Verifies temporary DateTime factory results release their PHP object handles for reuse.
#[test]
fn test_datetime_factories_reuse_released_temporary_handles() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set('UTC');
$mutable = DateTime::createFromTimestamp(0);
$mutableId = spl_object_id($mutable);
unset($mutable);
$mutable = DateTime::createFromTimestamp(1);
echo (int) ($mutableId === spl_object_id($mutable));
unset($mutable);

$immutable = DateTimeImmutable::createFromTimestamp(0);
$immutableId = spl_object_id($immutable);
unset($immutable);
$immutable = DateTimeImmutable::createFromTimestamp(1);
echo (int) ($immutableId === spl_object_id($immutable));
unset($immutable);

$source = new DateTimeImmutable('2000-01-01', new DateTimeZone('UTC'));
$mutable = DateTime::createFromInterface($source);
$mutableId = spl_object_id($mutable);
unset($mutable);
$mutable = DateTime::createFromInterface($source);
echo (int) ($mutableId === spl_object_id($mutable));
unset($mutable);

$source = new DateTime('2000-01-01', new DateTimeZone('UTC'));
$immutable = DateTimeImmutable::createFromInterface($source);
$immutableId = spl_object_id($immutable);
unset($immutable);
$immutable = DateTimeImmutable::createFromInterface($source);
echo (int) ($immutableId === spl_object_id($immutable));
"#,
    );
    assert_eq!(out, "1111");
}

/// Verifies sub-second support: set/getMicrosecond, `format('u')`/`format('v')` reflecting the stored
/// microseconds (escaped `\u` stays literal), PHP's reset on mutable `setTimestamp` and preservation
/// across an
/// immutable operation chain, the `createFromFormat('u')` specifier, and `DateInterval->f` (always
/// 0.0 at second resolution).
#[test]
fn test_datetime_microseconds() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = new DateTime("2024-06-15 12:00:00");
$d->setMicrosecond(123456);
echo $d->getMicrosecond(), "|", $d->format("H:i:s.u"), "|", $d->format("H:i:s.v"), "|",
     $d->format('H:i:s \u');
$d->setTimestamp(0);
echo "|", $d->getMicrosecond();
$im = (new DateTimeImmutable("2024-01-01 00:00:00"))->setMicrosecond(7)->setDate(2025, 3, 4);
echo "|", $im->format("Y-m-d.u");
$p = DateTime::createFromFormat("Y-m-d H:i:s.u", "2024-06-15 12:00:00.654321");
echo "|", $p->getMicrosecond();
$iv = new DateInterval("PT1H");
echo "|", $iv->f;
"#,
    );
    assert_eq!(
        out,
        "123456|12:00:00.123456|12:00:00.123|12:00:00 u|0|2025-03-04.000007|654321|0"
    );
}

/// Verifies `setMicrosecond()` preserves late-static class identity and rejects values outside the
/// php-src range without mutating the receiver.
#[test]
fn test_datetime_set_microsecond_range_and_subclass_identity() {
    let out = compile_and_run(
        r#"<?php
class ChildDateTimeImmutable extends DateTimeImmutable {}
$immutable = new ChildDateTimeImmutable("2024-01-01 00:00:00");
$updated = $immutable->setMicrosecond(654321);
echo get_class($updated), "|", $updated->format("u"), "|", $immutable->format("u"), "\n";
$mutable = new DateTime("2024-01-01 00:00:00");
foreach ([-1, 1000000] as $value) {
    try {
        $mutable->setMicrosecond($value);
    } catch (DateRangeError $error) {
        echo $error->getMessage(), "\n";
    }
}
echo $mutable->format("u");
"#,
    );
    assert_eq!(
        out,
        "ChildDateTimeImmutable|654321|000000\n\
DateTime::setMicrosecond(): Argument #1 ($microsecond) must be between 0 and 999999, -1 given\n\
DateTime::setMicrosecond(): Argument #1 ($microsecond) must be between 0 and 999999, 1000000 given\n\
000000"
    );
}

/// Verifies `getLastErrors()` / `date_get_last_errors()` expose timelib's complete diagnostics for
/// the most recent `createFromFormat()` call, including duplicate-position error counts and sparse
/// numeric message keys. Also exercises shared synthetic-class static-property storage.
#[test]
fn test_datetime_get_last_errors() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
DateTime::createFromFormat("Y-m-d", "2024-06-15");
$ok = DateTime::getLastErrors();
DateTime::createFromFormat("Y-m-d", "not-a-date");
$bad = DateTime::getLastErrors();
$alias = date_get_last_errors();
echo ($ok === false) ? "false" : "other", "|",
     $bad["error_count"], "|", count($bad["errors"]), "|", $alias["error_count"];
"#,
    );
    assert_eq!(out, "false|3|2|3");
}

/// Verifies the procedural `date_create_from_format` alias desugars to `DateTime::createFromFormat`,
/// including the `false`-on-mismatch result.
#[test]
fn test_create_from_format_procedural_alias() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = date_create_from_format("Y-m-d H:i:s", "2024-03-15 14:30:45");
echo $d->format("Y-m-d H:i:s"), "|",
     (date_create_from_format("Y-m-d", "bad") === false ? "false" : "x");
"#,
    );
    assert_eq!(out, "2024-03-15 14:30:45|false");
}

/// Verifies `date_parse_from_format` returns the PHP component array with parsed fields as integers.
#[test]
fn test_date_parse_from_format_components() {
    let out = compile_and_run(
        r#"<?php
$r = date_parse_from_format("Y-m-d H:i:s", "2024-03-15 14:30:45");
echo $r["year"], "-", $r["month"], "-", $r["day"], " ",
     $r["hour"], ":", $r["minute"], ":", $r["second"], "|", $r["error_count"];
"#,
    );
    assert_eq!(out, "2024-3-15 14:30:45|0");
}

/// Verifies `date_parse_from_format` leaves unparsed fields as `false`, but a parsed time field
/// resets the unparsed lower time fields to `0` (matching PHP).
#[test]
fn test_date_parse_from_format_unparsed_fields() {
    let out = compile_and_run(
        r#"<?php
$d = date_parse_from_format("Y-m-d", "2024-03-15");
echo ($d["hour"] === false ? "F" : "v"), ($d["fraction"] === false ? "F" : "v");
$t = date_parse_from_format("H:i", "14:30");
echo "|", ($t["year"] === false ? "F" : "v"), $t["second"];
"#,
    );
    assert_eq!(out, "FF|F0");
}

/// Verifies `date_parse` parses common formats (auto-detected) into the component array, leaving
/// unparsed fields `false`.
#[test]
fn test_date_parse_common_formats() {
    let out = compile_and_run(
        r#"<?php
$a = date_parse("2024-03-15 14:30:45");
echo $a["year"], "-", $a["month"], "-", $a["day"], " ",
     $a["hour"], ":", $a["minute"], ":", $a["second"];
$b = date_parse("2024-03-15");
echo "|", ($b["hour"] === false ? "F" : "v"), "|", $a["error_count"];
"#,
    );
    assert_eq!(out, "2024-3-15 14:30:45|F|0");
}

/// Verifies `DateTimeZone::listIdentifiers()` (and the `timezone_identifiers_list()` alias) return
/// the embedded IANA identifier list as a usable array (count, indexing, and `in_array`).
#[test]
fn test_timezone_list_identifiers() {
    let out = compile_and_run(
        r#"<?php
$z = DateTimeZone::listIdentifiers();
echo count($z), "|", $z[0], "|", (in_array("Europe/Paris", $z) ? "y" : "n"),
     "|", count(timezone_identifiers_list());
"#,
    );
    assert_eq!(out, "419|Africa/Abidjan|y|419");
}

/// Verifies `DateTimeZone::listIdentifiers($group)` filters the identifier list by region-group
/// bitmask (and `ALL_WITH_BC` adds the backward-compat zones, combined masks union the regions),
/// keeping the result a usable `array<string>` so `count`/indexing/`in_array` work; the
/// `timezone_identifiers_list()` alias filters identically. Values are byte-exact with PHP 8.5.
#[test]
fn test_timezone_list_identifiers_group_filter() {
    let out = compile_and_run(
        r#"<?php
$eu = DateTimeZone::listIdentifiers(DateTimeZone::EUROPE);
$asia = DateTimeZone::listIdentifiers(DateTimeZone::ASIA);
$bc = DateTimeZone::listIdentifiers(DateTimeZone::ALL_WITH_BC);
$combo = DateTimeZone::listIdentifiers(DateTimeZone::EUROPE | DateTimeZone::ASIA);
$pac = timezone_identifiers_list(DateTimeZone::PACIFIC);
echo count($eu), "|", $eu[0], "|", (in_array("Europe/Istanbul", $eu) ? "y" : "n"),
     "|", count($asia), "|", (in_array("Europe/Istanbul", $asia) ? "y" : "n"),
     "|", count($bc), "|", (in_array("US/Eastern", $bc) ? "y" : "n"),
     "|", count($combo), "|", count($pac), "|", $pac[0];
"#,
    );
    assert_eq!(out, "58|Europe/Amsterdam|y|82|n|598|y|140|38|Pacific/Apia");
}

/// Verifies `DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY, $cc)` filters by ISO 3166-1
/// country code (case-sensitive, like PHP — lowercase `fr` matches nothing), and that PER_COUNTRY
/// without a country throws `ValueError` with PHP's exact message.
#[test]
fn test_timezone_list_identifiers_per_country() {
    let out = compile_and_run(
        r#"<?php
$fr = DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY, "FR");
$us = DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY, "US");
$lower = DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY, "fr");
echo count($fr), "|", $fr[0], "|", count($us), "|", count($lower), "|";
try {
    DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY);
    echo "no-throw";
} catch (ValueError $e) {
    echo "ValueError";
}
"#,
    );
    assert_eq!(out, "1|Europe/Paris|29|0|ValueError");
}

/// Verifies that `DateTime` and `strtotime()` parse dates before 1900 (which libc `mktime` rejects),
/// across ISO, slash, and textual forms, via the 400-year Gregorian-cycle shift.
#[test]
fn test_datetime_pre_1900() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
echo (new DateTime("1850-03-15"))->format("Y-m-d"), "|",
     strtotime("1850-03-15"), "|",
     date("Y-m-d", strtotime("15 March 1850")), "|",
     (new DateTime("1776-07-04 12:30:00"))->format("Y-m-d H:i:s");
"#,
    );
    assert_eq!(out, "1850-03-15|-3780518400|1850-03-15|1776-07-04 12:30:00");
}

/// Verifies `DatePeriod::createFromISO8601String()` parses a subset of RFC 5545
/// (`Rn/start[/interval[/end]]`) and yields the same iteration order as the equivalent
/// `(start, interval, end|recurrences)` constructor. Malformed input throws
/// `DateMalformedPeriodStringException` (PHP 8.3+).
#[test]
fn test_date_period_create_from_iso8601_string() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/London");
// R4 + 7-day interval, no end bound.
$p = DatePeriod::createFromISO8601String("R4/2012-07-01T00:00:00Z/P7D");
$dates = [];
foreach ($p as $d) { $dates[] = $d->format("Y-m-d"); }
$startState = $p->start->__serialize();
echo count($dates), "|", $dates[0], "|", $dates[3], "|",
    $startState["timezone_type"], ":", $startState["timezone"], "|";
// R3 + 1-day interval, no end bound.
$p = DatePeriod::createFromISO8601String("R3/2024-01-01T00:00:00Z/P1D");
$dates = [];
foreach ($p as $d) { $dates[] = $d->format("Y-m-d"); }
echo count($dates), "|", $dates[0], "|", $dates[2], "|";
// R2 with explicit end date.
$p = DatePeriod::createFromISO8601String("R2/2012-07-01T00:00:00Z/P7D/2012-07-29T00:00:00Z");
$dates = [];
foreach ($p as $d) { $dates[] = $d->format("Y-m-d"); }
echo count($dates), "|", $dates[0], "|", $dates[count($dates)-1], "|";
// Malformed inputs throw DateMalformedPeriodStringException (PHP 8.3+): R-1/R/ are
// bad-format errors and R0 is a recurrence-count error, but both are the same class.
$thrown = "";
foreach (["R-1/2012-07-01T00:00:00Z/P7D", "R0/2012-07-01T00:00:00Z/P7D", "R/2012-07-01T00:00:00Z/P7D"] as $spec) {
    try { DatePeriod::createFromISO8601String($spec); $thrown .= "0"; }
    catch (DateMalformedPeriodStringException $e) { $thrown .= "1"; }
}
echo $thrown;
"#,
    );
    assert_eq!(
        out,
        "5|2012-07-01|2012-07-22|1:+00:00|\
4|2024-01-01|2024-01-03|4|2012-07-01|2012-07-22|111"
    );
}

/// Verifies the `DateTimeInterface` format constants (`ATOM`, `RFC2822`, `W3C`, ...) resolve
/// on the interface and both classes, and produce PHP-identical `format()` output.
#[test]
fn test_datetime_format_constants() {
    let out = compile_and_run(
        r#"<?php
echo DateTime::ATOM, "|";
echo DateTimeImmutable::RFC2822, "|";
echo DateTimeInterface::W3C, "|";
echo DateTime::COOKIE, "|";
echo DateTime::RFC3339_EXTENDED, "|";
$d = new DateTime("2024-07-01 14:30:00", new DateTimeZone("Europe/Paris"));
echo $d->format(DateTime::ATOM), "|";
echo $d->format(DateTimeInterface::RFC7231), "|";
echo $d->format(DateTime::RFC822);
"#,
    );
    assert_eq!(
        out,
        "Y-m-d\\TH:i:sP|D, d M Y H:i:s O|Y-m-d\\TH:i:sP|l, d-M-Y H:i:s T|Y-m-d\\TH:i:s.vP|2024-07-01T14:30:00+02:00|Mon, 01 Jul 2024 14:30:00 GMT|Mon, 01 Jul 24 14:30:00 +0200"
    );
}

/// Verifies `DateInterval::format()` renders `%f` (microseconds, no padding) and `%F`
/// (microseconds zero-padded to six digits) from the public `$f` fractional-second float,
/// matching PHP for both the default 0.0 and an assigned fraction.
#[test]
fn test_date_interval_format_microseconds() {
    let out = compile_and_run(
        r#"<?php
$i = new DateInterval("P1Y2M3DT4H5M6S");
echo $i->format("%f|%F"), "|";
$j = new DateInterval("PT1S");
$j->f = 0.006602;
echo $j->format("%f|%F");
"#,
    );
    assert_eq!(out, "0|000000|6602|006602");
}

/// Verifies `timezone_version_get()` reports the bundled IANA release the
/// timezone-introspection data was baked from (matching PHP's timelib version),
/// and that `function_exists()` recognizes the alias.
#[test]
fn test_timezone_version_get() {
    let out = compile_and_run(
        r#"<?php
echo timezone_version_get(), "|", function_exists("timezone_version_get") ? "1" : "0";
"#,
    );
    assert_eq!(out, "2026.3|1");
}

/// Verifies the createFromFormat() specifiers added for full PHP parity: weekday names `D`/`l`
/// (relative forward shift to the named weekday, like timelib), month names `M`/`F` (full,
/// abbreviated, "sept", case-insensitive), 0-based day-of-year `z` (requires a parsed year,
/// overrides month/day, overflows through mktime), milliseconds `v`, ordinal suffix `S`, the
/// separator metas `#` / `?` / `*`, trailing-junk tolerance `+`, and the new strict
/// trailing-data failure without `+`. Every expectation is byte-identical to PHP 8.
#[test]
fn test_create_from_format_extended_specifiers() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
function r($x) { return $x === false ? "FALSE" : $x->format("Y-m-d H:i:s"); }
echo r(DateTime::createFromFormat("!D, d M Y", "Tue, 01 Jul 2024")), "|";
echo r(DateTime::createFromFormat("!l, d M Y", "Sunday, 01 Jul 2024")), "|";
echo r(DateTime::createFromFormat("!D, d M Y", "Xyz, 01 Jul 2024")), "|";
echo r(DateTime::createFromFormat("!d F Y", "15 march 2024")), "|";
echo r(DateTime::createFromFormat("!d F Y", "15 sept 2024")), "|";
echo r(DateTime::createFromFormat("!Y z", "2024 60")), "|";
echo r(DateTime::createFromFormat("!Y z", "2023 365")), "|";
echo r(DateTime::createFromFormat("!z Y", "0 2024")), "|";
$v = DateTime::createFromFormat("!Y-m-d H:i:s.v", "2024-03-15 10:00:00.123");
echo ($v === false) ? "FALSE" : $v->format("u"), "|";
echo r(DateTime::createFromFormat("!jS F Y", "1st March 2024")), "|";
echo r(DateTime::createFromFormat("!Y#m#d", "2024;03/15")), "|";
echo r(DateTime::createFromFormat("!Y#m", "2024x03")), "|";
echo r(DateTime::createFromFormat("!Y?m", "2024x03")), "|";
echo r(DateTime::createFromFormat("!Y-*-d", "2024-blah-15")), "|";
echo r(DateTime::createFromFormat("!Y-m-d+", "2024-03-15 junk here")), "|";
echo r(DateTime::createFromFormat("!Y-m-d", "2024-03-15 junk")), "|";
echo r(DateTime::createFromFormat("D d M Y H:i", "Tue 01 Jul 2024 09:30"));
"#,
    );
    assert_eq!(
        out,
        "2024-07-02 00:00:00|2024-07-07 00:00:00|FALSE|2024-03-15 00:00:00|2024-09-15 00:00:00|2024-03-01 00:00:00|2024-01-01 00:00:00|2024-01-01 00:00:00|123000|2024-03-01 00:00:00|2024-03-15 00:00:00|FALSE|2024-03-01 00:00:00|2024-01-15 00:00:00|2024-03-15 00:00:00|FALSE|2024-07-02 09:30:00"
    );
}

/// Verifies `createFromFormat()` parses the no-pad hour specifiers `G` (24-hour) and `g`
/// (12-hour, with lowercase `a` am/pm), and that a backslash-escaped `\T` matches a literal `T`
/// in the subject. Cross-checked against PHP 8.5: `09:05|21:05|2024-07-01 09:05`.
#[test]
fn test_create_from_format_no_pad_hours_and_escape() {
    let out = compile_and_run(
        r#"<?php
$a = DateTime::createFromFormat("Y-m-d G:i", "2024-07-01 9:05");
$b = DateTime::createFromFormat("Y-m-d g:i a", "2024-07-01 9:05 pm");
$c = DateTime::createFromFormat('Y-m-d\TH:i', "2024-07-01T09:05");
echo $a->format("H:i"), "|", $b->format("H:i"), "|", $c->format("Y-m-d H:i");
"#,
    );
    assert_eq!(out, "09:05|21:05|2024-07-01 09:05");
}

/// Verifies the PHP 8.3 date/time exception hierarchy: `DateMalformed*`/`DateInvalid*` extend
/// `DateException` (and thus `Exception`), while `DateObjectError`/`DateRangeError` extend
/// `DateError` (and thus `Error`). A subclass throw is catchable at every ancestor level. The
/// `DateInvalid*` pair (`DateInvalidTimeZoneException`, `DateInvalidOperationException`) is thrown
/// and caught as `DateException` here to confirm both are constructible and reachable through the
/// shared ancestor (the operations that raise them in PHP are tracked separately).
#[test]
fn test_date_exception_hierarchy() {
    let out = compile_and_run(
        r#"<?php
try { throw new DateMalformedStringException("s"); }
catch (DateException $e) { echo "de:", $e->getMessage(), "|"; }
try { throw new DateMalformedIntervalStringException("i"); }
catch (Exception $e) { echo "ex:", $e->getMessage(), "|"; }
try { throw new DateRangeError("r"); }
catch (DateError $e) { echo "der:", $e->getMessage(), "|"; }
try { throw new DateObjectError("o"); }
catch (Error $e) { echo "err:", $e->getMessage(), "|"; }
try { throw new DateInvalidTimeZoneException("z"); }
catch (DateException $e) { echo "ditz:", $e->getMessage(), "|"; }
try { throw new DateInvalidOperationException("p"); }
catch (DateException $e) { echo "diop:", $e->getMessage(); }
"#,
    );
    assert_eq!(out, "de:s|ex:i|der:r|err:o|ditz:z|diop:p");
}

/// Verifies `date_sun_info()` matches PHP's nine-key array bit-for-bit (a faithful port of timelib's
/// astro.c): integer Unix timestamps for sunrise/sunset/transit and the four twilight bounds, `true`
/// when the sun stays above an altitude all day (astronomical twilight at the Paris summer solstice),
/// and `false` during the Svalbard polar night. The `SUNFUNCS_RET_*` constants are also exercised.
#[test]
fn test_date_sun_info() {
    let out = compile_and_run(
        r#"<?php
$ts = mktime(0, 0, 0, 6, 21, 2024);
$i = date_sun_info($ts, 48.8566, 2.3522);
echo $i["sunrise"], ",", $i["sunset"], ",", $i["transit"], ",";
echo $i["civil_twilight_begin"], ",", $i["civil_twilight_end"], ",";
echo $i["nautical_twilight_begin"], ",", $i["nautical_twilight_end"], ",";
echo ($i["astronomical_twilight_begin"] === true ? "T" : "F"), "|";
$p = date_sun_info(mktime(0, 0, 0, 1, 1, 2024), 78.0, 15.0);
echo ($p["sunrise"] === false ? "F" : "x"), ",", ($p["sunset"] === false ? "F" : "x"), ",", $p["transit"];
"#,
    );
    assert_eq!(
        out,
        "1718941622,1718999880,1718970751,1718939068,1719002433,1718935423,1719006078,T|F,F,1704106998"
    );
}

/// Reproduces GH-14732: non-finite date_sun_info coordinates throw the
/// argument-specific ValueError, while the deprecated sunrise/sunset aliases
/// retain their `false` return contract.
#[test]
fn test_date_sun_functions_reject_non_finite_coordinates() {
    let out = compile_and_run(
        r#"<?php
foreach ([[NAN, 1.0], [-INF, 1.0], [1.0, NAN], [1.0, INF]] as $coords) {
    try {
        date_sun_info(1, $coords[0], $coords[1]);
    } catch (ValueError $error) {
        echo $error->getMessage(), "|";
    }
}
var_dump(@date_sunset(1, SUNFUNCS_RET_STRING, NAN, 1));
var_dump(@date_sunrise(1, SUNFUNCS_RET_STRING, 1, NAN));
"#,
    );
    assert_eq!(
        out,
        "date_sun_info(): Argument #2 ($latitude) must be finite|\
date_sun_info(): Argument #2 ($latitude) must be finite|\
date_sun_info(): Argument #3 ($longitude) must be finite|\
date_sun_info(): Argument #3 ($longitude) must be finite|\
bool(false)\nbool(false)\n"
    );
}

/// Verifies the deprecated `date_sunrise()` / `date_sunset()` across all three return formats:
/// `SUNFUNCS_RET_TIMESTAMP` (exact Unix timestamp), `SUNFUNCS_RET_STRING` (`"HH:MM"` with a UTC
/// offset applied), and `SUNFUNCS_RET_DOUBLE` (hour-of-day, rounded here to absorb last-ULP float
/// differences). A polar-summer case returns `false`. Values cross-checked against PHP.
#[test]
fn test_date_sunrise_sunset() {
    let out = compile_and_run(
        r#"<?php
$ts = mktime(0, 0, 0, 6, 21, 2024);
echo date_sunrise($ts, SUNFUNCS_RET_TIMESTAMP, 48.8566, 2.3522, 90 + 50 / 60, 0), ",";
echo date_sunset($ts, SUNFUNCS_RET_TIMESTAMP, 48.8566, 2.3522, 90 + 50 / 60, 0), "|";
echo date_sunrise($ts, SUNFUNCS_RET_STRING, 48.8566, 2.3522, 90 + 50 / 60, 2), ",";
echo date_sunset(mktime(0, 0, 0, 12, 21, 2024), SUNFUNCS_RET_STRING, 48.8566, 2.3522, 90 + 50 / 60, 1), "|";
echo round(date_sunrise($ts, SUNFUNCS_RET_DOUBLE, 48.8566, 2.3522, 90 + 50 / 60, 2), 6), "|";
echo (date_sunrise(mktime(0, 0, 0, 6, 21, 2024), SUNFUNCS_RET_STRING, 78.0, 15.0) === false ? "F" : "x");
"#,
    );
    assert_eq!(out, "1718941505,1718999996|05:45,16:58|5.751525|F");
}

/// Verifies non-finite UTC offsets return false in constant time for string and double formats.
#[test]
fn test_date_sunrise_non_finite_utc_offset_returns_false() {
    let out = compile_and_run(
        "<?php var_dump(@date_sunrise(1151690400, SUNFUNCS_RET_STRING, 38.4, -9.0, 90.83, INF)); var_dump(@date_sunrise(1151690400, SUNFUNCS_RET_DOUBLE, 38.4, -9.0, 90.83, INF));",
    );
    assert_eq!(out, "bool(false)\nbool(false)\n");
}

/// Verifies solar calculations select the civil day in the active timezone, matching timelib's
/// `timelib_unixtime2local()` behavior when the input timestamp straddles a UTC date boundary.
#[test]
fn test_date_sun_info_uses_active_timezone_civil_day() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Pacific/Honolulu");
$sun = date_sun_info(946684800, 21.3069, -157.8583);
echo date("Y-m-d H:i:s", $sun["sunrise"]), "|",
    date("Y-m-d H:i:s", $sun["sunset"]), "|", $sun["sunrise"];
"#,
    );
    assert_eq!(out, "1999-12-31 07:08:41|1999-12-31 18:00:14|946660121");
}

/// Verifies the deprecated solar wrappers throw php-src's ValueError for an invalid return format.
#[test]
fn test_date_sunrise_and_sunset_reject_invalid_return_format() {
    let out = compile_and_run(
        r#"<?php
try {
    @date_sunrise(time(), 3);
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
try {
    @date_sunset(time(), 4);
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "date_sunrise(): Argument #2 ($returnFormat) must be one of SUNFUNCS_RET_TIMESTAMP, SUNFUNCS_RET_STRING, or SUNFUNCS_RET_DOUBLE\n\
date_sunset(): Argument #2 ($returnFormat) must be one of SUNFUNCS_RET_TIMESTAMP, SUNFUNCS_RET_STRING, or SUNFUNCS_RET_DOUBLE\n"
    );
}

/// Verifies `strptime()` (the inverse of `strftime()`) fills PHP's `struct tm` array bit-for-bit:
/// numeric and month-name specifiers, the computed `tm_wday`/`tm_yday` for a full date, the
/// `tm_mon` 0-base / `tm_year` since-1900 conventions, an `unparsed` tail, time-only input leaving
/// the date fields at 0 (no wday/yday computation, matching glibc), and `false` on mismatch.
#[test]
fn test_strptime() {
    let out = compile_and_run(
        r#"<?php
$r = strptime("2024-06-15 14:30:45", "%Y-%m-%d %H:%M:%S");
echo $r["tm_sec"], ",", $r["tm_min"], ",", $r["tm_hour"], ",", $r["tm_mday"], ",";
echo $r["tm_mon"], ",", $r["tm_year"], ",", $r["tm_wday"], ",", $r["tm_yday"], ",[", $r["unparsed"], "]|";
$r2 = strptime("15 June 2024 rest", "%d %B %Y");
echo $r2["tm_mday"], ",", $r2["tm_mon"], ",", $r2["tm_year"], ",", $r2["tm_wday"], ",", $r2["tm_yday"], ",[", $r2["unparsed"], "]|";
$r3 = strptime("14:30", "%H:%M");
echo $r3["tm_hour"], ",", $r3["tm_min"], ",", $r3["tm_mday"], ",", $r3["tm_year"], ",", $r3["tm_wday"], "|";
echo (strptime("garbage", "%Y") === false ? "F" : "x");
"#,
    );
    assert_eq!(out, "45,30,14,15,5,124,6,166,[]|15,5,124,6,166,[ rest]|14,30,0,0,0|F");
}

/// Verifies `timezone_name_from_abbr()` searches the complete timelib-derived table
/// case-insensitively, including uncommon and historically ambiguous rows, and returns `false`
/// for abbreviations absent from php-src's table. Values cross-checked against PHP 8.5.6.
#[test]
fn test_timezone_name_from_abbr() {
    let out = compile_and_run(
        r#"<?php
echo timezone_name_from_abbr("CEST"), "|", timezone_name_from_abbr("est"), "|";
echo timezone_name_from_abbr("JST"), "|", timezone_name_from_abbr("MSK"), "|";
echo timezone_name_from_abbr("bDsT"), "|", timezone_name_from_abbr("AMT", 5692, 0), "|";
echo (timezone_name_from_abbr("ZZZ") === false ? "F" : "x"), "|";
echo (timezone_name_from_abbr("SGT") === false ? "F" : "x"), "|";
echo function_exists("timezone_name_from_abbr") ? "1" : "0";
"#,
    );
    assert_eq!(
        out,
        "Europe/Berlin|America/New_York|Asia/Tokyo|Europe/Moscow|Europe/London|Europe/Athens|F|F|1"
    );
}

/// Verifies `DateTimeZone::getLocation()` (and the `timezone_location_get()`
/// procedural alias) return the country code, latitude, longitude, and comments
/// PHP reports for a normal zone. Values cross-checked against PHP 8.5.6.
#[test]
fn test_timezone_get_location() {
    let out = compile_and_run(
        r#"<?php
$l = (new DateTimeZone("Europe/Paris"))->getLocation();
echo $l["country_code"], "|", $l["latitude"], "|", $l["longitude"], "|", $l["comments"], "\n";
$p = timezone_location_get(new DateTimeZone("America/Argentina/Buenos_Aires"));
echo $p["country_code"], "|", $p["latitude"], "|", $p["longitude"], "|", $p["comments"];
"#,
    );
    assert_eq!(
        out,
        "FR|48.86666|2.33333|\nAR|-34.6|-58.45|Buenos Aires (BA, CF)"
    );
}

/// Verifies `getLocation()` returns the special `??`/`-90`/`-180` values for `UTC`
/// and `false` for the legacy abbreviation-zones (e.g. `CET`) that carry no
/// location in PHP.
#[test]
fn test_timezone_get_location_special() {
    let out = compile_and_run(
        r#"<?php
$u = (new DateTimeZone("UTC"))->getLocation();
echo $u["country_code"], "|", $u["latitude"], "|", $u["longitude"], "\n";
echo (new DateTimeZone("CET"))->getLocation() === false ? "false" : "x";
"#,
    );
    assert_eq!(out, "??|-90|-180\nfalse");
}

/// Verifies `DateTimeZone::getTransitions()` with no arguments reproduces PHP's
/// full transition list: the synthetic `PHP_INT_MIN` row 0 (LMT), the first real
/// transition, and the last row, with the exact count for the bundled tz data.
#[test]
fn test_timezone_get_transitions_full() {
    let out = compile_and_run(
        r#"<?php
$t = (new DateTimeZone("Europe/Paris"))->getTransitions();
echo count($t), "\n";
echo $t[0]["ts"], "|", $t[0]["time"], "|", $t[0]["offset"], "|", ($t[0]["isdst"]?1:0), "|", $t[0]["abbr"], "\n";
echo $t[1]["ts"], "|", $t[1]["abbr"], "\n";
$last = $t[184];
echo $last["ts"], "|", $last["time"], "|", $last["offset"], "|", $last["abbr"];
"#,
    );
    assert_eq!(
        out,
        "185\n-9223372036854775808|-292277022657-01-27T08:29:52+00:00|561|0|LMT\n-2486592561|PMT\n2140045200|2037-10-25T01:00:00+00:00|3600|CET"
    );
}

/// Verifies the windowed `getTransitions($begin, $end)` form returns the synthetic
/// "active at begin" row plus the transitions inside the window, and that `UTC`
/// yields a single row while a no-transition zone (`CET`) yields `false`.
#[test]
fn test_timezone_get_transitions_windowed_and_special() {
    let out = compile_and_run(
        r#"<?php
$w = (new DateTimeZone("Europe/Paris"))->getTransitions(mktime(0,0,0,1,1,2020), mktime(0,0,0,6,1,2021));
echo count($w);
foreach ($w as $r) { echo "|", $r["ts"], ",", $r["abbr"]; }
echo "\n";
$u = (new DateTimeZone("UTC"))->getTransitions();
echo count($u), ",", $u[0]["abbr"], "\n";
echo (new DateTimeZone("CET"))->getTransitions() === false ? "false" : "x";
"#,
    );
    assert_eq!(
        out,
        "4|1577836800,CET|1585443600,CEST|1603587600,CET|1616893200,CEST\n1,UTC\nfalse"
    );
}

/// Verifies `DateTimeZone::listAbbreviations()` (and the
/// `timezone_abbreviations_list()` procedural alias) reproduce PHP's static
/// abbreviation table: key count, total rows, a sample entry, and a null
/// `timezone_id`. Cross-checked against PHP 8.5.6 (144 keys / 1127 rows).
#[test]
fn test_timezone_list_abbreviations() {
    let out = compile_and_run(
        r#"<?php
$a = DateTimeZone::listAbbreviations();
$rows = 0; foreach ($a as $v) { $rows += count($v); }
echo count($a), "|", $rows, "\n";
$x = $a["acdt"][0];
echo ($x["dst"]?1:0), "|", $x["offset"], "|", $x["timezone_id"], "\n";
echo $a["a"][0]["timezone_id"] === null ? "null" : "x", "\n";
echo count(timezone_abbreviations_list());
"#,
    );
    assert_eq!(out, "144|1127\n1|37800|Australia/Adelaide\nnull\n144");
}

/// Verifies the `DateTimeZone` region/group constants resolve to PHP's exact
/// bitmask values (used as `listIdentifiers()` selectors and in comparisons).
#[test]
fn test_datetime_zone_group_constants() {
    let out = compile_and_run(
        r#"<?php
echo DateTimeZone::AFRICA, ",", DateTimeZone::AMERICA, ",", DateTimeZone::ANTARCTICA, ",",
     DateTimeZone::ARCTIC, ",", DateTimeZone::ASIA, ",", DateTimeZone::ATLANTIC, ",",
     DateTimeZone::AUSTRALIA, ",", DateTimeZone::EUROPE, ",", DateTimeZone::INDIAN, ",",
     DateTimeZone::PACIFIC, ",", DateTimeZone::UTC, ",", DateTimeZone::ALL, ",",
     DateTimeZone::ALL_WITH_BC, ",", DateTimeZone::PER_COUNTRY;
"#,
    );
    assert_eq!(out, "1,2,4,8,16,32,64,128,256,512,1024,2047,4095,4096");
}

/// Verifies `DatePeriod::getIterator()` returns an independent iterator over the period's dates,
/// usable with `foreach` and `iterator_to_array`.
#[test]
fn test_dateperiod_get_iterator() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(new DateTime("2020-01-01"), new DateInterval("P1D"), new DateTime("2020-01-04"));
$days = "";
foreach ($p->getIterator() as $d) { $days .= $d->format("d"); }
$p2 = new DatePeriod(new DateTime("2020-01-01"), new DateInterval("P1D"), 2);
echo $days, "|", count(iterator_to_array($p2->getIterator()));
"#,
    );
    assert_eq!(out, "010203|3");
}

/// Verifies php-src's `DatePeriod` interface and cursor contract: the period is only an
/// `IteratorAggregate`, each iterator is distinct, and exhausted iteration leaves `$current`
/// on the first instant after the yielded range.
#[test]
fn test_dateperiod_independent_iterator_and_current_contract() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(
    new DateTime("2024-01-01"),
    new DateInterval("P1D"),
    new DateTime("2024-01-04")
);
echo ($p instanceof IteratorAggregate ? "A" : "-"),
     ($p instanceof Iterator ? "I" : "-"), "|",
     ($p->current === null ? "null" : "set"), "|";
$a = $p->getIterator();
$b = $p->getIterator();
echo (($a !== $b && $a !== $p) ? "independent" : "shared"), "|",
     ($p->current === null ? "null" : "set"), "|";
$a->rewind();
echo $p->current->format("Y-m-d"), "|";
$a->next();
echo $p->current->format("Y-m-d"), "|";
foreach ($b as $date) {}
echo $p->current->format("Y-m-d");
"#,
    );
    assert_eq!(
        out,
        "A-|null|independent|null|2024-01-01|2024-01-02|2024-01-04"
    );
}

/// Verifies `DatePeriod`'s seven public properties are virtual and reject userland writes with
/// php-src's readonly-property error while remaining reflection-visible.
#[test]
fn test_dateperiod_virtual_properties_reject_writes() {
    let out = compile_and_run(
        r#"<?php
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), 2);
$rp = new ReflectionProperty(DatePeriod::class, "start");
echo ($rp->isVirtual() ? "virtual" : "stored"), "|",
     ($rp->isReadOnly() ? "flagged" : "handler"), "|",
     ($rp->hasHooks() ? "hooks" : "handler-only"), "|";
try {
    $p->start = null;
    echo "write";
} catch (Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "virtual|handler|handler-only|Cannot modify readonly property DatePeriod::$start"
    );
}

/// Verifies `function_exists()` recognizes the three timezone-introspection
/// procedural aliases even when they are not called (so the elephc_tz bridge is
/// not linked) — matching PHP, where they are always-defined functions.
#[test]
fn test_function_exists_timezone_introspection_aliases() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("timezone_location_get") ? "1" : "0";
echo function_exists("timezone_transitions_get") ? "1" : "0";
echo function_exists("timezone_abbreviations_list") ? "1" : "0";
echo function_exists("TIMEZONE_LOCATION_GET") ? "1" : "0";
echo function_exists("not_a_tz_function") ? "1" : "0";
"#,
    );
    assert_eq!(out, "11110");
}

/// Verifies every function exported by php-src's date extension is present in
/// Elephc's case-insensitive builtin catalog.
#[test]
fn test_php_src_date_function_inventory() {
    let functions = [
        "checkdate",
        "date",
        "date_add",
        "date_create",
        "date_create_from_format",
        "date_create_immutable",
        "date_create_immutable_from_format",
        "date_date_set",
        "date_default_timezone_get",
        "date_default_timezone_set",
        "date_diff",
        "date_format",
        "date_get_last_errors",
        "date_interval_create_from_date_string",
        "date_interval_format",
        "date_isodate_set",
        "date_modify",
        "date_offset_get",
        "date_parse",
        "date_parse_from_format",
        "date_sub",
        "date_sun_info",
        "date_sunrise",
        "date_sunset",
        "date_time_set",
        "date_timestamp_get",
        "date_timestamp_set",
        "date_timezone_get",
        "date_timezone_set",
        "getdate",
        "gmdate",
        "gmmktime",
        "gmstrftime",
        "idate",
        "localtime",
        "mktime",
        "strftime",
        "strtotime",
        "time",
        "timezone_abbreviations_list",
        "timezone_identifiers_list",
        "timezone_location_get",
        "timezone_name_from_abbr",
        "timezone_name_get",
        "timezone_offset_get",
        "timezone_open",
        "timezone_transitions_get",
        "timezone_version_get",
    ];
    let probes = functions
        .iter()
        .map(|function| {
            format!(
                "echo function_exists(\"{function}\") && function_exists(\"{}\") ? \"1\" : \"0\";",
                function.to_uppercase()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let out = compile_and_run(&format!("<?php\n{probes}\n"));
    assert_eq!(out, "1".repeat(functions.len()));
}

/// Verifies sub-second arithmetic: diff() reports the fractional-second difference
/// in DateInterval::$f (with a one-second borrow and a microsecond-aware invert),
/// and add()/sub() apply an interval's $f with carry. Microseconds are sourced via
/// setMicrosecond() (the constructor does not parse a fractional second).
#[test]
fn test_datetime_subsecond_arithmetic() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
function mk($s, $us) { $d = new DateTime($s); $d->setMicrosecond($us); return $d; }
// diff: 1.5s apart -> s=1 f=0.5
$x = mk("2020-01-01 00:00:00", 250000)->diff(mk("2020-01-01 00:00:01", 750000));
echo $x->s, ",", $x->f, ",", $x->invert, "|";
// diff with borrow: 0.5s apart -> s=0 f=0.5
$y = mk("2020-01-01 00:00:00", 750000)->diff(mk("2020-01-01 00:00:01", 250000));
echo $y->s, ",", $y->f, "|";
// diff micro-aware invert: same second, target earlier
$z = mk("2020-01-01 00:00:05", 800000)->diff(mk("2020-01-01 00:00:05", 300000));
echo $z->s, ",", $z->f, ",", $z->invert, "|";
// add/sub with f
$iv = new DateInterval("PT1S"); $iv->f = 0.5;
$a = mk("2020-01-01 00:00:00", 250000); $a->add($iv);
$b = mk("2020-01-01 00:00:01", 750000); $b->sub($iv);
echo $a->format("s.u"), ",", $b->format("s.u"), "|";
// add with carry across the second
$iv2 = new DateInterval("PT0S"); $iv2->f = 0.5;
$c = mk("2020-01-01 00:00:00", 800000); $c->add($iv2);
echo $c->format("s.u");
"#,
    );
    assert_eq!(out, "1,0.5,0|0,0.5|0,0.5,1|01.750000,00.250000|01.300000");
}

/// Verifies the strftime specifiers that were previously approximated now match
/// PHP exactly: %U/%W week numbers (Sunday/Monday based), %V (ISO), the
/// space-padded %e/%k/%l, %c (with its space-padded day giving a double space),
/// and %g (two-digit ISO year). Cross-checked against PHP 8.5.6.
#[test]
fn test_strftime_fixed_specifiers() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
function row($ts) {
    $o = "";
    foreach (["%U","%V","%W","%e","%k","%l","%c","%g"] as $s) { $o .= strftime($s, $ts) . ";"; }
    return $o;
}
echo row(1593612645), "|";  // 2020-07-01 14:10:45 Wed
echo row(1577836805), "|";  // 2020-01-01 00:00:05 Wed
echo row(1609459199), "|";  // 2020-12-31 23:59:59 Thu
echo row(978307200);        // 2001-01-01 00:00:00 Mon
"#,
    );
    assert_eq!(
        out,
        "26;27;26; 1;14; 2;Wed Jul  1 14:10:45 2020;20;|\
         00;01;00; 1; 0;12;Wed Jan  1 00:00:05 2020;20;|\
         52;53;52;31;23;11;Thu Dec 31 23:59:59 2020;20;|\
         00;01;01; 1; 0;12;Mon Jan  1 00:00:00 2001;01;"
    );
}

/// Verifies `strftime`'s `%x` (locale date) and `%X` (locale time) match PHP's default C/POSIX
/// locale byte-for-byte, complementing `test_strftime_fixed_specifiers` (which covers `%c`).
#[test]
fn test_strftime_locale_date_time() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
echo strftime("%x|%X", 1593612645), "|", strftime("%x|%X", 978307200);
"#,
    );
    // 2020-07-01 14:10:45 -> 07/01/20 | 14:10:45 ; 2001-01-01 00:00:00 -> 01/01/01 | 00:00:00
    assert_eq!(out, "07/01/20|14:10:45|01/01/01|00:00:00");
}

/// Verifies strftime preserves multibyte literal bytes while translating ASCII `%` tokens.
#[test]
fn test_strftime_preserves_utf8_literals() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$formatted = @strftime("あ-%Y", 0);
echo $formatted, "|", bin2hex($formatted);
"#,
    );
    assert_eq!(out, "あ-1970|e381822d31393730");
}

/// Verifies PHP's `LC_*` constants use the target C library's category values.
#[test]
fn test_locale_category_constants_match_target_php() {
    let out = compile_and_run(
        r#"<?php
echo LC_CTYPE, ",", LC_NUMERIC, ",", LC_TIME, ",", LC_COLLATE, ",";
echo LC_MONETARY, ",", LC_ALL, ",", LC_MESSAGES;
"#,
    );
    if cfg!(target_os = "macos") {
        assert_eq!(out, "2,4,5,1,3,0,6");
    } else {
        assert_eq!(out, "0,1,2,3,4,6,5");
    }
}

/// Verifies `setlocale` accepts string arrays and variadic fallbacks and returns `string|false`.
#[test]
fn test_setlocale_php_candidate_order_and_result_type() {
    let out = compile_and_run(
        r#"<?php
echo \SeTlOcAlE(LC_ALL, ["elephc_missing_locale", "C"]), "|";
var_dump(setlocale(LC_ALL, "elephc_missing_locale"));
echo "|", setlocale(LC_ALL, "elephc_missing_locale", "C");
echo "|", setlocale(LC_ALL, 0);
"#,
    );
    assert_eq!(out, "C|bool(false)\n|C|C");
}

/// Verifies `print_r()` exposes php-src's virtual DateTime, DateTimeZone, and DateInterval fields.
#[test]
fn test_print_r_datetime_virtual_properties_and_return_mode() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
class CustomDateInterval extends DateInterval {
    public $unit = 42;
    protected $hidden = 7;
    private $secret = 8;
}
$date = new DateTime("2020-01-02 03:04:05.123456", new DateTimeZone("UTC"));
echo print_r($date, true);
print_r(new DateTimeZone("+02:30"));
print_r(new DateInterval("P1Y2M3DT4H5M6S"));
$serialized = serialize(new DateInterval("P1Y2M3DT4H5M6S"));
print_r(unserialize($serialized));
print_r(new CustomDateInterval("P1D"));
"#,
    );
    assert_eq!(
        out,
        r#"DateTime Object
(
    [date] => 2020-01-02 03:04:05.123456
    [timezone_type] => 3
    [timezone] => UTC
)
DateTimeZone Object
(
    [timezone_type] => 1
    [timezone] => +02:30
)
DateInterval Object
(
    [y] => 1
    [m] => 2
    [d] => 3
    [h] => 4
    [i] => 5
    [s] => 6
    [f] => 0
    [invert] => 0
    [days] =>{empty}
    [from_string] =>{empty}
)
DateInterval Object
(
    [y] => 1
    [m] => 2
    [d] => 3
    [h] => 4
    [i] => 5
    [s] => 6
    [f] => 0
    [invert] => 0
    [days] =>{empty}
    [from_string] =>{empty}
)
CustomDateInterval Object
(
    [unit] => 42
    [hidden:protected] => 7
    [secret:CustomDateInterval:private] => 8
    [y] => 0
    [m] => 0
    [d] => 1
    [h] => 0
    [i] => 0
    [s] => 0
    [f] => 0
    [invert] => 0
    [days] =>{empty}
    [from_string] =>{empty}
)
"#
        .replace("{empty}", " ")
    );
}

/// Verifies the constructor parses a trailing fractional second
/// (HH:MM:SS.ffffff) into the microsecond component (padded/truncated to six
/// digits), leaves non-fractional dots (a DD.MM.YYYY-style separator) untouched,
/// and that the value survives format()/getMicrosecond() in a shared function
/// frame (the parsing lives in static helpers to keep the ctor frame small).
#[test]
fn test_datetime_constructor_fractional_seconds() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
function show($s) { $d = new DateTime($s); return $d->format("H:i:s") . "/" . $d->getMicrosecond(); }
echo show("2020-01-01 12:30:45.123456"), "|";
echo show("2020-01-01 00:00:01.5"), "|";
echo show("2020-01-01 12:30:45"), "|";
echo show("2020-03-15"), "|";
$a = new DateTime("2020-01-01 00:00:00.250000");
$b = new DateTime("2020-01-01 00:00:01.750000");
$x = $a->diff($b);
echo $x->s, ",", $x->f;
"#,
    );
    assert_eq!(out, "12:30:45/123456|00:00:01/500000|12:30:45/0|00:00:00/0|1,0.5");
}

/// Regression: constructing `DateTime` from an untyped (`Mixed`) argument must not corrupt the
/// parsed instant. The constructor self-reassigns `$datetime = __elephc_strip_micros($datetime)`;
/// when the helper returned the borrowed argument, that assignment freed the owned Mixed-derived
/// source string and then reused the freed pointer, so `strtotime()` saw garbage and the object
/// leaked the current wall-clock time (or `-1`). Strings reaching `strtotime` must survive the
/// reassignment regardless of whether they have a fractional second.
#[test]
fn test_datetime_constructor_untyped_arg_no_use_after_free() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
function ts($s) { $d = new DateTime($s); return $d->getTimestamp(); }
function hms($s) { $d = new DateTime($s); return $d->format("H:i:s"); }
echo ts("2020-01-01 11:11:11"), "|", ts("2020-03-15"), "|",
     hms("2020-01-01 11:11:11"), "|", hms("2020-01-01 11:11:11.250000");
"#,
    );
    assert_eq!(out, "1577877071|1584230400|11:11:11|11:11:11");
}

/// Verifies modify() applies timelib's microsecond and millisecond aliases with
/// carry/borrow into the whole second, alone or combined with other clauses,
/// while leaving sub-second-free modifiers unchanged.
#[test]
fn test_datetime_modify_microseconds() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
function t($init, $us, $mod) {
    $d = new DateTime($init); $d->setMicrosecond($us); $d->modify($mod);
    return $d->format("H:i:s.u");
}
echo t("00:00:00", 0, "+500000 microseconds"), "|";
echo t("00:00:00", 0, "+1500000 microseconds"), "|";
echo t("00:00:01", 100000, "-200000 microseconds"), "|";
echo t("00:00:00", 0, "+1 hour +500000 microseconds"), "|";
echo t("00:00:00", 0, "+1 microsecond"), "|";
echo t("00:00:00", 0, "+500000 usec"), "|";
echo t("01:01:01", 1, "-100 ms"), "|";
echo t("00:00:00", 0, "+3 msecs"), "|";
echo t("00:00:00", 0, "+9 microseconds"), "|";
echo t("00:00:00", 0, "+11 µsec"), "|";
echo t("12:00:00", 0, "+1 day"), "|";
echo t("12:47:18", 81921, "yesterday"), "|";
echo t("12:47:18", 81921, "noon"), "|";
echo t("12:47:18", 81921, "10 weekday");
"#,
    );
    assert_eq!(
        out,
        "00:00:00.500000|00:00:01.500000|00:00:00.900000|01:00:00.500000|\
00:00:00.000001|00:00:00.500000|01:01:00.900001|00:00:00.003000|\
00:00:00.000009|00:00:00.000011|12:00:00.000000|00:00:00.000000|\
12:00:00.000000|12:47:18.081921"
    );
}

/// Verifies modify() uses php-src's field-copy algorithm rather than strtotime-with-base
/// semantics for fractional relative expressions that cross a daylight-saving transition.
#[test]
fn test_datetime_modify_fractional_relative_expression_across_dst() {
    let out = compile_and_run(
        r#"<?php
$start = new DateTime("2024-03-30 12:00:00.250000", new DateTimeZone("Europe/Paris"));
$end = (clone $start)->modify("+2 days 3 hours 4 minutes 5.500000 seconds");
$difference = $start->diff($end);
echo $end->format("Y-m-d H:i:s.u e T P U.u"), "\n";
echo $difference->format("%a|%H:%I:%S.%F");
"#,
    );
    assert_eq!(
        out,
        "2024-04-01 08:54:00.000000 Europe/Paris CEST +02:00 1711954440.000000\n\
         1|20:53:59.750000"
    );
}

/// Verifies `usort` over an array of `DateTime` objects with an unannotated
/// spaceship comparator: the comparator parameters are typed as `DateTime`, so
/// `$a <=> $b` lowers to the instant comparison and the array sorts chronologically.
#[test]
fn test_usort_datetime_spaceship_comparator() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$dates = [
    new DateTime("2024-06-01"),
    new DateTime("2024-01-15"),
    new DateTime("2024-03-20"),
];
usort($dates, function ($a, $b) { return $a <=> $b; });
foreach ($dates as $d) { echo $d->format("Y-m-d"), ","; }
"#,
    );
    assert_eq!(out, "2024-01-15,2024-03-20,2024-06-01,");
}

/// Verifies `usort` over `DateTime` objects with a comparator that calls a method
/// on each element: the comparator's unannotated parameters resolve to `DateTime`
/// so `$a->getTimestamp()` type-checks and the array sorts by instant.
#[test]
fn test_usort_datetime_method_comparator() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$dates = [
    new DateTime("2024-06-01"),
    new DateTime("2024-01-15"),
    new DateTime("2024-03-20"),
];
usort($dates, fn ($a, $b) => $a->getTimestamp() <=> $b->getTimestamp());
foreach ($dates as $d) { echo $d->format("Y-m-d"), ","; }
"#,
    );
    assert_eq!(out, "2024-01-15,2024-03-20,2024-06-01,");
}

/// Verifies that `DateTime::__construct()` throws `DateMalformedStringException`
/// when given an unparseable time string, matching PHP 8.3+ behavior.
#[test]
fn test_datetime_constructor_malformed_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $dt = new DateTime("not a date");
    echo "no-throw";
} catch (DateMalformedStringException $e) {
    echo "caught:" . get_class($e);
}
"#,
    );
    assert_eq!(out, "caught:DateMalformedStringException");
}

/// Verifies that `DateTimeImmutable::__construct()` also throws on malformed input.
#[test]
fn test_datetime_immutable_constructor_malformed_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $dt = new DateTimeImmutable("totally invalid");
    echo "no-throw";
} catch (DateMalformedStringException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies that `DateTime::modify()` throws `DateMalformedStringException`
/// when the modifier cannot be parsed.
#[test]
fn test_datetime_modify_malformed_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $dt = new DateTime("2024-01-01");
    $dt->modify("nonsense modifier");
    echo "no-throw";
} catch (DateMalformedStringException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies that `DateInterval::__construct()` throws `DateMalformedIntervalStringException`
/// for a non-ISO-8601 duration string.
#[test]
fn test_date_interval_constructor_invalid_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $iv = new DateInterval("garbage");
    echo "no-throw";
} catch (DateMalformedIntervalStringException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies that `DateInterval::createFromDateString()` throws on unknown unit words.
#[test]
fn test_date_interval_create_from_date_string_unknown_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    DateInterval::createFromDateString("1 fortnight 3 eons");
    echo "no-throw";
} catch (DateMalformedIntervalStringException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies `DatePeriod::getEndDate()` returns `null` for the recurrence-count form.
#[test]
fn test_date_period_get_end_date_null_for_recurrences() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$period = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), 3);
$end = $period->getEndDate();
echo $end === null ? "null" : "not-null";
"#,
    );
    assert_eq!(out, "null");
}

/// Verifies `DatePeriod::getStartDate()` and `current()` preserve `DateTimeImmutable`.
#[test]
fn test_date_period_preserves_immutable_start() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$period = new DatePeriod(new DateTimeImmutable("2024-01-01"), new DateInterval("P1D"), 2);
$iterator = $period->getIterator();
$iterator->rewind();
echo get_class($period->getStartDate()), "|", get_class($iterator->current());
"#,
    );
    assert_eq!(out, "DateTimeImmutable|DateTimeImmutable");
}

/// Reproduces php-src's DatePeriod iterator allocation contract: each
/// `current()` call returns a fresh date snapshot, so mutating one result cannot
/// alter a later read at the same iterator position.
#[test]
fn test_date_period_iterator_current_returns_fresh_snapshots() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$period = new DatePeriod(
    new DateTime("2018-12-31 00:00:00"),
    new DateInterval("P1M"),
    1
);
$iterator = $period->getIterator();
$first = $iterator->current();
$second = $iterator->current();
$first->setTimestamp(0);
echo $second->format("Y-m-d"), "|", $iterator->current()->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2018-12-31|2018-12-31");
}

/// Verifies `DateTime::format()` handles PHP 8.2+ expanded-year specifiers `X` and `x`.
#[test]
fn test_datetime_format_expanded_year() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = new DateTime("2024-03-15 12:00:00");
echo $a->format("X-m-d"), "|", $a->format("x-m-d"), "|";
$a->setTime(0, 0, 0);
$a->setDate(20201, 1, 1);
$state = $a->__serialize();
$copy = unserialize(serialize($a));
echo $state["date"], "|", $copy->format("x-m-d H:i:s.u");
"#,
    );
    assert_eq!(
        out,
        "+2024-03-15|2024-03-15|+20201-01-01 00:00:00.000000|+20201-01-01 00:00:00.000000"
    );
}

/// Verifies `DateTime::setTime()` accepts the PHP 8.4+ `$microsecond` parameter.
#[test]
fn test_datetime_set_time_microsecond() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$dt = new DateTimeImmutable("2024-01-01 00:00:00");
$dt = $dt->setTime(12, 30, 45, 123456);
echo $dt->format("H:i:s.u");
"#,
    );
    assert_eq!(out, "12:30:45.123456");
}

/// Verifies elephc does not expose the non-existent `DateUnknownException` class.
#[test]
fn test_date_unknown_exception_is_absent_like_php_src() {
    let out = compile_and_run(
        r#"<?php
var_dump(class_exists("DateUnknownException"));
"#,
    );
    assert_eq!(out, "bool(false)\n");
}

/// Verifies `strptime()` consumes the week-number, weekday, and timezone specifiers
/// (`%u %w %U %W %V %z %Z`) without failing the parse, matching glibc's lenient consumption.
#[test]
fn test_strptime_extended_specifiers() {
    let out = compile_and_run(
        r#"<?php
$r = strptime("2024-06-15 14:30:45 +0200 CEST", "%Y-%m-%d %H:%M:%S %z %Z");
echo $r["tm_mday"], ",", $r["tm_mon"], ",", $r["tm_year"], ",[", $r["unparsed"], "]|";
$r2 = strptime("2024 23 6", "%Y %V %u");
echo $r2["tm_year"], ",[", $r2["unparsed"], "]";
"#,
    );
    assert_eq!(out, "15,5,124,[]|124,[]");
}

/// Verifies `date_parse_from_format()` handles textual month names, fractional seconds, Unix
/// timestamps, and timezone metadata beyond the numeric fields.
#[test]
fn test_date_parse_from_format_textual_and_extended() {
    let out = compile_and_run(
        r#"<?php
$a = date_parse_from_format("j F Y H:i:s.u", "15 March 2024 14:30:45.123456");
echo $a["year"], "-", $a["month"], "-", $a["day"], " ", $a["hour"], ":", $a["minute"], ":", $a["second"], ".", $a["fraction"], "|", $a["error_count"];
echo "|";
$b = date_parse_from_format("Y-m-d\\TH:i:sP", "2024-06-15T14:30:45+02:00");
echo $b["year"], "-", $b["month"], "-", $b["day"], ",", $b["is_localtime"] ? "local" : "utc",
     ",", $b["zone_type"], ",", $b["zone"];
echo "|";
$c = date_parse_from_format("U", "1700000000");
echo $c["year"], "-", $c["month"], "-", $c["day"], " ", $c["hour"], ":", $c["minute"], ":",
     $c["second"], "|", $c["fraction"], "|", $c["zone_type"], "|", $c["zone"];
"#,
    );
    assert_eq!(
        out,
        "2024-3-15 14:30:45.0.123456|0|2024-6-15,local,1,7200|2023-11-14 22:13:20|0|1|0"
    );
}

/// Verifies `date_parse_from_format()` returns php-src's fractional value and byte-positioned
/// warning/error maps for invalid dates, accepted trailing data, and short input.
#[test]
fn test_date_parse_from_format_php_src_diagnostics_shape() {
    let out = compile_and_run(
        r#"<?php
$fraction = date_parse_from_format(
    "Y-m-d H:i:s.uP",
    "2024-03-15 14:30:45.123456+02:30"
);
echo $fraction["fraction"], "|", $fraction["zone"], "|";
$invalid = date_parse_from_format("Y-m-d", "2024-02-31");
echo $invalid["warning_count"], ":", $invalid["warnings"][10], "|";
$trailing = date_parse_from_format("Y-m-d+", "2024-01-02tail");
echo $trailing["warning_count"], ":", $trailing["warnings"][10], "|";
$short = date_parse_from_format("Y-m-d H:i:s", "2024-01");
echo $short["error_count"], ":", $short["errors"][7];
"#,
    );
    assert_eq!(
        out,
        "0.123456|9000|1:The parsed date was invalid|1:Trailing data|1:Not enough data available to satisfy format"
    );
}

/// Verifies `date_parse()` accepts slash dates and military-zone suffixes and preserves php-src's
/// warning/error metadata for the ambiguous and invalid compatibility cases.
#[test]
fn test_date_parse_php_src_residual_cases() {
    let out = compile_and_run(
        r#"<?php
$slash = date_parse("2024/06/15");
echo $slash["year"], "-", $slash["month"], "-", $slash["day"], "|";
$zone = date_parse("2024-01-02x");
echo $zone["zone_type"], ":", $zone["zone"], ":", $zone["tz_abbr"], "|";
$ambiguous = date_parse("2024-0x-15");
echo $ambiguous["warning_count"], ":", $ambiguous["warnings"][7], ":",
     $ambiguous["warnings"][11], "|";
$invalid = date_parse("not a date");
echo $invalid["warning_count"], ":", $invalid["error_count"], ":",
     $invalid["errors"][0], ":", $invalid["errors"][6];
"#,
    );
    assert_eq!(
        out,
        "2024-6-15|2:-39600:X|2:Double timezone specification:The parsed date was invalid|1:2:The timezone could not be found in the database:Double timezone specification"
    );
}

/// Verifies php-src's global date-format constants and the suppression-aware deprecation emitted
/// by the RFC7231 and SUNFUNCS constant surfaces.
#[test]
fn test_datetime_global_constants_and_deprecations() {
    let out = compile_and_run_capture(
        r#"<?php
echo DATE_ATOM, "|", DATE_RFC3339_EXTENDED, "|";
echo @DATE_RFC7231, "|";
echo DateTimeInterface::RFC7231, "|";
echo SUNFUNCS_RET_TIMESTAMP, "|", @SUNFUNCS_RET_STRING;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "Y-m-d\\TH:i:sP|Y-m-d\\TH:i:s.vP|D, d M Y H:i:s \\G\\M\\T|\
D, d M Y H:i:s \\G\\M\\T|0|1"
    );
    assert!(
        out.stderr.contains(
            "Deprecated: Constant DateTimeInterface::RFC7231 is deprecated since 8.5"
        ),
        "expected class-constant deprecation, got stderr={}",
        out.stderr
    );
    assert!(
        out.stderr.contains(
            "Deprecated: Constant SUNFUNCS_RET_TIMESTAMP is deprecated since 8.4"
        ),
        "expected SUNFUNCS deprecation, got stderr={}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("Constant DATE_RFC7231"),
        "the @-suppressed global RFC7231 read should be silent, got stderr={}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("Constant SUNFUNCS_RET_STRING"),
        "the @-suppressed SUNFUNCS read should be silent, got stderr={}",
        out.stderr
    );
}

/// Verifies deprecated date functions and invalid `idate()` calls emit the exact suppression-aware
/// runtime diagnostics that php-src exposes.
#[test]
fn test_datetime_function_runtime_diagnostics() {
    let out = compile_and_run_capture(
        r#"<?php
date_default_timezone_set("UTC");
echo (idate("") === false ? "empty" : "bad"), "|";
echo (@idate("q") === false ? "suppressed" : "bad"), "|";
echo gmstrftime("%Y", 0), "|";
@strftime("%Y", 0);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "empty|suppressed|1970|");
    assert!(
        out.stderr
            .contains("Warning: idate(): idate format is one char"),
        "expected idate warning, got stderr={}",
        out.stderr
    );
    assert!(
        !out.stderr
            .contains("Warning: idate(): Unrecognized date format token"),
        "the @-suppressed idate warning should be silent, got stderr={}",
        out.stderr
    );
    assert!(
        out.stderr.contains(
            "Deprecated: Function gmstrftime() is deprecated since 8.1, use IntlDateFormatter::format() instead"
        ),
        "expected gmstrftime deprecation, got stderr={}",
        out.stderr
    );
    assert!(
        !out.stderr
            .contains("Deprecated: Function strftime() is deprecated"),
        "the @-suppressed strftime deprecation should be silent, got stderr={}",
        out.stderr
    );
}

/// Verifies direct date-object wakeups emit PHP 8.5's deprecation and reject invalid state with the
/// exact `Error`, while `DateInterval::__wakeup()` remains callable after its deprecation.
#[test]
fn test_datetime_wakeup_runtime_contract() {
    let out = compile_and_run_capture(
        r#"<?php
$d = new DateTime();
try {
    $d->__wakeup();
} catch (Error $e) {
    echo $e->getMessage(), "|";
}
$iv = new DateInterval("P1D");
@$iv->__wakeup();
echo "interval-ok";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "Invalid serialization data for DateTime object|interval-ok"
    );
    assert!(
        out.stderr.contains(
            "Deprecated: Method DateTime::__wakeup() is deprecated since 8.5"
        ),
        "expected DateTime wakeup deprecation, got stderr={}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("DateInterval::__wakeup"),
        "the @-suppressed DateInterval wakeup deprecation should be silent, got stderr={}",
        out.stderr
    );
}

/// Verifies PHP 8.5's `NoDiscard` contract for `DateTimeImmutable`: a plain unused mutator result
/// warns, while assignment, `(void)`, `@`, and mutable `DateTime` uses remain silent.
#[test]
fn test_datetime_immutable_nodiscard_runtime_warning() {
    let out = compile_and_run_capture(
        r#"<?php
$immutable = new DateTimeImmutable("2024-01-01");
$immutable->modify("+1 day");
$used = $immutable->add(new DateInterval("P1D"));
(void)$immutable->setDate(2024, 2, 1);
@$immutable->sub(new DateInterval("P1D"));
$mutable = new DateTime("2024-01-01");
$mutable->modify("+1 day");
echo $used->format("Y-m-d"), "|", $mutable->format("Y-m-d");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2024-01-02|2024-01-02");
    assert!(
        out.stderr.contains(
            "Warning: The return value of method DateTimeImmutable::modify() should either be used or intentionally ignored by casting it as (void), as DateTimeImmutable::modify() does not modify the object itself"
        ),
        "expected NoDiscard warning, got stderr={}",
        out.stderr
    );
    assert_eq!(
        out.stderr.matches("The return value of method").count(),
        1,
        "only the plain unused result should warn, got stderr={}",
        out.stderr
    );
}

/// Verifies `DateTime::createFromFormat()` parses PHP 8.2+ expanded-year specifiers `X` and `x`.
#[test]
fn test_create_from_format_expanded_year() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = DateTime::createFromFormat("X-m-d", "+2024-03-15");
echo $a->format("Y-m-d");
echo "|";
$b = DateTime::createFromFormat("x-m-d", "2024-03-15");
echo $b->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2024-03-15|2024-03-15");
}

/// Verifies `DatePeriod::createFromISO8601String()` accepts the no-`R` `start/interval/end` form.
#[test]
fn test_date_period_create_from_iso8601_string_no_recurrence() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = DatePeriod::createFromISO8601String("2024-01-01T00:00:00Z/P1D/2024-01-04T00:00:00Z");
$dates = [];
foreach ($p as $d) { $dates[] = $d->format("Y-m-d"); }
echo count($dates), "|", $dates[0], "|", $dates[count($dates)-1];
"#,
    );
    assert_eq!(out, "3|2024-01-01|2024-01-03");
}

/// Verifies `date_parse()` falls back to `strtotime()` for relative strings not covered by the
/// explicit format list, decomposing the resolved instant into components.
#[test]
fn test_date_parse_relative_fallback() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = date_parse("15 March 2024");
echo $a["year"], "-", $a["month"], "-", $a["day"], "|", $a["error_count"];
echo "|";
// Relative string resolves to a full instant (midnight fields are 0, not false).
$b = date_parse("next Monday");
echo ($b["hour"] === false ? "F" : "v"), $b["error_count"];
echo "|";
$c = date_parse("totally not a date");
echo $c["error_count"];
"#,
    );
    assert_eq!(out, "2024-3-15|0|v0|4");
}

/// Verifies the runtime `date()`/`gmdate()` formatter emits the PHP 8.2 expanded-year specifiers
/// `X` (always signed, minimum 4 digits) and `x` (signed only for year < 0 or year >= 10000) for
/// ordinary CE years, exercised through both the UTC (`gmdate`) and local (`date`) entry points.
/// Pinned to the Unix epoch and a fixed 2024 timestamp so the assertions are timezone-independent.
#[test]
fn test_gmdate_expanded_year_x_x_normal() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
echo gmdate("X-m-d", 0), "|", gmdate("x-m-d", 0);
echo "|", gmdate("X\TH:i:sP", 0), "|", gmdate("x\TH:i:sP", 0);
echo "|", date("X-m-d", 1710460800);
"#,
    );
    assert_eq!(out, "+1970-01-01|1970-01-01|+1970T00:00:00+00:00|1970T00:00:00+00:00|+2024-03-15");
}

/// Verifies the expanded-year runtime path for the magnitude branches that the 4-digit writer
/// cannot reach: a 6-digit CE year (>=10000 routes to the variable-width writer with a '+' sign)
/// and a BCE year (negative branch emits '-' then the 4-digit magnitude). Only the year token is
/// asserted so the test stays robust to per-target proleptic-Gregorian day drift for extreme years.
#[test]
fn test_gmdate_expanded_year_far_and_bce() {
    let out = compile_and_run(
        r#"<?php
echo gmdate("X", 3093542366400), "|", gmdate("x", 3093542366400);
echo "|", gmdate("X", -62310686400), "|", gmdate("x", -62310686400);
"#,
    );
    assert_eq!(out, "+100000|+100000|-0005|-0005");
}

/// `date_create($datetime, $timezone)` and `date_create_immutable($datetime, $timezone)` now accept
/// the optional second `$timezone` argument, desugaring to the two-arg `DateTime`/
/// `DateTimeImmutable` constructors. The procedural aliases previously only accepted 0–1 args.
#[test]
fn test_date_create_with_timezone_arg() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = date_create("2024-06-15 12:00:00", new DateTimeZone("Europe/Paris"));
$im = date_create_immutable("2024-01-15 08:00:00", new DateTimeZone("America/New_York"));
echo $d->format("H:i"), " ", $d->getTimezone()->getName(), " ", $d->getTimestamp(), "|",
     $im->format("H:i"), " ", $im->getTimezone()->getName();
"#,
    );
    assert_eq!(out, "12:00 Europe/Paris 1718445600|08:00 America/New_York");
}

/// `date_time_set($datetime, $hour, $minute, $second, $microsecond)` now accepts the optional fifth
/// `$microsecond` argument, desugaring to `setTime(hour, minute, second, microsecond)` which already
/// supported it. The 3- and 4-arg forms remain valid.
#[test]
fn test_date_time_set_microsecond_arg() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = new DateTime("2024-06-15 10:00:00");
date_time_set($d, 12, 30, 45, 123456);
echo $d->format("H:i:s.u"), "|", $d->getMicrosecond();
"#,
    );
    assert_eq!(out, "12:30:45.123456|123456");
}

/// `date_diff($a, $b, $absolute)` now accepts the optional third `$absolute` argument: when true the
/// returned `DateInterval` is always positive (`invert` = 0) regardless of argument order. Without it,
/// `date_diff($a, $b)` where `$b` is earlier sets `invert` = 1.
#[test]
fn test_date_diff_absolute_arg() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = new DateTime("2020-01-01");
$b = new DateTime("2019-01-01");
$signed = date_diff($a, $b);
$absolute = date_diff($a, $b, true);
echo $signed->invert, " ", $signed->days, "|", $absolute->invert, " ", $absolute->days;
"#,
    );
    assert_eq!(out, "1 365|0 365");
}

/// Verifies the php-src `date_diff.phpt` workload keeps formatter temporaries bounded.
///
/// The original 172,800-iteration fixture exhausted the default 8 MiB heap because the
/// `date()` call used internally by `DateTime::format()` leaked two 48-byte blocks per diff.
/// This reduced loop retains the same diff/clone/format/new/add-sub ownership shape. The final
/// DateTime roots leave a small fixed heap-debug baseline, so the assertion bounds live blocks
/// independently of the 400 iterations rather than requiring an unrelated zero-root cleanup.
#[test]
fn test_date_diff_loop_reclaims_formatter_temporaries() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$dates[0] = new DateTime('2009-11-20');
$dates[1] = new DateTime('2009-12-20');
$ok = 0;
for ($i = 0; $i < 400; $i++) {
    $diff = date_diff($dates[0], $dates[1]);
    $current = clone $dates[0];
    $interval = new DateInterval($diff->format('P%yY%mM%dD'));
    if ($current > $dates[1]) {
        $current->sub($interval);
    } else {
        $current->add($interval);
    }
    if ($current == $dates[1]) {
        $ok++;
    }
}
echo $ok;
"#,
    );
    assert_eq!(
        output.stdout, "400",
        "unexpected date_diff result: {}",
        output.stderr
    );
    let summary = output
        .stderr
        .lines()
        .find(|line| line.starts_with("HEAP DEBUG: leak summary:"))
        .unwrap_or_else(|| panic!("missing heap-debug summary: {}", output.stderr));
    let live_blocks = if summary.ends_with("clean") {
        0
    } else {
        summary
            .split("live_blocks=")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| panic!("invalid heap-debug summary: {summary}"))
    };
    assert!(
        live_blocks <= 16,
        "date_diff loop retained {live_blocks} blocks after 400 iterations: {}",
        output.stderr,
    );
}

// --- procedural-alias behavioral coverage (audit gaps) ---
// The aliases below were already desugared by the name resolver and exercised only through
// `function_exists()` smoke checks; these tests pin their runtime behavior against PHP.

/// `date_create_immutable()` returns a `DateTimeImmutable`: `modify()` returns a NEW object and
/// leaves the original untouched — the defining behavioral difference from the mutable `DateTime`.
/// The procedural alias desugars to `new DateTimeImmutable(...)`.
#[test]
fn test_date_create_immutable_is_immutable() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = date_create_immutable("2024-01-01 10:00:00");
$d2 = $d->modify("+1 day");
echo $d->format("Y-m-d"), "|", $d2->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2024-01-01|2024-01-02");
}

/// `date_modify($datetime, $modifier)` mutates the object in place and returns the same `DateTime`
/// instance (the procedural alias desugars to `DateTime::modify`), matching PHP's chainable return.
#[test]
fn test_date_modify_returns_same_object() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = date_create("2024-01-01");
$m = date_modify($d, "+1 month");
echo $d->format("Y-m-d"), "|", ($m === $d ? "same" : "diff");
"#,
    );
    assert_eq!(out, "2024-02-01|same");
}

/// `date_timestamp_set($datetime, $ts)` desugars to `DateTime::setTimestamp($ts)`, replacing the
/// underlying Unix timestamp (interpreted in the object's timezone) and returning the object.
#[test]
fn test_date_timestamp_set() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = date_create("2024-01-01");
date_timestamp_set($d, 1700000000);
echo $d->format("Y-m-d H:i:s"), "|", $d->getTimestamp();
"#,
    );
    assert_eq!(out, "2023-11-14 22:13:20|1700000000");
}

/// `timezone_transitions_get($tz)` is the procedural alias for `DateTimeZone::getTransitions()`.
/// For UTC (a fixed-offset zone with no DST history) PHP returns exactly one transition spanning
/// the full `PHP_INT_MIN`..`PHP_INT_MAX` range: `ts = PHP_INT_MIN`, `offset = 0`, `isdst = false`,
/// `abbr = "UTC"`. elephc's embedded tzdata matches this bit-for-bit.
#[test]
fn test_timezone_transitions_get() {
    let out = compile_and_run(
        r#"<?php
$tz = new DateTimeZone("UTC");
$t = timezone_transitions_get($tz);
$f = $t[0];
echo count($t), "|", $f["ts"], "|", $f["offset"], "|", ($f["isdst"] ? "1" : "0"), "|", $f["abbr"];
"#,
    );
    assert_eq!(out, "1|-9223372036854775808|0|0|UTC");
}

/// Verifies `DateTime` and `DateTimeImmutable` share the last parse-error state and preserve the
/// php-src byte position/message for a truncated numeric field.
#[test]
fn test_datetime_get_last_errors_shared_between_classes() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
DateTimeImmutable::createFromFormat("Y-m-d", "2024-01");
$e = DateTime::getLastErrors();
echo $e["error_count"], "|", $e["errors"]["7"];
"#,
    );
    assert_eq!(out, "1|Not enough data available to satisfy format");
}

/// Regression: `createFromTimestamp()` keeps the fractional second as microseconds (PHP 8.4),
/// using `floor()` for the whole-second part so negative fractional timestamps round toward -inf.
#[test]
fn test_datetime_create_from_timestamp_microseconds() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = DateTimeImmutable::createFromTimestamp(1.9);
$b = DateTimeImmutable::createFromTimestamp(-1.5);
$c = DateTimeImmutable::createFromTimestamp(2);
echo $a->getTimestamp(), ":", $a->format("u"), "|";
echo $b->getTimestamp(), ":", $b->format("u"), "|";
echo $c->getTimestamp(), ":", $c->format("u");
"#,
    );
    assert_eq!(out, "1:900000|-2:500000|2:000000");
}

/// Regression: `createFromTimestamp()` rejects non-finite/out-of-range floats with php-src's
/// `DateRangeError` wording and late-static-binds both successful and failing inherited calls.
#[test]
fn test_datetime_create_from_timestamp_range_and_late_static_binding() {
    let out = compile_and_run(
        r#"<?php
class ChildDateTime extends DateTime {
    public function __construct() { throw new Exception("constructor called"); }
}
try {
    DateTime::createFromTimestamp(NAN);
} catch (DateRangeError $e) {
    echo $e->getMessage(), "|";
}
try {
    DateTimeImmutable::createFromTimestamp(9.223372036854776E+18);
} catch (DateRangeError $e) {
    echo $e->getMessage(), "|";
}
try {
    ChildDateTime::createFromTimestamp(INF);
} catch (DateRangeError $e) {
    echo $e->getMessage(), "|";
}
echo get_class(ChildDateTime::createFromTimestamp(0));
"#,
    );
    assert_eq!(
        out,
        "DateTime::createFromTimestamp(): Argument #1 ($timestamp) must be a finite number between \
-9223372036854775808 and 9223372036854775807.999999, NAN given|\
DateTimeImmutable::createFromTimestamp(): Argument #1 ($timestamp) must be a finite number between \
-9223372036854775808 and 9223372036854775807.999999, 9.22337e+18 given|\
ChildDateTime::createFromTimestamp(): Argument #1 ($timestamp) must be a finite number between \
-9223372036854775808 and 9223372036854775807.999999, INF given|ChildDateTime"
    );
}

/// Regression: `DateInterval::__construct()` requires a leading `P`; a missing or lowercase `P`
/// throws `DateMalformedIntervalStringException`, matching PHP. Well-formed input still parses.
#[test]
fn test_date_interval_requires_leading_p() {
    let out = compile_and_run(
        r#"<?php
$r = "";
try { $x = new DateInterval("1Y"); $r .= "a"; } catch (DateMalformedIntervalStringException $e) { $r .= "t"; }
try { $x = new DateInterval("p1y"); $r .= "a"; } catch (DateMalformedIntervalStringException $e) { $r .= "t"; }
try { $x = new DateInterval(""); $r .= "a"; } catch (DateMalformedIntervalStringException $e) { $r .= "t"; }
$ok = new DateInterval("P1Y2M3DT4H");
$r .= "|" . $ok->y . $ok->m . $ok->d . $ok->h;
echo $r;
"#,
    );
    assert_eq!(out, "ttt|1234");
}

/// Regression: `DateTimeInterface::diff()` uses PHP's parameter name `$targetObject`, so the
/// named-argument form resolves correctly.
#[test]
fn test_datetime_diff_named_argument() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = new DateTime("2020-01-01");
$b = new DateTime("2020-01-10");
echo $a->diff(targetObject: $b)->days, "|", $a->diff(targetObject: $b, absolute: true)->days;
"#,
    );
    assert_eq!(out, "9|9");
}

/// Regression: `DatePeriod::createFromISO8601String()` accepts the optional `int $options`
/// argument (PHP 8.4) and honours `EXCLUDE_START_DATE`, dropping the start element.
#[test]
fn test_date_period_create_from_iso8601_options() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = DatePeriod::createFromISO8601String("R3/2020-01-01T00:00:00Z/P1D", DatePeriod::EXCLUDE_START_DATE);
$n = 0;
foreach ($p as $d) { $n++; }
echo $n;
"#,
    );
    assert_eq!(out, "3");
}

/// Regression: `getdate()` and `localtime()` default to UTC (PHP's default timezone) when no
/// `date_default_timezone_set()` has run, instead of using the host's local time. Timestamp 0 must
/// decompose to 1970-01-01 00:00:00 UTC.
#[test]
fn test_getdate_localtime_default_utc() {
    let out = compile_and_run(
        r#"<?php
$g = getdate(0);
$l = localtime(0, true);
echo $g["year"], "-", $g["mon"], "-", $g["mday"], " ", $g["hours"], ":", $g["minutes"];
echo "|", ($l["tm_year"] + 1900), "-", $l["tm_hour"];
"#,
    );
    assert_eq!(out, "1970-1-1 0:0|1970-0");
}

/// Regression: the instant-key comparison rewrite is restricted to non-nullable
/// `DateTime`/`DateTimeImmutable` operands (so it never reads `->timestamp`/`->microsecond` off a
/// possible `null`). This guards that ordinary non-nullable DateTime comparisons still order by the
/// absolute instant after that restriction.
#[test]
fn test_datetime_instant_comparison_non_nullable() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = new DateTime("2020-01-01");
$b = new DateTime("2020-01-02");
echo ($a < $b) ? "1" : "0";
echo ($a > $b) ? "1" : "0";
echo ($a == $a) ? "1" : "0";
echo ($a <=> $b);
"#,
    );
    assert_eq!(out, "101-1");
}

/// G10: `new DateTime("totoro")` throws `DateMalformedStringException` (PHP 8.3+).
#[test]
fn test_datetime_invalid_string_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $d = new DateTime("totoro");
    echo "no-throw";
} catch (DateMalformedStringException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// G10b: `new DateTimeImmutable("totoro")` throws `DateMalformedStringException` (PHP 8.3+).
#[test]
fn test_datetime_immutable_invalid_string_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $d = new DateTimeImmutable("totoro");
    echo "no-throw";
} catch (DateMalformedStringException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// G9: `new DateTimeZone("garbage")` throws `DateInvalidTimeZoneException` (PHP 8.3+).
#[test]
fn test_datetimezone_invalid_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $tz = new DateTimeZone("garbage");
    echo "no-throw";
} catch (DateInvalidTimeZoneException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies PHP canonicalizes compact numeric zones while preserving their wall-clock/offset
/// semantics (including second precision and the optional `GMT` prefix), rejects normalized
/// offsets of 100 hours or more, and resolves abbreviation zones without changing their public
/// name.
#[test]
fn test_datetimezone_offset_valid() {
    let out = compile_and_run(
        r#"<?php
$tz = new DateTimeZone("+0200");
$d = new DateTime("2024-01-01 12:00:00", $tz);
$a = new DateTime("2024-07-15 12:00:00", new DateTimeZone("CEST"));
$seconds = new DateTimeZone("GMT+02:30:45");
$sd = new DateTime("2024-01-01 12:00:00", $seconds);
$zero = new DateTimeZone("-000000");
try {
    new DateTimeZone("+9999");
    $bad = "N";
} catch (DateInvalidTimeZoneException $e) {
    $bad = "Y";
}
echo $tz->getName(), "|", $d->format("H:i P"), "|", $d->getOffset(), "|",
     $a->getTimezone()->getName(), "|", $a->format("H:i P T"), "|",
     $seconds->getName(), "|", $sd->format("H:i:s P"), "|", $sd->getOffset(), "|",
     $zero->getName(), "|", $bad;
"#,
    );
    assert_eq!(
        out,
        "+02:00|12:00 +02:00|7200|CEST|12:00 +02:00 CEST|+02:30:45|12:00:00 +02:30|9045|+00:00|Y"
    );
}

/// G24: `DateTime::createFromTimestamp(float)` preserves the fractional part as microseconds.
#[test]
fn test_create_from_timestamp_float_keeps_micros() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
echo DateTime::createFromTimestamp(1700000000.123456)->format("u");
"#,
    );
    assert_eq!(out, "123456");
}

/// G25: `DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY)` without a country code throws
/// `ValueError` (PHP 8.0+). Reinforces the existing per-country test with the bare no-code path.
#[test]
fn test_list_identifiers_per_country_no_code_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $x = DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY);
    echo "no-throw";
} catch (ValueError $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// G19c: the `date_create_from_format` procedural alias returns `false` (not a throw) when the
/// subject fails to match the format, matching `DateTime::createFromFormat`.
#[test]
fn test_date_create_from_format_invalid_returns_false() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$bad = date_create_from_format("Y-m-d", "not-a-date");
echo ($bad === false) ? "false" : "other";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies `createFromDateString()` exposes `from_string` through serialization state.
#[test]
fn test_dateinterval_from_string_serialization_state() {
    let out = compile_and_run(
        r#"<?php
$iv = DateInterval::createFromDateString("2 days");
$state = $iv->__serialize();
echo $state["from_string"] ? "true" : "false";
"#,
    );
    assert_eq!(out, "true");
}

/// Verifies PHP's two exclusive `DateInterval::__serialize()` shapes: relative-string intervals
/// expose only their source metadata, while ISO intervals have no `date_string` key.
#[test]
fn test_dateinterval_date_string_serialization_state() {
    let out = compile_and_run(
        r#"<?php
$a = DateInterval::createFromDateString("2 days");
$as = $a->__serialize();
echo $as["date_string"], "|", ($as["from_string"] ? "t" : "f"), "|";
$b = new DateInterval("P1Y");
$bs = $b->__serialize();
echo (isset($bs["date_string"]) ? "Y" : "N"), "|", ($bs["from_string"] ? "t" : "f");
"#,
    );
    assert_eq!(out, "2 days|t|N|f");
}

/// G6: `DatePeriod::getEndDate()` return type is `?DateTimeInterface` — the returned object satisfies
/// `instanceof DateTimeInterface` (it is a `DateTime`, which implements the interface).
#[test]
fn test_dateperiod_get_end_date_returns_interface() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), new DateTime("2024-01-04"));
$e = $p->getEndDate();
echo ($e instanceof DateTimeInterface) ? "iface" : "no";
"#,
    );
    assert_eq!(out, "iface");
}

/// G7: `DatePeriod::getStartDate()` return type is `DateTimeInterface` — the returned object
/// satisfies `instanceof DateTimeInterface`.
#[test]
fn test_dateperiod_get_start_date_returns_interface() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), new DateTime("2024-01-04"));
$s = $p->getStartDate();
echo ($s instanceof DateTimeInterface) ? "iface" : "no";
"#,
    );
    assert_eq!(out, "iface");
}

/// G8: `DatePeriod` implements `IteratorAggregate` (PHP 5.3+, formally since 8.0) in addition to
/// `Iterator`. `instanceof IteratorAggregate` must be true.
#[test]
fn test_dateperiod_instanceof_iterator_aggregate() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), new DateTime("2024-01-04"));
echo ($p instanceof IteratorAggregate) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// G4: `DatePeriod::$start` exposes the start instant (PHP 8.2+ public readonly virtual property).
#[test]
fn test_dateperiod_start_property() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$start = new DateTime("2024-01-01");
$p = new DatePeriod($start, new DateInterval("P1D"), new DateTime("2024-01-04"));
echo $p->start->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2024-01-01");
}

/// G4: `DatePeriod::$end` exposes the end instant (null in the recurrence-count form).
#[test]
fn test_dateperiod_end_property() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), new DateTime("2024-01-04"));
echo $p->end->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2024-01-04");
}

/// G4: `DatePeriod::$interval` exposes the step interval.
#[test]
fn test_dateperiod_interval_property() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), new DateTime("2024-01-04"));
echo $p->interval->format("%d");
"#,
    );
    assert_eq!(out, "1");
}

/// G4: `DatePeriod::$current` reflects the live cursor during iteration.
#[test]
fn test_dateperiod_current_property() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), new DateTime("2024-01-04"));
foreach ($p as $d) {
    echo $p->current->format("Y-m-d"), "|";
}
"#,
    );
    assert_eq!(out, "2024-01-01|2024-01-02|2024-01-03|");
}

/// Verifies PHP's distinction between the public minimum-yield count and `getRecurrences()`.
#[test]
fn test_dateperiod_recurrences_property() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$s = new DateTime("2024-01-01");
$i = new DateInterval("P1D");
foreach ([0, DatePeriod::EXCLUDE_START_DATE, DatePeriod::INCLUDE_END_DATE,
          DatePeriod::EXCLUDE_START_DATE | DatePeriod::INCLUDE_END_DATE] as $option) {
    $p = new DatePeriod($s, $i, 3, $option);
    echo $p->recurrences, ":", $p->getRecurrences(), "|";
}
$end = new DateTime("2024-01-04");
foreach ([0, DatePeriod::EXCLUDE_START_DATE, DatePeriod::INCLUDE_END_DATE,
          DatePeriod::EXCLUDE_START_DATE | DatePeriod::INCLUDE_END_DATE] as $option) {
    $p = new DatePeriod($s, $i, $end, $option);
    echo $p->recurrences, ":", ($p->getRecurrences() === null ? "N" : "X"), "|";
}
"#,
    );
    assert_eq!(out, "4:3|3:3|5:3|4:3|1:N|0:N|2:N|1:N|");
}

/// G4: `DatePeriod::$include_start_date` / `$include_end_date` reflect the option flags.
#[test]
fn test_dateperiod_include_start_end_date_property() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(
    new DateTime("2024-01-01"),
    new DateInterval("P1D"),
    new DateTime("2024-01-04"),
    DatePeriod::EXCLUDE_START_DATE | DatePeriod::INCLUDE_END_DATE,
);
echo $p->include_start_date ? "1" : "0", "|", $p->include_end_date ? "1" : "0";
"#,
    );
    assert_eq!(out, "0|1");
}

/// G13: `DateTime::diff()` produces a `DateInterval` whose `$days` is the whole-day total (int),
/// and `format("%a")` renders that total. Directly constructed intervals keep `days === false`.
#[test]
fn test_diff_days_is_int() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = new DateTime("2020-01-01");
$b = new DateTime("2021-03-15");
$d = $a->diff($b);
echo $d->days, "|", $d->format("%a");
"#,
    );
    assert_eq!(out, "439|439");
}

/// G13: a directly constructed `DateInterval` has `days === false`, and `format("%a")` renders
/// `(unknown)`, matching PHP.
#[test]
fn test_diff_format_a_unknown_when_days_false() {
    let out = compile_and_run(
        r#"<?php
$iv = new DateInterval("P2W");
echo ($iv->days === false) ? "F" : "T", "|", $iv->format("%a");
"#,
    );
    assert_eq!(out, "F|(unknown)");
}

/// R3/R4: `DateTimeZone::getTransitions()` with no arguments reproduces PHP's full transition list
/// — row 0 `ts = PHP_INT_MIN` (= `i64::MIN` on 64-bit), the count for the bundled tz data, and the
/// expanded-year `time` format. Non-regression for the bridge's `format_utc_iso` formatter.
#[test]
fn test_get_transitions_row0_ts_php_int_min() {
    let out = compile_and_run(
        r#"<?php
$t = (new DateTimeZone("Europe/Paris"))->getTransitions();
echo $t[0]["ts"], "\n", $t[0]["time"];
"#,
    );
    assert_eq!(
        out,
        "-9223372036854775808\n-292277022657-01-27T08:29:52+00:00"
    );
}

/// Verifies transition rows preserve php-src's observable associative-key insertion order.
#[test]
fn test_get_transitions_row_key_order() {
    let out = compile_and_run(
        r#"<?php
$transition = (new DateTimeZone("Europe/Paris"))->getTransitions()[0];
echo implode(",", array_keys($transition));
"#,
    );
    assert_eq!(out, "ts,time,offset,isdst,abbr");
}

/// G12: `DateTime::__serialize()` returns the PHP-shaped array with `date`, `timezone_type`, and
/// `timezone` keys.
#[test]
fn test_datetime_serialize() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = new DateTime("2024-01-01 12:00:00.5", new DateTimeZone("Europe/Paris"));
$a = $d->__serialize();
echo $a["date"], "|", $a["timezone_type"], "|", $a["timezone"];
"#,
    );
    assert_eq!(out, "2024-01-01 12:00:00.500000|3|Europe/Paris");
}

/// Verifies serialization preserves php-src's timezone representation discriminator:
/// numeric offsets are type 1, abbreviations type 2, and database identifiers type 3.
#[test]
fn test_datetime_serialize_timezone_types() {
    let out = compile_and_run(
        r#"<?php
$offset = (new DateTime("2024-01-01 12:00:00+02:00"))->__serialize();
$abbr = (new DateTime("2024-01-01 12:00:00 CET"))->__serialize();
$identifier = (new DateTime("2024-01-01", new DateTimeZone("UTC")))->__serialize();
echo $offset["timezone_type"], "|", $offset["timezone"], "|";
echo $abbr["timezone_type"], "|", $abbr["timezone"], "|";
echo $identifier["timezone_type"], "|", $identifier["timezone"];
"#,
    );
    assert_eq!(out, "1|+02:00|2|CET|3|UTC");
}

/// Verifies `var_dump()` uses php-src's DateTime/DateTimeImmutable object-handler shape,
/// including the virtual date, timezone type, and timezone fields.
#[test]
fn test_datetime_var_dump_php_src_shape() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
var_dump(new DateTime("2024-01-02 03:04:05.123456", new DateTimeZone("UTC")));
var_dump(new DateTimeImmutable("2024-01-02 03:04:05 CET"));
"#,
    );
    assert!(out.starts_with("object(DateTime)#"), "unexpected DateTime dump: {out}");
    assert!(
        out.contains(concat!(
            " (3) {\n",
            "  [\"date\"]=>\n",
            "  string(26) \"2024-01-02 03:04:05.123456\"\n",
            "  [\"timezone_type\"]=>\n",
            "  int(3)\n",
            "  [\"timezone\"]=>\n",
            "  string(3) \"UTC\"\n",
            "}\n",
            "object(DateTimeImmutable)#",
        )),
        "unexpected date object dump shape: {out}"
    );
    assert!(
        out.ends_with(concat!(
            " (3) {\n",
            "  [\"date\"]=>\n",
            "  string(26) \"2024-01-02 03:04:05.000000\"\n",
            "  [\"timezone_type\"]=>\n",
            "  int(2)\n",
            "  [\"timezone\"]=>\n",
            "  string(3) \"CET\"\n",
            "}\n",
        )),
        "unexpected DateTimeImmutable dump: {out}"
    );
}

/// Verifies the remaining ext/date object handlers expose php-src's exact debug field names,
/// counts, scalar types, and nested date/interval renderers.
#[test]
fn test_datetimezone_interval_and_period_var_dump_php_src_shapes() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
var_dump(new DateTimeZone("CET"));
var_dump(new DateInterval("P1DT2H"));
var_dump(DateInterval::createFromDateString("2 days"));
var_dump(new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), 2));
"#,
    );
    assert!(
        out.starts_with(concat!(
            "object(DateTimeZone)#",
        )),
        "unexpected DateTimeZone dump: {out}"
    );
    assert!(
        out.contains(concat!(
            " (2) {\n",
            "  [\"timezone_type\"]=>\n",
            "  int(2)\n",
            "  [\"timezone\"]=>\n",
            "  string(3) \"CET\"\n",
            "}\n",
            "object(DateInterval)#",
        )),
        "unexpected DateTimeZone fields: {out}"
    );
    assert!(
        out.contains(concat!(
            " (10) {\n",
            "  [\"y\"]=>\n",
            "  int(0)\n",
            "  [\"m\"]=>\n",
            "  int(0)\n",
            "  [\"d\"]=>\n",
            "  int(1)\n",
            "  [\"h\"]=>\n",
            "  int(2)\n",
        )),
        "unexpected component DateInterval fields: {out}"
    );
    assert!(
        out.contains(concat!(
            " (2) {\n",
            "  [\"from_string\"]=>\n",
            "  bool(true)\n",
            "  [\"date_string\"]=>\n",
            "  string(6) \"2 days\"\n",
            "}\n",
            "object(DatePeriod)#",
        )),
        "unexpected relative DateInterval fields: {out}"
    );
    assert!(
        out.contains(concat!(
            " (7) {\n",
            "  [\"start\"]=>\n",
            "  object(DateTime)#",
        )),
        "unexpected DatePeriod start field: {out}"
    );
    assert!(
        out.ends_with(concat!(
            "  [\"recurrences\"]=>\n",
            "  int(3)\n",
            "  [\"include_start_date\"]=>\n",
            "  bool(true)\n",
            "  [\"include_end_date\"]=>\n",
            "  bool(false)\n",
            "}\n",
        )),
        "unexpected DatePeriod tail: {out}"
    );
}

/// Verifies ext/date subclasses inherit php-src's special object debug handlers while retaining
/// their runtime subclass name instead of exposing Elephc's private storage properties.
#[test]
fn test_datetime_subclass_var_dump_uses_inherited_php_src_shape() {
    let out = compile_and_run(
        r#"<?php
class DebugDate extends DateTime { private int $privateValue = 1; protected $protectedValue = 2; }
class DebugZone extends DateTimeZone { public $publicValue = 3; }
class DebugInterval extends DateInterval { public $publicValue = 4; }
class DebugPeriod extends DatePeriod { public $publicValue = 5; }
var_dump(DebugDate::createFromTimestamp(0));
var_dump(new DebugZone("UTC"));
var_dump(new DebugInterval("P1D"));
var_dump(new DebugPeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), 1));
"#,
    );
    for class_name in ["DebugDate", "DebugZone", "DebugInterval", "DebugPeriod"] {
        assert!(
            out.contains(&format!("object({class_name})#")),
            "missing inherited debug handler for {class_name}: {out}"
        );
    }
    assert!(
        !out.contains("__elephc_initialized")
            && !out.contains("timestamp\":\"DateTime")
            && !out.contains("_from_string\":\"DateInterval"),
        "private runtime storage leaked through subclass var_dump: {out}"
    );
    assert!(
        out.contains(concat!(
            " (5) {\n",
            "  [\"privateValue\":\"DebugDate\":private]=>\n",
            "  int(1)\n",
            "  [\"protectedValue\":protected]=>\n",
            "  int(2)\n",
        )) && out.contains(concat!(
            " (3) {\n",
            "  [\"publicValue\"]=>\n",
            "  int(3)\n",
        )) && out.contains(concat!(
            " (11) {\n",
            "  [\"publicValue\"]=>\n",
            "  int(4)\n",
        )) && out.contains(concat!(
            " (8) {\n",
            "  [\"publicValue\"]=>\n",
            "  int(5)\n",
        )),
        "user-declared ext/date properties were not rendered before virtual fields: {out}"
    );
}

/// Verifies recursive array dumps retain php-src's virtual DateTime fields and indentation.
#[test]
fn test_datetime_var_dump_nested_array_php_src_shape() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$timezone = new DateTimeZone("UTC");
$value = new DateTime("2024-01-02 03:04:05.123456", $timezone);
var_dump([$value]);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "array(1) {\n",
            "  [0]=>\n",
            "  object(DateTime)#2 (3) {\n",
            "    [\"date\"]=>\n",
            "    string(26) \"2024-01-02 03:04:05.123456\"\n",
            "    [\"timezone_type\"]=>\n",
            "    int(3)\n",
            "    [\"timezone\"]=>\n",
            "    string(3) \"UTC\"\n",
            "  }\n",
            "}\n",
        )
    );
}

/// Verifies DatePeriod snapshots its constructor objects independently and renders their shapes.
#[test]
fn test_dateperiod_clones_constructor_objects_independently() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$start = new DateTime("2024-01-01");
$interval = new DateInterval("P1D");
$end = new DateTime("2024-01-05");
$period = new DatePeriod($start, $interval, $end);
echo ($period->start === $start ? "S" : "s"),
     ($period->interval === $interval ? "I" : "i"),
     ($period->end === $end ? "E" : "e"), "|";
$start->modify("+1 day");
$interval->d = 9;
$end->modify("+1 day");
echo $period->start->format("Y-m-d"), "|", $period->interval->d, "|",
     $period->end->format("Y-m-d"), "\n";
var_dump([$period]);
"#,
    );
    assert!(
        out.starts_with("sie|2024-01-01|1|2024-01-05\n"),
        "DatePeriod did not preserve independent constructor snapshots: {out}"
    );
    assert!(
        out.contains("object(DatePeriod)#4 (7) {\n")
            && out.matches("object(DateTime)#").count() == 2
            && out.contains("object(DateInterval)#")
            && out.contains("[\"start\"]=>\n")
            && out.contains("[\"end\"]=>\n")
            && out.contains("[\"interval\"]=>\n"),
        "DatePeriod recursive debug shape diverged from php-src: {out}"
    );
}

/// Verifies php-src allocates an inline DatePeriod before its constructor arguments.
#[test]
fn test_dateperiod_inline_constructor_allocation_order_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
var_dump([
    new DatePeriod(
        new DateTime("2024-01-01"),
        new DateInterval("P1D"),
        new DateTime("2024-01-05")
    )
]);
"#,
    );
    assert!(
        out.contains("object(DatePeriod)#1 (7) {\n")
            && out.contains("[\"start\"]=>\n    object(DateTime)#4 (3) {\n")
            && out.contains("[\"end\"]=>\n    object(DateTime)#3 (3) {\n")
            && out.contains("[\"interval\"]=>\n    object(DateInterval)#2 (10) {\n"),
        "inline DatePeriod allocation order diverged from php-src: {out}"
    );
}

/// Verifies a runtime-selected DatePeriod class is also allocated before its constructor args.
#[test]
fn test_dateperiod_dynamic_constructor_allocation_order_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$class = DatePeriod::class;
var_dump([
    new $class(
        new DateTime("2024-01-01"),
        new DateInterval("P1D"),
        new DateTime("2024-01-05")
    )
]);
"#,
    );
    assert!(
        out.contains("object(DatePeriod)#1 (7) {\n")
            && out.contains("[\"start\"]=>\n    object(DateTime)#4 (3) {\n")
            && out.contains("[\"end\"]=>\n    object(DateTime)#3 (3) {\n")
            && out.contains("[\"interval\"]=>\n    object(DateInterval)#2 (10) {\n"),
        "dynamic DatePeriod allocation order diverged from php-src: {out}"
    );
}

/// Verifies dynamic ext/date construction keeps case-insensitive names, leading slashes,
/// named arguments, omitted defaults, and each concrete result type aligned with php-src.
#[test]
fn test_dynamic_datetime_constructors_preserve_php_argument_semantics() {
    let out = compile_and_run(
        r#"<?php
$class = "datetime";
$date = new $class(timezone: new DateTimeZone("UTC"));
echo get_class($date), "|", $date->getTimezone()->getName(), "\n";

$class = "\\DATETIMEIMMUTABLE";
$immutable = new $class();
echo get_class($immutable), "\n";

$class = "datetimezone";
$zone = new $class("Europe/Paris");
echo get_class($zone), "|", $zone->getName(), "\n";

$class = "dateinterval";
$interval = new $class(duration: "P2D");
echo get_class($interval), "|", $interval->d, "\n";

$class = "dateperiod";
$period = new $class(
    start: new DateTime("2024-01-01"),
    interval: new DateInterval("P1D"),
    end: new DateTime("2024-01-03")
);
$property = "recurrences";
$startProperty = "start";
$intervalProperty = "interval";
echo get_class($period), "|", $period->recurrences, "|", $period->$property, "|";
echo get_class($period->$startProperty), "|", get_class($period->$intervalProperty);
"#,
    );
    assert_eq!(
        out,
        "DateTime|UTC\nDateTimeImmutable\nDateTimeZone|Europe/Paris\nDateInterval|2\nDatePeriod|1|1|DateTime|DateInterval"
    );
}

/// Verifies runtime indexed/associative spreads use concrete ext/date signatures after
/// allocating the selected object, including DatePeriod's php-src object-handle order.
#[test]
fn test_dynamic_datetime_constructor_spreads_preserve_php_src_semantics() {
    let out = compile_and_run(
        r#"<?php
$class = "datetime";
$dateArgs = ["timezone" => new DateTimeZone("UTC")];
$date = new $class(...$dateArgs);
echo $date->getTimezone()->getName(), "\n";

$class = "dateinterval";
$intervalArgs = ["P2DT3H"];
$interval = new $class(...$intervalArgs);
echo $interval->d, "|", $interval->h, "\n";

$class = "dateperiod";
function dynamic_period_args(): array {
    return [
        new DateTime("2024-01-01"),
        new DateInterval("P1D"),
        new DateTime("2024-01-03"),
    ];
}
$period = new $class(...dynamic_period_args());
echo $period->recurrences;
"#,
    );
    assert_eq!(out, "UTC\n2|3\n1");

    let allocation_out = compile_and_run(
        r#"<?php
$class = "dateperiod";
function dynamic_period_allocation_args(): array {
    return [
        new DateTime("2024-01-01"),
        new DateInterval("P1D"),
        new DateTime("2024-01-03"),
    ];
}
var_dump([new $class(...dynamic_period_allocation_args())]);
"#,
    );
    assert!(
        allocation_out.contains("object(DatePeriod)#1 (7) {\n")
            && allocation_out.contains("[\"start\"]=>\n    object(DateTime)#4 (3) {\n")
            && allocation_out.contains("[\"end\"]=>\n    object(DateTime)#3 (3) {\n")
            && allocation_out.contains("[\"interval\"]=>\n    object(DateInterval)#2 (10) {\n"),
        "dynamic spread DatePeriod allocation order diverged from php-src: {allocation_out}"
    );
}

/// Verifies dynamic DatePeriod spreads reject invalid overloads with php-src's TypeError.
#[test]
fn test_dynamic_dateperiod_spreads_validate_runtime_overload_types() {
    let out = compile_and_run(
        r#"<?php
function report_invalid_period(string $class, array $args): void {
    try {
        new $class(...$args);
    } catch (Throwable $error) {
        echo get_class($error), ": ", $error->getMessage(), "\n";
    }
}
$class = "DatePeriod";
report_invalid_period($class, [42, new DateInterval("P1D"), 2]);
report_invalid_period($class, [new DateTime("2024-01-01"), 42, 2]);
report_invalid_period($class, [new DateTime("2024-01-01"), new DateInterval("P1D"), new stdClass()]);
report_invalid_period($class, [new DateTime("2024-01-01"), new DateInterval("P1D"), 2, 0, 99]);
"#,
    );
    let error =
        "TypeError: DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or \
(DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments\n";
    assert_eq!(
        out,
        error.repeat(4)
    );
}

/// Verifies dynamic ext/date spreads retain php-src's runtime parameter type diagnostics.
#[test]
fn test_dynamic_datetime_spreads_validate_runtime_parameter_types() {
    let out = compile_and_run(
        r#"<?php
function report_invalid_dynamic_date(string $class, array $args): void {
    try {
        @new $class(...$args);
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
report_invalid_dynamic_date("DateTime", ["now", new stdClass()]);
report_invalid_dynamic_date("DateTime", [[]]);
report_invalid_dynamic_date("DateTime", ["now", 42]);
report_invalid_dynamic_date("DateTimeZone", [[]]);
report_invalid_dynamic_date("DateInterval", [[]]);
report_invalid_dynamic_date(
    "DatePeriod",
    [new DateTime("2024-01-01"), new DateInterval("P1D"), 2, new stdClass()]
);
report_invalid_dynamic_date(
    "DatePeriod",
    [new DateTime("2024-01-01"), new DateInterval("P1D"), "not numeric"]
);
"#,
    );
    assert_eq!(
        out,
        "TypeError:DateTime::__construct(): Argument #2 ($timezone) must be of type ?DateTimeZone, stdClass given\n\
TypeError:DateTime::__construct(): Argument #1 ($datetime) must be of type string, array given\n\
TypeError:DateTime::__construct(): Argument #2 ($timezone) must be of type ?DateTimeZone, int given\n\
TypeError:DateTimeZone::__construct(): Argument #1 ($timezone) must be of type string, array given\n\
TypeError:DateInterval::__construct(): Argument #1 ($duration) must be of type string, array given\n\
TypeError:DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or \
(DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments\n\
TypeError:DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or \
(DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments\n"
    );
}

/// Verifies DatePeriod's weak scalar recurrence overload coercions match php-src.
#[test]
fn test_dynamic_dateperiod_spreads_coerce_scalar_recurrences() {
    let out = compile_and_run(
        r#"<?php
$class = "DatePeriod";
$start = new DateTime("2024-01-01");
$interval = new DateInterval("P1D");
foreach ([2.7, "2", true] as $end) {
    $period = @new $class(...[$start, $interval, $end]);
    echo $period->recurrences, "|";
}
try {
    @new $class(...[$start, $interval, null]);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "3|3|2|DateMalformedPeriodStringException:DatePeriod::__construct(): Recurrence count must be greater or equal to 1 and lower than 2147483640"
    );
}

/// Verifies ordinary dynamic ext/date calls keep php-src's internal-constructor arity errors.
#[test]
fn test_dynamic_datetime_constructors_validate_runtime_arity() {
    let out = compile_and_run(
        r#"<?php
function report_invalid_date_constructor(string $class, array $args): void {
    try {
        if (count($args) === 0) {
            new $class();
        } elseif (count($args) === 2) {
            new $class($args[0], $args[1]);
        } else {
            new $class($args[0], $args[1], $args[2]);
        }
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
report_invalid_date_constructor("DateTime", ["now", new DateTimeZone("UTC"), 3]);
report_invalid_date_constructor("DateTimeZone", []);
report_invalid_date_constructor("DateInterval", ["P1D", 2]);
report_invalid_date_constructor("DatePeriod", []);
"#,
    );
    assert_eq!(
        out,
        "ArgumentCountError:DateTime::__construct() expects at most 2 arguments, 3 given\n\
ArgumentCountError:DateTimeZone::__construct() expects exactly 1 argument, 0 given\n\
ArgumentCountError:DateInterval::__construct() expects exactly 1 argument, 2 given\n\
TypeError:DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or \
(DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments\n"
    );
}

/// Verifies statically named ext/date constructors raise catchable php-src runtime diagnostics.
#[test]
fn test_static_datetime_constructors_validate_runtime_arity_and_overloads() {
    let out = compile_and_run(
        r#"<?php
try {
    new DateTime("now", null, 3);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    new DateTimeImmutable("now", null, 3);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    new DateInterval();
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    new DatePeriod(new DateTime("2024-01-01"));
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    new DatePeriod("R1/2024-01-01/P1D", "bad");
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "ArgumentCountError:DateTime::__construct() expects at most 2 arguments, 3 given\n\
ArgumentCountError:DateTimeImmutable::__construct() expects at most 2 arguments, 3 given\n\
ArgumentCountError:DateInterval::__construct() expects exactly 1 argument, 0 given\n\
TypeError:DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or \
(DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments\n\
TypeError:DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or \
(DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments\n"
    );
}

/// Verifies runtime-selected DatePeriod construction retains the deprecated string overload.
#[test]
fn test_dynamic_dateperiod_constructor_supports_deprecated_string_overload() {
    let out = compile_and_run(
        r#"<?php
function dynamic_period_specification(): string {
    $sideEffectObject = new stdClass();
    echo spl_object_id($sideEffectObject), "|";
    return "R2/2024-01-01T00:00:00Z/P1D";
}
$class = "DatePeriod";
$period = @new $class(dynamic_period_specification());
var_dump([$period]);
function dynamic_period_string_args(): array {
    return ["R2/2024-01-01T00:00:00Z/P1D"];
}
$spreadPeriod = @new $class(...dynamic_period_string_args());
echo get_class($spreadPeriod), "|", $spreadPeriod->recurrences;
"#,
    );
    assert!(
        out.starts_with("2|array(1) {\n")
            && out.contains("[0]=>\n  object(DatePeriod)#1 (7) {\n")
            && out.contains("[\"recurrences\"]=>\n    int(3)\n")
            && out.ends_with("DatePeriod|3"),
        "dynamic DatePeriod string allocation order or state diverged from php-src: {out}"
    );
}

/// Verifies a DatePeriod subclass inherits the deprecated string constructor overload while a
/// subclass that declares its own constructor remains on the ordinary userland constructor path.
#[test]
fn test_dateperiod_subclass_inherits_string_constructor_overload() {
    let out = compile_and_run(
        r#"<?php
class InheritedDatePeriod extends DatePeriod {}
class OwnDatePeriodConstructor extends DatePeriod {
    public function __construct() {
        echo "own|";
    }
}
$period = @new InheritedDatePeriod("R2/2012-07-01T00:00:00Z/P7D");
foreach ($period as $index => $date) {
    echo $index, ":", $date->format("Y-m-d"), "|";
}
$own = new OwnDatePeriodConstructor();
echo get_class($own);
"#,
    );
    assert_eq!(
        out,
        "0:2012-07-01|1:2012-07-08|2:2012-07-15|own|OwnDatePeriodConstructor"
    );
}

/// Verifies DatePeriod applies its reflected parameter names before selecting the string overload.
#[test]
fn test_dateperiod_string_constructor_named_arguments_match_php_src() {
    let out = compile_and_run(
        r#"<?php
$specification = "R2/2024-01-01T00:00:00Z/P1D";
echo (@new DatePeriod(start: $specification))->recurrences, "|";
echo (@new DatePeriod(start: $specification, interval: "0"))->recurrences, "|";
echo (@new DatePeriod(...["start" => $specification, "interval" => 0]))->recurrences, "\n";
foreach ([
    fn() => new DatePeriod(isostr: $specification),
    fn() => new DatePeriod(start: $specification, options: 0),
    fn() => new DatePeriod(interval: 0),
] as $construct) {
    try {
        $construct();
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
$class = DatePeriod::class;
echo (@new $class(start: $specification, interval: 0))->recurrences, "\n";
$runtimeStringArguments = ["start" => $specification, "interval" => 0];
echo (@new DatePeriod(...$runtimeStringArguments))->recurrences, "|";
$runtimeObjectTail = [
    "interval" => new DateInterval("P1D"),
    "end" => new DateTime("2024-01-03"),
];
echo (new DatePeriod(new DateTime("2024-01-01"), ...$runtimeObjectTail))->recurrences, "\n";
function report_runtime_named_period(array $runtimeArguments): void {
    try {
        new DatePeriod(...$runtimeArguments);
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
report_runtime_named_period(["isostr" => $specification]);
report_runtime_named_period(["start" => $specification, 0 => 0]);
report_runtime_named_period([0 => $specification, "start" => $specification]);
"#,
    );
    assert_eq!(
        out,
        "3|3|3\n\
Error:Unknown named parameter $isostr\n\
ArgumentCountError:DatePeriod::__construct(): Argument #2 ($interval) must be passed explicitly, because the default value is not known\n\
ArgumentCountError:DatePeriod::__construct(): Argument #1 ($start) not passed\n\
3\n\
3|1\n\
Error:Unknown named parameter $isostr\n\
Error:Cannot use positional argument after named argument during unpacking\n\
Error:Named parameter $start overwrites previous argument\n"
    );
}

/// Verifies DatePeriod combines multiple runtime unpacks before overload dispatch.
#[test]
fn test_dateperiod_multiple_runtime_spreads_match_php_src() {
    let out = compile_and_run(
        r#"<?php
$specification = "R2/2024-01-01T00:00:00Z/P1D";
$isoHead = [$specification];
$zeroTail = [0];
echo (@new DatePeriod(...$isoHead, ...$zeroTail))->recurrences, "|";
$objectHead = [new DateTime("2024-01-01"), new DateInterval("P1D")];
$objectEnd = [new DateTime("2024-01-04")];
echo count(iterator_to_array(new DatePeriod(...$objectHead, ...$objectEnd))), "|";
echo count(iterator_to_array(
    new DatePeriod(...$objectHead, end: new DateTime("2024-01-04"))
)), "|";
$namedHead = ["start" => new DateTime("2024-01-01")];
$namedTail = [
    "interval" => new DateInterval("P1D"),
    "end" => new DateTime("2024-01-04"),
];
echo count(iterator_to_array(new DatePeriod(...$namedHead, ...$namedTail))), "|";
echo (@new DatePeriod(...$isoHead, interval: "0"))->recurrences, "|";
$class = DatePeriod::class;
echo (@new $class(...$isoHead, ...$zeroTail))->recurrences, "\n";
foreach ([
    fn() => new DatePeriod(...$isoHead, options: 0),
    fn() => new DatePeriod(
        ...$namedHead,
        ...["start" => new DateTime("2024-01-02")]
    ),
] as $construct) {
    try {
        $construct();
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "3|3|3|3|3|3\n\
ArgumentCountError:DatePeriod::__construct(): Argument #2 ($interval) must be passed explicitly, because the default value is not known\n\
Error:Named parameter $start overwrites previous argument\n"
    );
}

/// Verifies every preallocated ext/date constructor supports multiple runtime unpacks.
#[test]
fn test_datetime_family_multiple_runtime_spreads_match_php_src() {
    let out = compile_and_run(
        r#"<?php
$dateHead = ["2024-01-02 03:04:05"];
$timezoneTail = [new DateTimeZone("UTC")];
echo (new DateTime(...$dateHead, ...$timezoneTail))->format("Y-m-d H:i:s e"), "|";
echo (new DateTimeImmutable(...$dateHead, ...$timezoneTail))->format("Y-m-d H:i:s e"), "|";
$empty = [];
$zoneTail = ["Europe/Paris"];
echo (new DateTimeZone(...$empty, ...$zoneTail))->getName(), "|";
$durationTail = ["P2D"];
echo (new DateInterval(...$empty, ...$durationTail))->format("%d"), "|";
echo (new DateTime(
    ...["datetime" => "2024-01-03"],
    timezone: new DateTimeZone("UTC")
))->format("Y-m-d e"), "\n";
foreach ([
    fn() => new DateTime(...$dateHead, datetime: "2024-01-03"),
    fn() => new DateTimeZone(...["UTC"], ...["Europe/Paris"]),
] as $construct) {
    try {
        $construct();
    } catch (Throwable $error) {
        echo get_class($error), ":", $error->getMessage(), "\n";
    }
}
foreach ([
    DateTime::class,
    DateTimeImmutable::class,
    DateTimeZone::class,
    DateInterval::class,
] as $class) {
    if ($class === DateTime::class || $class === DateTimeImmutable::class) {
        echo (new $class(...$dateHead, ...$timezoneTail))->format("Y-m-d"), "|";
    } elseif ($class === DateTimeZone::class) {
        echo (new $class(...$empty, ...$zoneTail))->getName(), "|";
    } else {
        echo (new $class(...$empty, ...$durationTail))->format("%d"), "|";
    }
}
function date_spread_chunk(string $label, array $value): array {
    echo $label;
    return $value;
}
function date_named_value(string $label, mixed $value): mixed {
    echo $label;
    return $value;
}
try {
    new DateTime(
        ...date_spread_chunk("A", ["unknown" => 1]),
        timezone: date_named_value("B", new DateTimeZone("UTC"))
    );
} catch (Throwable $error) {
    echo "\n", get_class($error), ":", $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "2024-01-02 03:04:05 UTC|2024-01-02 03:04:05 UTC|Europe/Paris|2|2024-01-03 UTC\n\
Error:Named parameter $datetime overwrites previous argument\n\
ArgumentCountError:DateTimeZone::__construct() expects exactly 1 argument, 2 given\n\
2024-01-02|2024-01-02|Europe/Paris|2|A\n\
Error:Unknown named parameter $unknown\n"
    );
}

/// Verifies the fixed DatePeriod string overload also allocates before argument evaluation.
#[test]
fn test_dateperiod_string_constructor_allocation_order_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
function fixed_period_specification(): string {
    $sideEffectObject = new stdClass();
    echo spl_object_id($sideEffectObject), "|";
    return "R2/2024-01-01T00:00:00Z/P1D";
}
$period = @new DatePeriod(fixed_period_specification());
var_dump([$period]);
"#,
    );
    assert!(
        out.starts_with("2|array(1) {\n")
            && out.contains("[0]=>\n  object(DatePeriod)#1 (7) {\n")
            && out.contains("[\"recurrences\"]=>\n    int(3)\n"),
        "fixed DatePeriod string allocation order or state diverged from php-src: {out}"
    );
}

/// Verifies DateTimeInterface checks see DateTime values stored in heterogeneous arrays.
#[test]
fn test_datetimeinterface_instanceof_heterogeneous_array_element() {
    let out = compile_and_run(
        r#"<?php
function date_values(): array {
    return [new DateTime("2024-01-01"), new DateInterval("P1D")];
}
$values = date_values();
var_dump($values[0] instanceof DateTimeInterface);
var_dump($values[1] instanceof DateInterval);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// Verifies DatePeriod's virtual object properties clone on every read and preserve full state.
#[test]
fn test_dateperiod_virtual_snapshots_preserve_timezone_microseconds_and_dst() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$start = new DateTime("2024-03-30 12:34:56.123456", new DateTimeZone("Europe/Paris"));
$end = new DateTimeImmutable("2024-04-01 12:34:56.123456", new DateTimeZone("Europe/Paris"));
$period = new DatePeriod(
    $start,
    new DateInterval("P1D"),
    $end,
    DatePeriod::INCLUDE_END_DATE
);
$first = $period->start;
$second = $period->start;
echo ($first === $second ? "aliased" : "fresh"), "|";
echo $period->start->format("Y-m-d H:i:s.u e"), "|";
$interval = $period->interval;
$interval->d = 9;
echo $period->interval->d, "|";
$getFirst = $period->getStartDate();
$getSecond = $period->getStartDate();
echo ($getFirst === $getSecond ? "aliased" : "fresh"), "|";
echo get_class($period->getEndDate()), "|";
foreach ($period as $date) {
    echo $date->format("Y-m-d H:i:s.u P"), ",";
}
echo "|", $period->current->format("Y-m-d H:i:s.u e");
"#,
    );
    assert_eq!(
        out,
        "fresh|2024-03-30 12:34:56.123456 Europe/Paris|1|fresh|DateTimeImmutable|\
2024-03-30 12:34:56.123456 +01:00,2024-03-31 12:34:56.123456 +02:00,\
2024-04-01 12:34:56.123456 +02:00,|2024-04-02 12:34:56.123456 Europe/Paris"
    );
}

/// G12: `DateTime::__unserialize()` reconstructs the object from the serialize array.
#[test]
fn test_datetime_unserialize() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = new DateTime();
$d->__unserialize(["date" => "2024-01-01 12:00:00.500000", "timezone_type" => 3, "timezone" => "Europe/Paris"]);
echo $d->format("Y-m-d H:i:s.u"), "|", $d->getTimezone()->getName();
"#,
    );
    assert_eq!(out, "2024-01-01 12:00:00.500000|Europe/Paris");
}

/// Replays php-src `date_time_fractions_serialize.phpt` and pins the restored object's handle:
/// unserialization must not expose internal reconstruction allocations before the returned object.
#[test]
fn test_datetime_unserialize_preserves_php_src_object_handle_order() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$date = new DateTime("2016-10-03 12:47:18.819313");
echo spl_object_id($date), "|";
$serialized = serialize($date);
echo spl_object_id($date), "\n";
$restored = unserialize($serialized);
var_dump($restored);
$next = new stdClass();
echo spl_object_id($next), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "1|1\n",
            "object(DateTime)#2 (3) {\n",
            "  [\"date\"]=>\n",
            "  string(26) \"2016-10-03 12:47:18.819313\"\n",
            "  [\"timezone_type\"]=>\n",
            "  int(3)\n",
            "  [\"timezone\"]=>\n",
            "  string(3) \"UTC\"\n",
            "}\n",
            "3\n",
        )
    );
}

/// Replays direct ext/date magic serialization calls: the declared generic `array` return must
/// retain its string-keyed hash representation, and fixed-offset restoration preserves wall time.
#[test]
fn test_datetime_direct_magic_serialization_uses_assoc_arrays_and_fixed_offsets() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$date = new DateTime("2022-04-14 11:27:42.541106");
echo serialize($date->__serialize()), "\n";
$date->__unserialize([
    "date" => "2022-04-14 11:27:42.541106",
    "timezone_type" => 1,
    "timezone" => "+0130",
]);
echo $date->format("Y-m-d H:i:s.u P"), "\n";
$zone = new DateTimeZone("CEST");
echo serialize($zone->__serialize()), "\n";
$zone->__unserialize(["timezone_type" => 1, "timezone" => "+0130"]);
echo $zone->getName();
"#,
    );
    assert_eq!(
        out,
        concat!(
            "a:3:{s:4:\"date\";s:26:\"2022-04-14 11:27:42.541106\";",
            "s:13:\"timezone_type\";i:3;s:8:\"timezone\";s:3:\"UTC\";}\n",
            "2022-04-14 11:27:42.541106 +01:30\n",
            "a:2:{s:13:\"timezone_type\";i:2;s:8:\"timezone\";s:4:\"CEST\";}\n",
            "+01:30",
        )
    );
}

/// Replays php-src `bug67308.phpt`: literal-only `unserialize()` programs must emit date magic
/// restoration hooks even when no source-level allocation or prior `serialize()` call exists.
#[test]
fn test_datetime_unserialize_old_fractionless_payload_without_prior_serialize() {
    let out = compile_and_run(
        r#"<?php
$old = unserialize('O:8:"DateTime":3:{s:4:"date";s:19:"2005-07-14 22:30:41";s:13:"timezone_type";i:3;s:8:"timezone";s:13:"Europe/London";}');
$new = unserialize('O:8:"DateTime":3:{s:4:"date";s:26:"2005-07-14 22:30:41.123456";s:13:"timezone_type";i:3;s:8:"timezone";s:13:"Europe/London";}');
echo $old->format("Y-m-d H:i:s.u e"), "\n", $new->format("Y-m-d H:i:s.u e");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "2005-07-14 22:30:41.000000 Europe/London\n",
            "2005-07-14 22:30:41.123456 Europe/London",
        )
    );
}

/// G12: `DateTime::__set_state()` reconstructs from an array (used by `var_export`).
#[test]
fn test_datetime_set_state() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = DateTime::__set_state(["date" => "2024-01-01 12:00:00.500000", "timezone_type" => 3, "timezone" => "Europe/Paris"]);
echo $d->format("Y-m-d H:i:s.u"), "|", $d->getTimezone()->getName();
"#,
    );
    assert_eq!(out, "2024-01-01 12:00:00.500000|Europe/Paris");
}

/// G12: `DateTimeImmutable::__serialize()` / `__unserialize()` / `__set_state()` round-trip.
#[test]
fn test_datetime_immutable_serialize_roundtrip() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$d = new DateTimeImmutable("2024-06-15 08:30:00", new DateTimeZone("America/New_York"));
$a = $d->__serialize();
$d2 = DateTimeImmutable::__set_state($a);
echo $d2->format("Y-m-d H:i:s"), "|", $d2->getTimezone()->getName();
"#,
    );
    assert_eq!(out, "2024-06-15 08:30:00|America/New_York");
}

/// G12: `DateTimeZone::__serialize()` / `__set_state()` round-trip.
#[test]
fn test_datetimezone_serialize_set_state() {
    let out = compile_and_run(
        r#"<?php
$tz = new DateTimeZone("Europe/Paris");
$a = $tz->__serialize();
$tz2 = DateTimeZone::__set_state($a);
echo $tz2->getName();
"#,
    );
    assert_eq!(out, "Europe/Paris");
}

/// G12: `DateInterval::__serialize()` returns all public properties as an array.
#[test]
fn test_dateinterval_serialize() {
    let out = compile_and_run(
        r#"<?php
$iv = new DateInterval("P1Y2M3DT4H5M6S");
$a = $iv->__serialize();
echo $a["y"], "|", $a["m"], "|", $a["d"], "|", $a["h"], "|", $a["i"], "|", $a["s"], "|",
     $a["f"], "|", $a["invert"], "|", ($a["days"] === false ? "F" : "T"), "|",
     ($a["from_string"] === false ? "F" : "T");
"#,
    );
    assert_eq!(out, "1|2|3|4|5|6|0|0|F|F");
}

/// Verifies `DateInterval::__set_state()` reconstructs both PHP serialization shapes.
#[test]
fn test_dateinterval_set_state() {
    let out = compile_and_run(
        r#"<?php
$iv = DateInterval::__set_state(["y"=>1,"m"=>2,"d"=>3,"h"=>4,"i"=>5,"s"=>6,"f"=>0,"invert"=>0,"days"=>false,"from_string"=>false]);
echo $iv->format("%y-%m-%d %h:%i:%s"), "|";
$relative = DateInterval::__set_state(["from_string"=>true,"date_string"=>"2 days"]);
echo $relative->format("%d"), "|", $relative->__serialize()["date_string"];
"#,
    );
    assert_eq!(out, "1-2-3 4:5:6|2|2 days");
}

/// Verifies missing `DateInterval` serialization fields receive php-src's sentinel defaults, while
/// a string `date_string` takes precedence over the advisory `from_string` field.
#[test]
fn test_dateinterval_partial_serialization_state_defaults() {
    let out = compile_and_run(
        r#"<?php
$empty = DateInterval::__set_state([])->__serialize();
echo $empty["y"], ",", $empty["m"], ",", $empty["d"], ",", $empty["h"], ",",
     $empty["i"], ",", $empty["s"], ",", $empty["f"], ",", $empty["invert"], ",",
     $empty["days"], ",", ($empty["from_string"] ? "T" : "F"), "|";
$partial = DateInterval::__set_state(["y" => 2, "from_string" => true])->__serialize();
echo $partial["y"], ",", $partial["m"], ",", ($partial["from_string"] ? "T" : "F"), "|";
$date = DateInterval::__set_state(["date_string" => "2023-01-16 17:01:19"])->__serialize();
echo ($date["from_string"] ? "T" : "F"), ",", $date["date_string"];
"#,
    );
    assert_eq!(
        out,
        "-1,-1,-1,-1,-1,-1,0,0,-1,F|2,-1,F|T,2023-01-16 17:01:19"
    );
}

/// Verifies `DateInterval` restoration applies php-src's field coercions, including the special
/// `days` treatment for strings, objects, numeric values, and `false`.
#[test]
fn test_dateinterval_serialization_state_field_coercions() {
    let out = compile_and_run(
        r#"<?php
$object = new stdClass();
foreach (["aoeu", $object, false, "42"] as $days) {
    $interval = DateInterval::__set_state(["y" => "3", "f" => "0.25", "days" => $days]);
    echo gettype($interval->y), ":", $interval->y, "|",
         gettype($interval->f), ":", $interval->f, "|",
         gettype($interval->days), ":";
    echo $interval->days === false ? "false" : $interval->days;
    echo "\n";
}
"#,
    );
    assert_eq!(
        out,
        "integer:3|double:0.25|integer:0\n\
integer:3|double:0.25|integer:-1\n\
integer:3|double:0.25|boolean:false\n\
integer:3|double:0.25|integer:42\n"
    );
}

/// Verifies DateInterval restoration reports php-src's lossy float-to-timelib conversion when
/// fractional seconds scaled to microseconds exceed the signed integer range.
#[test]
fn test_dateinterval_serialization_warns_on_fraction_overflow() {
    let out = compile_and_run_capture(
        r#"<?php
$interval = DateInterval::__set_state(["f" => 9999999999990]);
echo gettype($interval->f);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "double");
    assert!(
        out.stderr.contains(
            "Warning: The float 9.99999999999E+18 is not representable as an int, cast occurred"
        ),
        "missing DateInterval fraction-overflow warning: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains(" on line 1"),
        "missing DateInterval warning source location: {}",
        out.stderr
    );
}

/// Verifies DateInterval restoration reports php-src's exact timelib error and preserves relative
/// microseconds when rebuilding the hidden free-form representation.
#[test]
fn test_dateinterval_set_state_timelib_validation_and_fraction() {
    let out = compile_and_run(
        r#"<?php
$microseconds = DateInterval::__set_state(["date_string" => "500 microseconds"]);
echo $microseconds->format("%f"), "|";
try {
    DateInterval::__set_state(["date_string" => "2023-01-16-foobar$*"]);
} catch (Throwable $exception) {
    echo get_class($exception), ": ", $exception->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "500|Error: Unknown or bad format (2023-01-16-foobar$*) at position 10 (-) ",
            "while unserializing: Unexpected character",
        )
    );
}

/// Verifies `var_export()` emits parsable php-src `__set_state()` expressions for the ext/date
/// object family and that `eval()` reconstructs the exported value.
#[test]
fn test_datetime_var_export_set_state_eval_round_trip() {
    let out = compile_and_run(
        r#"<?php
$source = new DateTime("2017-10-06 23:30:00", new DateTimezone("UTC"));
$state = var_export($source, true);
echo $state, "\n---\n";
eval("\$restored = {$state};");
echo $restored->format("Y-m-d H:i:s.u"), "|", $restored->getTimezone()->getName();
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\\DateTime::__set_state(array(\n",
            "   'date' => '2017-10-06 23:30:00.000000',\n",
            "   'timezone_type' => 3,\n",
            "   'timezone' => 'UTC',\n",
            "))\n",
            "---\n",
            "2017-10-06 23:30:00.000000|UTC",
        )
    );
}

/// Verifies all date serialization hooks use php-src's declared `array`/`void` signatures.
#[test]
fn test_datetime_serialization_hook_signatures() {
    let out = compile_and_run(
        r#"<?php
$s = new ReflectionMethod(DateTime::class, "__serialize");
$u = new ReflectionMethod(DateTime::class, "__unserialize");
echo $s->getReturnType()->getName(), ":", $u->getParameters()[0]->getType()->getName(), ":",
     $u->getReturnType()->getName(), "|";
$s = new ReflectionMethod(DateTimeImmutable::class, "__serialize");
$u = new ReflectionMethod(DateTimeImmutable::class, "__unserialize");
echo $s->getReturnType()->getName(), ":", $u->getParameters()[0]->getType()->getName(), ":",
     $u->getReturnType()->getName(), "|";
$s = new ReflectionMethod(DateTimeZone::class, "__serialize");
$u = new ReflectionMethod(DateTimeZone::class, "__unserialize");
echo $s->getReturnType()->getName(), ":", $u->getParameters()[0]->getType()->getName(), ":",
     $u->getReturnType()->getName(), "|";
$s = new ReflectionMethod(DateInterval::class, "__serialize");
$u = new ReflectionMethod(DateInterval::class, "__unserialize");
echo $s->getReturnType()->getName(), ":", $u->getParameters()[0]->getType()->getName(), ":",
     $u->getReturnType()->getName(), "|";
$s = new ReflectionMethod(DatePeriod::class, "__serialize");
$u = new ReflectionMethod(DatePeriod::class, "__unserialize");
echo $s->getReturnType()->getName(), ":", $u->getParameters()[0]->getType()->getName(), ":",
     $u->getReturnType()->getName(), "|";
"#,
    );
    assert_eq!(
        out,
        "array:array:void|array:array:void|array:array:void|array:array:void|array:array:void|"
    );
}

/// Verifies ext/date's legacy method return contracts remain tentative in Reflection while
/// PHP 8.5's newer methods and serialization hooks expose ordinary declared return types.
#[test]
fn test_datetime_tentative_return_type_reflection() {
    let out = compile_and_run(
        r#"<?php
$legacy = new ReflectionMethod(DateTime::class, "format");
echo ($legacy->hasReturnType() ? "R" : "r"), ":",
     ($legacy->hasTentativeReturnType() ? "T" : "t"), ":",
     $legacy->getTentativeReturnType()->getName(), ":",
     ($legacy->getReturnType() === null ? "N" : "n"), "|";
$modern = new ReflectionMethod(DateTime::class, "getMicrosecond");
echo ($modern->hasReturnType() ? "R" : "r"), ":",
     ($modern->hasTentativeReturnType() ? "T" : "t"), ":",
     $modern->getReturnType()->getName(), ":",
     ($modern->getTentativeReturnType() === null ? "N" : "n"), "|";
$iterator = new ReflectionMethod(DatePeriod::class, "getIterator");
echo ($iterator->hasReturnType() ? "R" : "r"), ":",
     ($iterator->hasTentativeReturnType() ? "T" : "t"), ":",
     $iterator->getReturnType()->getName();
"#,
    );
    assert_eq!(out, "r:T:string:N|R:t:int:N|R:t:Iterator");
}

/// Verifies rewritten ext/date functions remain Reflection-visible with php-src parameter,
/// return-type, deprecation, and attribute-argument metadata.
#[test]
fn test_datetime_procedural_alias_reflection_metadata() {
    let out = compile_and_run(
        r#"<?php
$create = new ReflectionFunction("date_create");
echo $create->getParameters()[0]->getName(), ":",
     $create->getParameters()[0]->getDefaultValue(), ":",
     $create->getParameters()[1]->getType(), ":",
     $create->getReturnType(), "|";
$abbr = new ReflectionFunction("timezone_name_from_abbr");
echo $abbr->getNumberOfRequiredParameters(), ":",
     $abbr->getParameters()[1]->getName(), ":",
     $abbr->getParameters()[1]->getDefaultValue(), ":",
     $abbr->getReturnType(), "|";
$deprecated = new ReflectionFunction("strftime");
$attribute = $deprecated->getAttributes("Deprecated")[0];
echo ($deprecated->isInternal() ? "I" : "i"), ":",
     ($deprecated->isDeprecated() ? "D" : "d"), ":",
     $attribute->getArguments()["since"], ":",
     $attribute->getArguments()["message"];
"#,
    );
    assert_eq!(
        out,
        "datetime:now:?DateTimeZone:DateTime|false|1:utcOffset:-1:string|false|I:D:8.1:use IntlDateFormatter::format() instead"
    );
}

/// Verifies ext/date Reflection hides compiler helpers, preserves every php-src method in
/// declaration order/casing, and exposes the overloaded DatePeriod constructor as one required
/// untyped parameter.
#[test]
fn test_datetime_php_src_method_surface_and_constructor_metadata() {
    let methods = |class_name: &str| {
        compile_and_run_with_heap_size(
            &format!(
                "<?php\nforeach ((new ReflectionClass({class_name}::class))->getMethods() as $method) {{\n    echo $method->getName(), \",\";\n}}\n"
            ),
            67_108_864,
        )
    };
    assert_eq!(
        methods("DateTimeInterface"),
        "format,getTimezone,getOffset,getTimestamp,getMicrosecond,diff,__wakeup,__serialize,__unserialize,"
    );
    assert_eq!(
        methods("DateTime"),
        "__construct,__serialize,__unserialize,__wakeup,__set_state,createFromImmutable,createFromInterface,createFromFormat,createFromTimestamp,getLastErrors,format,modify,add,sub,getTimezone,setTimezone,getOffset,getMicrosecond,setTime,setDate,setISODate,setTimestamp,setMicrosecond,getTimestamp,diff,"
    );
    assert_eq!(
        methods("DateTimeImmutable"),
        "__construct,__serialize,__unserialize,__wakeup,__set_state,createFromFormat,createFromTimestamp,getLastErrors,format,getTimezone,getOffset,getTimestamp,getMicrosecond,diff,modify,add,sub,setTimezone,setTime,setDate,setISODate,setTimestamp,setMicrosecond,createFromMutable,createFromInterface,"
    );
    assert_eq!(
        methods("DateTimeZone"),
        "__construct,getName,getOffset,getTransitions,getLocation,listAbbreviations,listIdentifiers,__serialize,__unserialize,__wakeup,__set_state,"
    );
    assert_eq!(
        methods("DateInterval"),
        "__construct,createFromDateString,format,__serialize,__unserialize,__wakeup,__set_state,"
    );
    assert_eq!(
        methods("DatePeriod"),
        "createFromISO8601String,__construct,getStartDate,getEndDate,getDateInterval,getRecurrences,__serialize,__unserialize,__wakeup,__set_state,getIterator,"
    );

    let out = compile_and_run(
        r#"<?php
$period = new ReflectionMethod(DatePeriod::class, "__construct");
echo $period->getNumberOfRequiredParameters(), ":", $period->getNumberOfParameters(), ":";
foreach ($period->getParameters() as $parameter) {
    echo ($parameter->hasType() ? "T" : "-"),
         ($parameter->isOptional() ? "O" : "R"),
         ($parameter->isDefaultValueAvailable() ? "D" : "-"), ",";
}
echo "|";
$zone = new ReflectionMethod(DateTimeZone::class, "__construct");
echo $zone->getNumberOfRequiredParameters(), ":",
     ($zone->getParameters()[0]->isOptional() ? "O" : "R"), "|",
     ((new ReflectionClass(DateInterval::class))->hasMethod("__get") ? "leak" : "hidden");
"#,
    );
    assert_eq!(
        out,
        "1:4:-R-,-O-,-O-,-O-,|1:R|hidden"
    );
}

/// Verifies every php-src ext/date method signature, including declared versus tentative
/// returns and the complete parameter type/optionality/reference/variadic metadata.
#[test]
fn test_datetime_php_src_method_signature_inventory() {
    let classes = [
        "DateTimeInterface",
        "DateTime",
        "DateTimeImmutable",
        "DateTimeZone",
        "DateInterval",
        "DatePeriod",
    ];
    let probes = classes
        .iter()
        .map(|class_name| {
            format!(
                r#"$class = new ReflectionClass({class_name}::class);
foreach ($class->getMethods() as $method) {{
    echo "{class_name}::", $method->getName(), "=",
         $method->getNumberOfRequiredParameters(), "/",
         $method->getNumberOfParameters(), ":",
         $method->isStatic(), ":",
         $method->returnsReference(), ":",
         $method->getReturnType(), ":",
         $method->getTentativeReturnType(), ":";
    foreach ($method->getParameters() as $parameter) {{
        echo $parameter->getName(), "~",
             $parameter->getType(), "~",
             $parameter->isOptional(), "~",
             $parameter->isPassedByReference(), "~",
             $parameter->isVariadic(), ";";
    }}
    echo "\n";
}}"#
            )
        })
        .map(|probe| {
            compile_and_run_with_heap_size(&format!("<?php\n{probe}\n"), 134_217_728)
        })
        .collect::<String>();
    let out = probes;
    assert_eq!(
        out,
        r#"DateTimeInterface::format=1/1::::string:format~string~~~;
DateTimeInterface::getTimezone=0/0::::DateTimeZone|false:
DateTimeInterface::getOffset=0/0::::int:
DateTimeInterface::getTimestamp=0/0::::int:
DateTimeInterface::getMicrosecond=0/0:::int::
DateTimeInterface::diff=1/2::::DateInterval:targetObject~DateTimeInterface~~~;absolute~bool~1~~;
DateTimeInterface::__wakeup=0/0::::void:
DateTimeInterface::__serialize=0/0:::array::
DateTimeInterface::__unserialize=1/1:::void::data~array~~~;
DateTime::__construct=0/2:::::datetime~string~1~~;timezone~?DateTimeZone~1~~;
DateTime::__serialize=0/0:::array::
DateTime::__unserialize=1/1:::void::data~array~~~;
DateTime::__wakeup=0/0::::void:
DateTime::__set_state=1/1:1:::DateTime:array~array~~~;
DateTime::createFromImmutable=1/1:1:::static:object~DateTimeImmutable~~~;
DateTime::createFromInterface=1/1:1::DateTime::object~DateTimeInterface~~~;
DateTime::createFromFormat=2/3:1:::DateTime|false:format~string~~~;datetime~string~~~;timezone~?DateTimeZone~1~~;
DateTime::createFromTimestamp=1/1:1:::static:timestamp~int|float~~~;
DateTime::getLastErrors=0/0:1:::array|false:
DateTime::format=1/1::::string:format~string~~~;
DateTime::modify=1/1::::DateTime:modifier~string~~~;
DateTime::add=1/1::::DateTime:interval~DateInterval~~~;
DateTime::sub=1/1::::DateTime:interval~DateInterval~~~;
DateTime::getTimezone=0/0::::DateTimeZone|false:
DateTime::setTimezone=1/1::::DateTime:timezone~DateTimeZone~~~;
DateTime::getOffset=0/0::::int:
DateTime::getMicrosecond=0/0:::int::
DateTime::setTime=2/4::::DateTime:hour~int~~~;minute~int~~~;second~int~1~~;microsecond~int~1~~;
DateTime::setDate=3/3::::DateTime:year~int~~~;month~int~~~;day~int~~~;
DateTime::setISODate=2/3::::DateTime:year~int~~~;week~int~~~;dayOfWeek~int~1~~;
DateTime::setTimestamp=1/1::::DateTime:timestamp~int~~~;
DateTime::setMicrosecond=1/1:::static::microsecond~int~~~;
DateTime::getTimestamp=0/0::::int:
DateTime::diff=1/2::::DateInterval:targetObject~DateTimeInterface~~~;absolute~bool~1~~;
DateTimeImmutable::__construct=0/2:::::datetime~string~1~~;timezone~?DateTimeZone~1~~;
DateTimeImmutable::__serialize=0/0:::array::
DateTimeImmutable::__unserialize=1/1:::void::data~array~~~;
DateTimeImmutable::__wakeup=0/0::::void:
DateTimeImmutable::__set_state=1/1:1:::DateTimeImmutable:array~array~~~;
DateTimeImmutable::createFromFormat=2/3:1:::DateTimeImmutable|false:format~string~~~;datetime~string~~~;timezone~?DateTimeZone~1~~;
DateTimeImmutable::createFromTimestamp=1/1:1:::static:timestamp~int|float~~~;
DateTimeImmutable::getLastErrors=0/0:1:::array|false:
DateTimeImmutable::format=1/1::::string:format~string~~~;
DateTimeImmutable::getTimezone=0/0::::DateTimeZone|false:
DateTimeImmutable::getOffset=0/0::::int:
DateTimeImmutable::getTimestamp=0/0::::int:
DateTimeImmutable::getMicrosecond=0/0:::int::
DateTimeImmutable::diff=1/2::::DateInterval:targetObject~DateTimeInterface~~~;absolute~bool~1~~;
DateTimeImmutable::modify=1/1::::DateTimeImmutable:modifier~string~~~;
DateTimeImmutable::add=1/1::::DateTimeImmutable:interval~DateInterval~~~;
DateTimeImmutable::sub=1/1::::DateTimeImmutable:interval~DateInterval~~~;
DateTimeImmutable::setTimezone=1/1::::DateTimeImmutable:timezone~DateTimeZone~~~;
DateTimeImmutable::setTime=2/4::::DateTimeImmutable:hour~int~~~;minute~int~~~;second~int~1~~;microsecond~int~1~~;
DateTimeImmutable::setDate=3/3::::DateTimeImmutable:year~int~~~;month~int~~~;day~int~~~;
DateTimeImmutable::setISODate=2/3::::DateTimeImmutable:year~int~~~;week~int~~~;dayOfWeek~int~1~~;
DateTimeImmutable::setTimestamp=1/1::::DateTimeImmutable:timestamp~int~~~;
DateTimeImmutable::setMicrosecond=1/1:::static::microsecond~int~~~;
DateTimeImmutable::createFromMutable=1/1:1:::static:object~DateTime~~~;
DateTimeImmutable::createFromInterface=1/1:1::DateTimeImmutable::object~DateTimeInterface~~~;
DateTimeZone::__construct=1/1:::::timezone~string~~~;
DateTimeZone::getName=0/0::::string:
DateTimeZone::getOffset=1/1::::int:datetime~DateTimeInterface~~~;
DateTimeZone::getTransitions=0/2::::array|false:timestampBegin~int~1~~;timestampEnd~int~1~~;
DateTimeZone::getLocation=0/0::::array|false:
DateTimeZone::listAbbreviations=0/0:1:::array:
DateTimeZone::listIdentifiers=0/2:1:::array:timezoneGroup~int~1~~;countryCode~?string~1~~;
DateTimeZone::__serialize=0/0:::array::
DateTimeZone::__unserialize=1/1:::void::data~array~~~;
DateTimeZone::__wakeup=0/0::::void:
DateTimeZone::__set_state=1/1:1:::DateTimeZone:array~array~~~;
DateInterval::__construct=1/1:::::duration~string~~~;
DateInterval::createFromDateString=1/1:1:::DateInterval:datetime~string~~~;
DateInterval::format=1/1::::string:format~string~~~;
DateInterval::__serialize=0/0:::array::
DateInterval::__unserialize=1/1:::void::data~array~~~;
DateInterval::__wakeup=0/0::::void:
DateInterval::__set_state=1/1:1:::DateInterval:array~array~~~;
DatePeriod::createFromISO8601String=1/2:1::static::specification~string~~~;options~int~1~~;
DatePeriod::__construct=1/4:::::start~~~~;interval~~1~~;end~~1~~;options~~1~~;
DatePeriod::getStartDate=0/0::::DateTimeInterface:
DatePeriod::getEndDate=0/0::::?DateTimeInterface:
DatePeriod::getDateInterval=0/0::::DateInterval:
DatePeriod::getRecurrences=0/0::::?int:
DatePeriod::__serialize=0/0:::array::
DatePeriod::__unserialize=1/1:::void::data~array~~~;
DatePeriod::__wakeup=0/0::::void:
DatePeriod::__set_state=1/1:1:::DatePeriod:array~array~~~;
DatePeriod::getIterator=0/0:::Iterator::
"#
    );
}

/// Verifies every php-src ext/date procedural signature, including parameter names/types,
/// optionality, by-reference/variadic flags, and declared return types.
#[test]
fn test_datetime_php_src_function_signature_inventory() {
    let functions = [
        "strtotime", "date", "idate", "gmdate", "mktime", "gmmktime", "checkdate",
        "strftime", "gmstrftime", "time", "localtime", "getdate", "date_create",
        "date_create_immutable", "date_create_from_format",
        "date_create_immutable_from_format", "date_parse", "date_parse_from_format",
        "date_get_last_errors", "date_format", "date_modify", "date_add", "date_sub",
        "date_timezone_get", "date_timezone_set", "date_offset_get", "date_diff",
        "date_time_set", "date_date_set", "date_isodate_set", "date_timestamp_set",
        "date_timestamp_get", "timezone_open", "timezone_name_get",
        "timezone_name_from_abbr", "timezone_offset_get", "timezone_transitions_get",
        "timezone_location_get", "timezone_identifiers_list",
        "timezone_abbreviations_list", "timezone_version_get",
        "date_interval_create_from_date_string", "date_interval_format",
        "date_default_timezone_set", "date_default_timezone_get", "date_sunrise",
        "date_sunset", "date_sun_info",
    ];
    let probes = functions
        .iter()
        .map(|name| {
            format!(
                r#"$function = new ReflectionFunction("{name}");
echo "{name}=", $function->getNumberOfRequiredParameters(), "/",
     $function->getNumberOfParameters(), ":",
     ($function->hasReturnType() ? (string)$function->getReturnType() : "-"), ":";
foreach ($function->getParameters() as $parameter) {{
    echo $parameter->getName(), "~",
         ($parameter->hasType() ? (string)$parameter->getType() : "-"), "~",
         ($parameter->isOptional() ? "O" : "R"), "~",
         ($parameter->isPassedByReference() ? "&" : "-"), "~",
         ($parameter->isVariadic() ? "V" : "-"), ";";
}}
echo "\n";"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let out = compile_and_run_with_heap_size(&format!("<?php\n{probes}\n"), 134_217_728);
    assert_eq!(
        out,
        r#"strtotime=1/2:int|false:datetime~string~R~-~-;baseTimestamp~?int~O~-~-;
date=1/2:string:format~string~R~-~-;timestamp~?int~O~-~-;
idate=1/2:int|false:format~string~R~-~-;timestamp~?int~O~-~-;
gmdate=1/2:string:format~string~R~-~-;timestamp~?int~O~-~-;
mktime=1/6:int|false:hour~int~R~-~-;minute~?int~O~-~-;second~?int~O~-~-;month~?int~O~-~-;day~?int~O~-~-;year~?int~O~-~-;
gmmktime=1/6:int|false:hour~int~R~-~-;minute~?int~O~-~-;second~?int~O~-~-;month~?int~O~-~-;day~?int~O~-~-;year~?int~O~-~-;
checkdate=3/3:bool:month~int~R~-~-;day~int~R~-~-;year~int~R~-~-;
strftime=1/2:string|false:format~string~R~-~-;timestamp~?int~O~-~-;
gmstrftime=1/2:string|false:format~string~R~-~-;timestamp~?int~O~-~-;
time=0/0:int:
localtime=0/2:array:timestamp~?int~O~-~-;associative~bool~O~-~-;
getdate=0/1:array:timestamp~?int~O~-~-;
date_create=0/2:DateTime|false:datetime~string~O~-~-;timezone~?DateTimeZone~O~-~-;
date_create_immutable=0/2:DateTimeImmutable|false:datetime~string~O~-~-;timezone~?DateTimeZone~O~-~-;
date_create_from_format=2/3:DateTime|false:format~string~R~-~-;datetime~string~R~-~-;timezone~?DateTimeZone~O~-~-;
date_create_immutable_from_format=2/3:DateTimeImmutable|false:format~string~R~-~-;datetime~string~R~-~-;timezone~?DateTimeZone~O~-~-;
date_parse=1/1:array:datetime~string~R~-~-;
date_parse_from_format=2/2:array:format~string~R~-~-;datetime~string~R~-~-;
date_get_last_errors=0/0:array|false:
date_format=2/2:string:object~DateTimeInterface~R~-~-;format~string~R~-~-;
date_modify=2/2:DateTime|false:object~DateTime~R~-~-;modifier~string~R~-~-;
date_add=2/2:DateTime:object~DateTime~R~-~-;interval~DateInterval~R~-~-;
date_sub=2/2:DateTime:object~DateTime~R~-~-;interval~DateInterval~R~-~-;
date_timezone_get=1/1:DateTimeZone|false:object~DateTimeInterface~R~-~-;
date_timezone_set=2/2:DateTime:object~DateTime~R~-~-;timezone~DateTimeZone~R~-~-;
date_offset_get=1/1:int:object~DateTimeInterface~R~-~-;
date_diff=2/3:DateInterval:baseObject~DateTimeInterface~R~-~-;targetObject~DateTimeInterface~R~-~-;absolute~bool~O~-~-;
date_time_set=3/5:DateTime:object~DateTime~R~-~-;hour~int~R~-~-;minute~int~R~-~-;second~int~O~-~-;microsecond~int~O~-~-;
date_date_set=4/4:DateTime:object~DateTime~R~-~-;year~int~R~-~-;month~int~R~-~-;day~int~R~-~-;
date_isodate_set=3/4:DateTime:object~DateTime~R~-~-;year~int~R~-~-;week~int~R~-~-;dayOfWeek~int~O~-~-;
date_timestamp_set=2/2:DateTime:object~DateTime~R~-~-;timestamp~int~R~-~-;
date_timestamp_get=1/1:int:object~DateTimeInterface~R~-~-;
timezone_open=1/1:DateTimeZone|false:timezone~string~R~-~-;
timezone_name_get=1/1:string:object~DateTimeZone~R~-~-;
timezone_name_from_abbr=1/3:string|false:abbr~string~R~-~-;utcOffset~int~O~-~-;isDST~int~O~-~-;
timezone_offset_get=2/2:int:object~DateTimeZone~R~-~-;datetime~DateTimeInterface~R~-~-;
timezone_transitions_get=1/3:array|false:object~DateTimeZone~R~-~-;timestampBegin~int~O~-~-;timestampEnd~int~O~-~-;
timezone_location_get=1/1:array|false:object~DateTimeZone~R~-~-;
timezone_identifiers_list=0/2:array:timezoneGroup~int~O~-~-;countryCode~?string~O~-~-;
timezone_abbreviations_list=0/0:array:
timezone_version_get=0/0:string:
date_interval_create_from_date_string=1/1:DateInterval|false:datetime~string~R~-~-;
date_interval_format=2/2:string:object~DateInterval~R~-~-;format~string~R~-~-;
date_default_timezone_set=1/1:bool:timezoneId~string~R~-~-;
date_default_timezone_get=0/0:string:
date_sunrise=1/6:string|int|float|false:timestamp~int~R~-~-;returnFormat~int~O~-~-;latitude~?float~O~-~-;longitude~?float~O~-~-;zenith~?float~O~-~-;utcOffset~?float~O~-~-;
date_sunset=1/6:string|int|float|false:timestamp~int~R~-~-;returnFormat~int~O~-~-;latitude~?float~O~-~-;longitude~?float~O~-~-;zenith~?float~O~-~-;utcOffset~?float~O~-~-;
date_sun_info=3/3:array:timestamp~int~R~-~-;latitude~float~R~-~-;longitude~float~R~-~-;
"#
    );
}

/// Verifies ext/date class reflection exposes only php-src's virtual DatePeriod properties and
/// that storage/helper members remain absent from PHP property and callable probes.
#[test]
fn test_datetime_php_src_property_and_helper_surface() {
    let out = compile_and_run(
        r#"<?php
echo "DateInterval=", count((new ReflectionClass(DateInterval::class))->getProperties()), "|";
echo "DatePeriod=", count((new ReflectionClass(DatePeriod::class))->getProperties());
"#,
    );
    assert_eq!(
        out,
        "DateInterval=0|DatePeriod=7"
    );
}

/// Verifies date/time implementation storage is absent from class-level Reflection properties.
#[test]
fn test_datetime_storage_properties_are_hidden_from_reflection() {
    let out = compile_and_run(
        r#"<?php
echo count((new ReflectionClass(DateTime::class))->getProperties());
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies DateTimeImmutable storage is absent from class-level Reflection properties.
#[test]
fn test_datetimeimmutable_storage_properties_are_hidden_from_reflection() {
    let out = compile_and_run(
        r#"<?php
echo count((new ReflectionClass(DateTimeImmutable::class))->getProperties());
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies DateTimeZone storage is absent from class-level Reflection properties.
#[test]
fn test_datetimezone_storage_properties_are_hidden_from_reflection() {
    let out = compile_and_run(
        r#"<?php
echo count((new ReflectionClass(DateTimeZone::class))->getProperties());
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies `property_exists()` follows php-src's virtual date-property rules without exposing
/// compiler storage slots.
#[test]
fn test_datetime_property_exists_php_src_surface() {
    let out = compile_and_run(
        r#"<?php
$interval = new DateInterval("P1D");
echo property_exists(DateInterval::class, "y") ? "Y" : "n", ":";
echo property_exists($interval, "y") ? "Y" : "n", ":";
echo property_exists($interval, "_from_string") ? "leak" : "hidden";
"#,
    );
    assert_eq!(out, "n:Y:hidden");

    let out = compile_and_run(
        r#"<?php
$period = new DatePeriod(
    new DateTimeImmutable("2024-01-01"),
    new DateInterval("P1D"),
    1
);
echo property_exists(DatePeriod::class, "start") ? "Y" : "n", ":";
echo property_exists($period, "start") ? "Y" : "n", ":";
echo property_exists($period, "startTs") ? "leak" : "hidden";
"#,
    );
    assert_eq!(out, "Y:Y:hidden");
}

/// Verifies `ReflectionObject(DateInterval)` follows php-src's state-dependent dynamic property
/// list and marks those entries as untyped, non-default dynamic properties.
#[test]
fn test_dateinterval_reflection_object_dynamic_properties() {
    let out = compile_and_run(
        r#"<?php
foreach ([new DateInterval("P1D"), DateInterval::createFromDateString("1 day")] as $interval) {
    foreach ((new ReflectionObject($interval))->getProperties() as $property) {
        echo $property->getName(), ":",
             $property->getModifiers(), ":",
             ($property->hasType() ? "T" : "-"), ":",
             ($property->isDefault() ? "D" : "-"), ":",
             ($property->isDynamic() ? "Y" : "n"), ",";
    }
    echo "|";
}
"#,
    );
    assert_eq!(
        out,
        "y:1:-:-:Y,m:1:-:-:Y,d:1:-:-:Y,h:1:-:-:Y,i:1:-:-:Y,s:1:-:-:Y,\
f:1:-:-:Y,invert:1:-:-:Y,days:1:-:-:Y,from_string:1:-:-:Y,|\
from_string:1:-:-:Y,date_string:1:-:-:Y,|"
    );
}

/// Verifies php-src's constant-default names and interface-declared DateTime format constants.
#[test]
fn test_datetime_reflection_constant_defaults_and_declaring_interface() {
    let out = compile_and_run(
        r#"<?php
$group = (new ReflectionFunction("timezone_identifiers_list"))->getParameters()[0];
$begin = (new ReflectionFunction("timezone_transitions_get"))->getParameters()[1];
$sun = (new ReflectionFunction("date_sunrise"))->getParameters()[1];
echo ($group->isDefaultValueConstant() ? "C" : "-"), ":",
     $group->getDefaultValueConstantName(), ":", $group->getDefaultValue(), "|";
echo ($begin->isDefaultValueConstant() ? "C" : "-"), ":",
     $begin->getDefaultValueConstantName(), ":", $begin->getDefaultValue(), "|";
echo ($sun->isDefaultValueConstant() ? "C" : "-"), ":",
     $sun->getDefaultValueConstantName(), ":", $sun->getDefaultValue(), "|";
echo (new ReflectionClassConstant(DateTime::class, "ATOM"))
    ->getDeclaringClass()->getName();
"#,
    );
    assert_eq!(
        out,
        "C:DateTimeZone::ALL:2047|C:PHP_INT_MIN:-9223372036854775808|\
C:SUNFUNCS_RET_STRING:1|DateTimeInterface"
    );
}

/// Verifies DateTimeZone serialization retains php-src's offset, abbreviation, and identifier
/// discriminators instead of flattening every zone to type 3.
#[test]
fn test_datetimezone_serialization_preserves_zone_type() {
    let out = compile_and_run(
        r#"<?php
foreach (["+02:00", "CET", "GMT", "UTC", "EST5EDT", "CST6CDT", "NZ-CHAT"] as $name) {
    $state = (new DateTimeZone($name))->__serialize();
    echo $state["timezone_type"], ":", $state["timezone"], "|";
}
"#,
    );
    assert_eq!(
        out,
        "1:+02:00|2:CET|2:GMT|3:UTC|3:EST5EDT|3:CST6CDT|3:NZ-CHAT|"
    );
}

/// Verifies military timezone letters retain php-src's sign, timestamp, display name, and
/// `date_parse_from_format()` offset for both positive (`A`) and negative (`X`) zones.
#[test]
fn test_datetime_military_timezone_exact_offsets() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
foreach (["A", "X"] as $zone) {
    $date = new DateTime("2024-01-01T12:00:00" . $zone);
    $state = $date->__serialize();
    echo $date->format("P T e"), ":", $date->getTimestamp(), ":",
         $state["timezone_type"], ":", $state["timezone"], "|";
}
$a = date_parse_from_format("Y-m-d T", "2024-01-01 A");
$x = date_parse_from_format("Y-m-d T", "2024-01-01 X");
echo $a["zone_type"], ":", $a["zone"], ":", $a["tz_abbr"], "|",
     $x["zone_type"], ":", $x["zone"], ":", $x["tz_abbr"];
"#,
    );
    assert_eq!(
        out,
        "+01:00 A A:1704106800:2:A|-11:00 X X:1704150000:2:X|2:3600:A|2:-39600:X"
    );
}

/// Verifies object and procedural add/sub surfaces report php-src's exact runtime TypeError,
/// including the original argument position and rejected debug type.
#[test]
fn test_datetime_add_sub_interval_type_errors() {
    let out = compile_and_run(
        r#"<?php
try {
    (new DateTime("2024-01-01"))->add(false);
} catch (TypeError $error) {
    echo $error->getMessage(), "|";
}
try {
    (new DateTimeImmutable("2024-01-01"))->sub(1.5);
} catch (TypeError $error) {
    echo $error->getMessage(), "|";
}
try {
    date_add(new DateTime("2024-01-01"), null);
} catch (TypeError $error) {
    echo $error->getMessage(), "|";
}
try {
    date_sub(new DateTime("2024-01-01"), "P1D");
} catch (TypeError $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "DateTime::add(): Argument #1 ($interval) must be of type DateInterval, false given|\
DateTimeImmutable::sub(): Argument #1 ($interval) must be of type DateInterval, float given|\
date_add(): Argument #2 ($interval) must be of type DateInterval, null given|\
date_sub(): Argument #2 ($interval) must be of type DateInterval, string given"
    );
}

/// Verifies the nullable timestamp components declared by php-src are accepted on direct calls,
/// with explicit `null` selecting the same current-time defaults as omission.
#[test]
fn test_datetime_php_src_nullable_timestamp_arguments() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
echo (date("Y", null) === date("Y") ? "D" : "d"), "|",
     (gmdate("Y", null) === gmdate("Y") ? "G" : "g"), "|",
     strtotime("2024-01-01", null), "|",
     count(localtime(null)), "|",
     (getdate(null)["year"] === intval(date("Y")) ? "Y" : "y"), "|",
     (mktime(1, null, null, null, null, null) !== false ? "M" : "m"), "|",
     (gmmktime(1, null, null, null, null, null) !== false ? "U" : "u");
"#,
    );
    assert_eq!(out, "D|G|1704067200|9|Y|M|U");
}

/// Verifies runtime nullable values retain php-src's current-time semantics instead of being
/// coerced to the Unix epoch when they cross a boxed union boundary.
#[test]
fn test_datetime_php_src_dynamic_null_timestamp_arguments() {
    let out = compile_and_run(
        r#"<?php
function dynamic_null(bool $returnNull): ?int {
    return $returnNull ? null : 0;
}
function dynamic_mixed_null(bool $returnNull): mixed {
    return $returnNull ? null : 0;
}
$timestamp = dynamic_null(true);
$boxedTimestamp = dynamic_mixed_null(true);
date_default_timezone_set("UTC");
echo (date("Y", $timestamp) !== "1970" ? "D" : "d"), "|",
     (gmdate("Y", $timestamp) !== "1970" ? "G" : "g"), "|",
     (strtotime("now", $timestamp) > 1700000000 ? "S" : "s"), "|",
     (localtime($timestamp)[5] !== 70 ? "L" : "l"), "|",
     (getdate($timestamp)["year"] !== 1970 ? "Y" : "y"), "|",
     (date("Y-m-d", mktime(12, $timestamp, $timestamp, $timestamp, $timestamp, $timestamp))
         === date("Y-m-d") ? "M" : "m"), "|",
     (gmdate("Y-m-d", gmmktime(12, $timestamp, $timestamp, $timestamp, $timestamp, $timestamp))
         === gmdate("Y-m-d") ? "U" : "u"), "|",
     (date("Y", $boxedTimestamp) !== "1970" ? "D" : "d"), "|",
     (gmdate("Y", $boxedTimestamp) !== "1970" ? "G" : "g"), "|",
     (strtotime("now", $boxedTimestamp) > 1700000000 ? "S" : "s"), "|",
     (localtime($boxedTimestamp)[5] !== 70 ? "L" : "l"), "|",
     (getdate($boxedTimestamp)["year"] !== 1970 ? "Y" : "y"), "|",
     (date("Y-m-d", mktime(12, $boxedTimestamp, $boxedTimestamp, $boxedTimestamp,
                           $boxedTimestamp, $boxedTimestamp)) === date("Y-m-d") ? "M" : "m"), "|",
     (gmdate("Y-m-d", gmmktime(12, $boxedTimestamp, $boxedTimestamp, $boxedTimestamp,
                               $boxedTimestamp, $boxedTimestamp)) === gmdate("Y-m-d") ? "U" : "u");
"#,
    );
    assert_eq!(out, "D|G|S|L|Y|M|U|D|G|S|L|Y|M|U");
}

/// Verifies `strtotime()` delegates the complete free-form grammar to timelib for forms the
/// former handwritten runtime rejected.
#[test]
fn test_strtotime_timelib_free_form_grammar() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
echo strtotime("2024/06/15"), "|",
     strtotime("2024-06-15 12:30x"), "|",
     strtotime("third Thursday of November 2024");
"#,
    );
    assert_eq!(out, "1718409600|1718494200|1732147200");
}

/// Verifies php-src's reflection-visible date deprecations, NoDiscard metadata, and typed
/// predefined class constants are retained on the synthetic builtin declarations.
#[test]
fn test_datetime_php_src_attributes_and_typed_constants() {
    let out = compile_and_run_capture(
        r#"<?php
$wakeup = new ReflectionMethod(DateTime::class, "__wakeup");
$discard = new ReflectionMethod(DateTimeImmutable::class, "modify");
$constant = new ReflectionClassConstant(DateTimeInterface::class, "RFC7231");
echo ($wakeup->isDeprecated() ? "D" : "d"), ":",
     count($wakeup->getAttributes("Deprecated")), "|";
$attrs = $discard->getAttributes("NoDiscard");
echo count($attrs), ":";
echo $attrs[0]->getName(), ":";
echo $attrs[0]->getArguments()["message"], "|";
echo ($constant->isDeprecated() ? "D" : "d"), ":",
     count($constant->getAttributes("Deprecated")), ":",
     ($constant->hasType() ? $constant->getType()->getName() : "none");
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "D:1|1:NoDiscard:as DateTimeImmutable::modify() does not modify the object itself|D:1:string"
    );
}

/// Verifies invalid state arrays are rejected with php-src's exact `Error` contract for every
/// date object except `DateInterval`, whose empty state is deliberately accepted.
#[test]
fn test_datetime_invalid_serialization_data_contract() {
    let out = compile_and_run(
        r#"<?php
try { DateTime::__set_state([]); echo "ok|"; }
catch (Error $e) { echo $e->getMessage(), "|"; }
try { DateTimeImmutable::__set_state([]); echo "ok|"; }
catch (Error $e) { echo $e->getMessage(), "|"; }
try { DateTimeZone::__set_state([]); echo "ok|"; }
catch (Error $e) { echo $e->getMessage(), "|"; }
try { DatePeriod::__set_state([]); echo "ok|"; }
catch (Error $e) { echo $e->getMessage(), "|"; }
try {
    DateInterval::__set_state([]);
    echo "interval-ok";
} catch (Error $e) {
    echo "interval-error";
}
"#,
    );
    assert_eq!(
        out,
        "Invalid serialization data for DateTime object|\
Invalid serialization data for DateTimeImmutable object|\
Invalid serialization data for DateTimeZone object|\
Invalid serialization data for DatePeriod object|interval-ok"
    );
}

/// Verifies `DateInterval`'s debug-only state keys behave like undefined properties, including
/// the suppressible runtime warning channel used by PHP's `@` operator.
#[test]
fn test_dateinterval_debug_state_reads_warn_and_return_null() {
    let out = compile_and_run_capture(
        r#"<?php
$iv = new DateInterval("P1D");
echo ($iv->from_string === null ? "null" : "value"), "|";
echo (@$iv->date_string === null ? "suppressed-null" : "value");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "null|suppressed-null");
    assert!(
        out.stderr
            .contains("Warning: Undefined property: DateInterval::$from_string"),
        "expected undefined-property warning, got stderr={}",
        out.stderr
    );
    assert!(
        out.stderr.starts_with("\nWarning:")
            && out.stderr.contains(" in ")
            && out.stderr.contains(" on line 3\n"),
        "expected php-src's leading separator and source location, got stderr={}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("DateInterval::$date_string"),
        "error control should suppress the second warning, got stderr={}",
        out.stderr
    );
}

/// Verifies a variadic `var_dump()` preserves every DateInterval property operand after rendering
/// the object argument, including the integer placed immediately after a float operand.
#[test]
fn test_dateinterval_var_dump_preserves_public_properties() {
    let out = compile_and_run(
        r#"<?php
$left = new DateTime("2009-10-11");
$interval = $left->diff(new DateTime("2009-10-13"));
var_dump(
    $interval,
    $interval->y,
    $interval->m,
    $interval->d,
    $interval->h,
    $interval->i,
    $interval->s,
    $interval->f,
    $interval->invert,
    $interval->days
);
"#,
    );
    assert!(
        out.ends_with("float(0)\nint(0)\nint(2)\n"),
        "DateInterval debug rendering mutated public state: {out}"
    );
    assert!(!out.contains("9223372036854775806"));
}

/// G12: `DatePeriod::__serialize()` returns the period's state as an array with `start`,
/// `interval`, `recurrences`, `include_start_date`, `include_end_date`.
#[test]
fn test_dateperiod_serialize() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$p = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), 3);
$a = $p->__serialize();
echo $a["start"]->format("Y-m-d"), "|", $a["interval"]->format("%d"), "|", $a["recurrences"], "|",
     $a["include_start_date"] ? "1" : "0", "|", $a["include_end_date"] ? "1" : "0";
"#,
    );
    assert_eq!(out, "2024-01-01|1|4|1|0");
}

/// Verifies both DatePeriod restoration hooks snapshot `current` instead of retaining an alias
/// to the caller-owned state array.
#[test]
fn test_dateperiod_restoration_clones_current_snapshot() {
    let out = compile_and_run_capture(
        r#"<?php
$p = new DatePeriod(
    new DateTime("2024-01-01", new DateTimeZone("Europe/Paris")),
    new DateInterval("P1D"),
    3
);
foreach ($p as $date) { break; }
$data = $p->__serialize();
$restored = new DatePeriod(new DateTime("2000-01-01"), new DateInterval("P1D"), 1);
$restored->__unserialize($data);
$fromState = DatePeriod::__set_state($data);
echo $restored->current->format("Y-m-d e"), "|",
     $fromState->current->format("Y-m-d e"), "|";
$data["current"]->modify("+10 days");
echo $restored->current->format("Y-m-d e"), "|",
     $fromState->current->format("Y-m-d e"), "|",
     $data["current"]->format("Y-m-d e");
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "2024-01-01 Europe/Paris|2024-01-01 Europe/Paris|\
2024-01-01 Europe/Paris|2024-01-01 Europe/Paris|2024-01-11 Europe/Paris"
    );
}

/// Verifies a serialized diff interval retains timelib's total-day field during subtraction,
/// including type-2 abbreviation zones whose historical civil offset differs.
#[test]
fn test_datetime_serialized_diff_subtraction_preserves_days_semantics() {
    let out = compile_and_run(
        r#"<?php
$first = new DateTimeImmutable("1978-12-22 09:15 CET");
$last = new DateTimeImmutable("2022-04-15 10:27:27 BST");
$interval = unserialize(serialize($first->diff($last)));
$restored = $last->sub($interval);
echo $restored->format("Y-m-d H:i:s T"), "|", $restored->getTimestamp();
"#,
    );
    assert_eq!(out, "1978-12-22 09:15:00 BST|283162500");
}

/// Verifies DatePeriod restoration accepts php-src's nullable state, rejects recurrence values
/// outside the serialized C integer domain, and computes unusual restored recurrence totals.
#[test]
fn test_dateperiod_restoration_nullable_state_and_recurrence_bounds() {
    let out = compile_and_run(
        r#"<?php
$base = [
    "start" => null,
    "end" => null,
    "current" => null,
    "interval" => DateInterval::createFromDateString("tomorrow"),
    "recurrences" => 1,
    "include_start_date" => false,
    "include_end_date" => false,
];
$restored = DatePeriod::__set_state($base);
echo "nullable:", $restored->recurrences, ":",
    ($restored->getRecurrences() === 1 ? "1" : "0"), "|";
foreach ([-1, 2147483648] as $invalid) {
    $base["recurrences"] = $invalid;
    try {
        DatePeriod::__set_state($base);
        echo "accepted|";
    } catch (Error $e) {
        echo $e->getMessage(), "|";
    }
}
"#,
    );
    assert_eq!(
        out,
        "nullable:1:1|Invalid serialization data for DatePeriod object|\
Invalid serialization data for DatePeriod object|"
    );
}

/// Verifies DatePeriod restoration has php-src's documented no-rollback-on-error behavior.
#[test]
fn test_dateperiod_unserialize_keeps_fields_applied_before_error() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$period = new DatePeriod(
    new DateTime("2022-07-14"),
    new DateInterval("P1D"),
    new DateTime("2022-07-16")
);
try {
    $period->__unserialize([
        "current" => new DateTime("2024-08-27"),
        "start" => new DateTime("2024-08-28"),
        "end" => new DateTime("2024-08-29"),
        "interval" => new DateInterval("P2D"),
        "recurrences" => 2,
        "include_start_date" => "wrong type",
        "include_end_date" => true,
    ]);
} catch (Error $e) {
    echo $e->getMessage(), "|";
}
echo $period->start->format("Y-m-d"), ":",
    $period->current->format("Y-m-d"), ":",
    $period->end->format("Y-m-d"), ":",
    $period->interval->format("%d"), ":",
    $period->recurrences, ":",
    ($period->include_start_date ? "1" : "0"), ":",
    ($period->include_end_date ? "1" : "0");
"#,
    );
    assert_eq!(
        out,
        "Invalid serialization data for DatePeriod object|\
2024-08-28:2024-08-27:2024-08-29:2:2:1:0"
    );
}

/// Verifies the deprecated string overload also accepts a runtime-computed specification.
#[test]
fn test_dateperiod_ctor_string_form() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$specification = "R3/2020-01-01T00:00:00Z/P1D";
$p = new DatePeriod($specification);
$n = 0;
foreach ($p as $d) { $n++; }
echo $n;
"#,
    );
    assert_eq!(out, "4");
}

/// G11: `getLastErrors()` returns a detailed error array with positions. Trailing data on a
/// `createFromFormat` mismatch is reported as an error at the trailing position.
#[test]
fn test_get_last_errors_trailing_data() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
DateTime::createFromFormat("Y-m-d", "2024-01-01X");
$e = DateTime::getLastErrors();
echo $e["error_count"], "|", $e["errors"]["10"];
"#,
    );
    assert_eq!(out, "1|Trailing data");
}

/// Verifies the lenient `+` format modifier preserves its trailing-data warning
/// after the successful result object has been initialized.
#[test]
fn test_get_last_errors_lenient_trailing_data_warning() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$date = DateTime::createFromFormat("m/d/y+", "06/08/04 12:00");
$errors = DateTime::getLastErrors();
echo $date->format("Y-m-d"), "|",
     $errors["warning_count"], "|",
     $errors["warnings"]["8"], "|",
     $errors["error_count"];
"#,
    );
    assert_eq!(out, "2004-06-08|1|Trailing data|0");
}

/// G11: `getLastErrors()` returns `false` when the last `createFromFormat` succeeded (no errors,
/// no warnings).
#[test]
fn test_get_last_errors_no_errors_returns_false() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
DateTime::createFromFormat("Y-m-d", "2024-01-15");
$r = DateTime::getLastErrors();
echo ($r === false) ? "false" : "other";
"#,
    );
    assert_eq!(out, "false");
}

/// G11: `getLastErrors()` reports a warning "The parsed date was invalid" when the date overflows
/// (e.g. month 13 → normalized to next year).
#[test]
fn test_get_last_errors_invalid_date_warning() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
DateTime::createFromFormat("Y-m-d", "2024-13-99");
$e = DateTime::getLastErrors();
echo $e["warning_count"], "|", $e["warnings"]["10"];
"#,
    );
    assert_eq!(out, "1|The parsed date was invalid");
}

/// Verifies `timezone_name_from_abbr()` follows timelib's ordered lookup: an exact UTC-offset
/// match disambiguates known abbreviations, while an unknown abbreviation can fall back solely
/// by UTC offset and DST flag.
#[test]
fn test_timezone_name_from_abbr_with_offset() {
    let out = compile_and_run(
        r#"<?php
echo timezone_name_from_abbr("CST"), "|",
     timezone_name_from_abbr("CST", -18000, 1), "|",
     timezone_name_from_abbr("CST", 28800, 0), "|",
     timezone_name_from_abbr("IST", 3600, 0), "|",
     timezone_name_from_abbr("CEST", 10800, 1), "|",
     timezone_name_from_abbr("", 0, 0), "|",
     timezone_name_from_abbr("???", 19800, 0);
"#,
    );
    assert_eq!(
        out,
        "America/Chicago|America/Havana|Asia/Chongqing|Europe/Dublin|Europe/Kaliningrad|Europe/London|Asia/Kolkata"
    );
}

/// Verifies DateInterval delegates every ISO/relative form to php-src timelib,
/// retains special weekday arithmetic, and keeps the procedural warning/false
/// contract distinct from the throwing object API.
#[test]
fn test_dateinterval_timelib_complete_grammar_and_special_arithmetic() {
    let out = compile_and_run(
        r#"<?php
$combined = new DateInterval("P0001-02-03T04:05:06");
echo $combined->format("%y,%m,%d,%h,%i,%s,%a"), "|";
$endpoints = new DateInterval(
    "2007-03-01T13:00:00Z/2008-05-11T15:30:00Z"
);
echo $endpoints->format("%y,%m,%d,%h,%i,%s,%a"), "|";
try {
    new DateInterval("P1Y2Y");
} catch (Throwable $exception) {
    echo get_class($exception), ":", $exception->getMessage(), "|";
}
$relative = DateInterval::createFromDateString(
    "first monday of next month"
);
echo $relative->format("%y,%m,%d,%h,%i,%s,%a"), "|";
$date = new DateTime("2024-01-15T00:00:00Z");
$date->add($relative);
echo $date->format("Y-m-d"), "|";
$date = new DateTime("2024-01-15T00:00:00Z");
try {
    $date->sub($relative);
} catch (Throwable $exception) {
    echo get_class($exception), ":", $exception->getMessage(), "|";
}
$date = new DateTime("2024-01-15T00:00:00Z");
$returned = @date_sub($date, $relative);
echo ($returned === $date ? "1" : "0"), ":", $date->format("Y-m-d"), "|";
var_dump(@date_interval_create_from_date_string("foobar") === false);
"#,
    );
    assert_eq!(
        out,
        "1,2,3,4,5,6,(unknown)|1,2,10,2,30,0,437|\
DateMalformedIntervalStringException:Unknown or bad format (P1Y2Y)|\
0,1,0,0,0,0,(unknown)|2024-02-05|\
DateInvalidOperationException:DateTime::sub(): Only non-special relative time specifications are supported for subtraction|\
1:2024-01-15|bool(true)\n"
    );
}

/// Replays php-src `DatePeriod_serialize-004.phpt`: a special relative interval is exposed and
/// serialized as normalized calendar components while its DatePeriod snapshot retains the hidden
/// weekday rule used for iteration.
#[test]
fn test_dateperiod_relative_interval_snapshot_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/London");
$start = new DateTimeImmutable("1978-12-22 09:15:00 Europe/Amsterdam");
$relative = DateInterval::createFromDateString("first monday of next month");
$period = new DatePeriod(
    $start,
    $relative,
    3,
    DatePeriod::EXCLUDE_START_DATE
);
$snapshot = $period->getDateInterval();
echo $snapshot->format("%m"), ":";
var_dump($snapshot->from_string);
echo str_contains(
    serialize($period),
    's:11:"from_string";b:0;'
) ? "normalized|" : "relative|";
foreach ($period as $date) {
    echo $date->format("Y-m-d"), "|";
}
"#,
    );
    assert_eq!(
        out,
        "1:NULL\nnormalized|1979-01-01|1979-02-05|1979-03-05|"
    );
}

/// Verifies DatePeriod uses timelib's full ISO interval grammar, including both
/// endpoint/period orders, combined interval notation, exact missing-part
/// diagnostics, and constructor recurrence bounds.
#[test]
fn test_dateperiod_timelib_complete_iso_grammar_and_errors() {
    let out = compile_and_run(
        r#"<?php
$forms = [
    "R4/2012-07-01T00:00:00Z/P7D",
    "2024-01-01T00:00:00Z/P2D/2024-01-07T00:00:00Z",
    "2024-01-01T00:00:00Z/2024-01-07T00:00:00Z/P2D",
    "R2/2024-01-01T00:00:00Z/P0000-00-02T00:00:00",
];
foreach ($forms as $specification) {
    $period = DatePeriod::createFromISO8601String($specification);
    foreach ($period as $date) {
        echo $date->format("Y-m-d"), ",";
    }
    echo "|";
}
foreach ([
    "R4",
    "R4/2012-07-01T00:00:00Z",
    "2012-07-01T00:00:00Z/P7D",
    "bad",
] as $specification) {
    try {
        DatePeriod::createFromISO8601String($specification);
    } catch (Throwable $exception) {
        echo get_class($exception), ":", $exception->getMessage(), "|";
    }
}
try {
    new DatePeriod(
        new DateTimeImmutable("2024-01-01"),
        new DateInterval("P1D"),
        0
    );
} catch (Throwable $exception) {
    echo get_class($exception), ":", $exception->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "2012-07-01,2012-07-08,2012-07-15,2012-07-22,2012-07-29,|\
2024-01-01,2024-01-03,2024-01-05,|\
2024-01-01,2024-01-03,2024-01-05,|\
2024-01-01,2024-01-03,2024-01-05,|\
DateMalformedPeriodStringException:DatePeriod::createFromISO8601String(): ISO interval must contain a start date, \"R4\" given|\
DateMalformedPeriodStringException:DatePeriod::createFromISO8601String(): ISO interval must contain an interval, \"R4/2012-07-01T00:00:00Z\" given|\
DateMalformedPeriodStringException:DatePeriod::createFromISO8601String(): ISO interval must contain an end date or a recurrence count, \"2012-07-01T00:00:00Z/P7D\" given|\
DateMalformedPeriodStringException:Unknown or bad format (bad)|\
DateMalformedPeriodStringException:DatePeriod::__construct(): Recurrence count must be greater or equal to 1 and lower than 2147483640"
    );
}

/// Verifies the deprecated string constructor reports its own entry point for
/// missing ISO components instead of leaking the static factory name.
#[test]
fn test_dateperiod_string_constructor_malformed_component_messages() {
    let out = compile_and_run(
        r#"<?php
error_reporting(E_ALL & ~E_DEPRECATED);
foreach (["R4", "R4/2012-07-01T00:00:00Z", "2012-07-01T00:00:00Z/P7D"] as $specification) {
    try { new DatePeriod($specification); }
    catch (DateMalformedPeriodStringException $exception) {
        echo $exception->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "DatePeriod::__construct(): ISO interval must contain a start date, \"R4\" given\n\
DatePeriod::__construct(): ISO interval must contain an interval, \"R4/2012-07-01T00:00:00Z\" given\n\
DatePeriod::__construct(): ISO interval must contain an end date or a recurrence count, \"2012-07-01T00:00:00Z/P7D\" given\n"
    );
}

/// Verifies php-src's `DateTimeZone` object comparator, including its uncomparable sentinel,
/// different-kind exception, clone equality, and uninitialized-subclass error.
#[test]
fn test_datetimezone_php_src_comparison_handler() {
    let out = compile_and_run(
        r#"<?php
foreach ([
    ["+0200", "-0200"],
    ["EST", "PST"],
    ["Europe/Amsterdam", "Europe/Berlin"],
] as $names) {
    $leftName = $names[0];
    $rightName = $names[1];
    $left = new DateTimeZone($leftName);
    $equal = clone $left;
    $right = new DateTimeZone($rightName);
    compareDateTimeZones($left, $equal);
    compareDateTimeZones($left, $right);
}
function compareDateTimeZones(DateTimeZone $a, DateTimeZone $b): void {
    echo ($a < $b ? "1" : "0"),
         ($a <= $b ? "1" : "0"),
         ($a == $b ? "1" : "0"),
         ($a != $b ? "1" : "0"),
         ($a >= $b ? "1" : "0"),
         ($a > $b ? "1" : "0"),
         ":", $a <=> $b, "|";
}
try {
    var_dump(new DateTimeZone("Europe/Berlin") == new DateTimeZone("CET"));
} catch (Throwable $exception) {
    echo get_class($exception), ":", $exception->getMessage(), "|";
}
class UninitializedDateTimeZone extends DateTimeZone {
    public function __construct() {}
}
try {
    var_dump(new UninitializedDateTimeZone() == new UninitializedDateTimeZone());
} catch (Throwable $exception) {
    echo get_class($exception), ":", $exception->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "011010:0|000100:1|011010:0|000100:1|011010:0|000100:1|\
DateException:Cannot compare two different kinds of DateTimeZone objects|\
DateObjectError:Trying to compare uninitialized DateTimeZone objects"
    );
}

/// Verifies inherited date methods reject subclasses that skipped their parent constructor, and
/// that date comparison uses php-src's specialized incomplete-object message.
#[test]
fn test_datetime_family_uninitialized_subclass_errors_match_php_src() {
    let out = compile_and_run(
        r#"<?php
class MissingDateTimeCtor extends DateTime { public function __construct() {} }
class MissingImmutableCtor extends DateTimeImmutable { public function __construct() {} }
class MissingZoneCtor extends DateTimeZone { public function __construct() {} }
class MissingIntervalCtor extends DateInterval { public function __construct() {} }
function showError(callable $callback): void {
    try {
        $callback();
    } catch (Throwable $exception) {
        echo get_class($exception), ":", $exception->getMessage(), "|";
    }
}
$date = new MissingDateTimeCtor();
$immutable = new MissingImmutableCtor();
$zone = new MissingZoneCtor();
$interval = new MissingIntervalCtor();
showError(fn() => $date->format("c"));
showError(fn() => $immutable->format("c"));
showError(fn() => $zone->getName());
showError(fn() => $interval->format("%d"));
showError(fn() => $date == new MissingDateTimeCtor());
"#,
    );
    assert_eq!(
        out,
        "DateObjectError:Object of type MissingDateTimeCtor (inheriting DateTime) has not been correctly initialized by calling parent::__construct() in its constructor|\
DateObjectError:Object of type MissingImmutableCtor (inheriting DateTimeImmutable) has not been correctly initialized by calling parent::__construct() in its constructor|\
DateObjectError:Object of type MissingZoneCtor (inheriting DateTimeZone) has not been correctly initialized by calling parent::__construct() in its constructor|\
DateObjectError:Object of type MissingIntervalCtor (inheriting DateInterval) has not been correctly initialized by calling parent::__construct() in its constructor|\
DateObjectError:Trying to compare an incomplete DateTime or DateTimeImmutable object|"
    );
}

/// Verifies the procedural `date_create()` compatibility path keeps php-src's legacy `Error`
/// class and wording for a `DateTimeZone` subclass that skipped its parent constructor.
#[test]
fn test_date_create_uninitialized_timezone_uses_legacy_error() {
    let out = compile_and_run(
        r#"<?php
class MissingDateCreateZoneCtor extends DateTimeZone {
    public function __construct() {}
}
try {
    date_create("2005-07-14 22:30:41", new MissingDateCreateZoneCtor());
} catch (Error $error) {
    echo get_class($error), ": ", $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Error: The DateTimeZone object has not been correctly initialized by its constructor"
    );
}

/// Verifies late-static DateTime factories reject abstract subclasses with a catchable php-src
/// `Error` instead of falling through the dynamic allocator's missing-class path.
#[test]
fn test_datetime_static_factories_reject_abstract_subclasses() {
    let out = compile_and_run(
        r#"<?php
abstract class AbstractMutableDate extends DateTime {
    abstract public function marker();
}
abstract class AbstractImmutableDate extends DateTimeImmutable {
    abstract public function marker();
}
abstract class AbstractPeriod extends DatePeriod {
    abstract public function marker();
}
try {
    AbstractPeriod::createFromISO8601String("R5");
} catch (Error $error) {
    echo $error->getMessage(), "|";
}
try {
    AbstractMutableDate::createFromTimestamp(0);
} catch (Error $error) {
    echo $error->getMessage(), "|";
}
try {
    AbstractImmutableDate::createFromMutable(new DateTime());
} catch (Error $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Cannot instantiate abstract class AbstractPeriod|\
Cannot instantiate abstract class AbstractMutableDate|\
Cannot instantiate abstract class AbstractImmutableDate"
    );
}

/// Ensures arrow callables preserve DateTime object returns from instance and static methods
/// instead of coercing those object payloads through the scalar fallback ABI.
#[test]
fn test_datetime_arrow_callable_method_returns_preserve_objects() {
    let out = compile_and_run(
        r#"<?php
function invoke_datetime_callable(callable $callable): void {
    echo get_class($callable()), "|";
}
$date = new DateTime("2024-01-01T00:00:00Z");
invoke_datetime_callable(fn() => $date->add(new DateInterval("P1D")));
invoke_datetime_callable(fn() => DateTimeImmutable::createFromInterface($date));
"#,
    );
    assert_eq!(out, "DateTime|DateTimeImmutable|");
}

/// Verifies runtime `serialize()` dispatches inherited ext/date magic methods and preserves each
/// class's php-src uninitialized-object error.
#[test]
fn test_datetime_uninitialized_serialization_errors_match_php_src() {
    let out = compile_and_run(
        r#"<?php
class MissingDateTimeSerializeCtor extends DateTime { public function __construct() {} }
class MissingZoneSerializeCtor extends DateTimeZone { public function __construct() {} }
class MissingIntervalSerializeCtor extends DateInterval { public function __construct() {} }
class MissingPeriodSerializeCtor extends DatePeriod { public function __construct() {} }
function serializeError(object $object): void {
    try {
        serialize($object);
    } catch (Throwable $exception) {
        echo get_class($exception), ":", $exception->getMessage(), "|";
    }
}
serializeError(new MissingDateTimeSerializeCtor());
serializeError(new MissingZoneSerializeCtor());
serializeError(new MissingIntervalSerializeCtor());
serializeError(new MissingPeriodSerializeCtor());
"#,
    );
    assert_eq!(
        out,
        "DateObjectError:Object of type MissingDateTimeSerializeCtor (inheriting DateTime) has not been correctly initialized by calling parent::__construct() in its constructor|\
DateObjectError:Object of type MissingZoneSerializeCtor (inheriting DateTimeZone) has not been correctly initialized by calling parent::__construct() in its constructor|\
DateObjectError:Object of type MissingIntervalSerializeCtor (inheriting DateInterval) has not been correctly initialized by calling parent::__construct() in its constructor|\
DateObjectError:Object of type MissingPeriodSerializeCtor (inheriting DatePeriod) has not been correctly initialized by calling parent::__construct() in its constructor|"
    );
}

/// Verifies php-src's `DatePeriod` uninitialized special cases: payload getters throw, iteration
/// uses the base-class wording, while nullable end and recurrence accessors remain callable.
#[test]
fn test_dateperiod_uninitialized_special_cases_match_php_src() {
    let out = compile_and_run(
        r#"<?php
class MissingPeriodCtor extends DatePeriod { public function __construct() {} }
$period = new MissingPeriodCtor();
try {
    $period->getStartDate();
} catch (Throwable $exception) {
    echo get_class($exception), ":", $exception->getMessage(), "|";
}
try {
    $period->getDateInterval();
} catch (Throwable $exception) {
    echo get_class($exception), ":", $exception->getMessage(), "|";
}
try {
    foreach ($period as $value) {}
} catch (Throwable $exception) {
    echo get_class($exception), ":", $exception->getMessage(), "|";
}
var_dump($period->getEndDate());
var_dump($period->getRecurrences());
"#,
    );
    assert_eq!(
        out,
        "DateObjectError:Object of type MissingPeriodCtor (inheriting DatePeriod) has not been correctly initialized by calling parent::__construct() in its constructor|\
DateObjectError:Object of type MissingPeriodCtor (inheriting DatePeriod) has not been correctly initialized by calling parent::__construct() in its constructor|\
DateObjectError:Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor|\
NULL\nNULL\n"
    );
}

/// Verifies DatePeriod keeps an internal current snapshot distinct from each iterator result.
#[test]
fn test_dateperiod_iterator_current_snapshot_does_not_alias_returned_value() {
    let out = compile_and_run(
        r#"<?php
$start = new DateTime('2018-12-31 00:00:00', new DateTimeZone('UTC'));
$period = new DatePeriod($start, new DateInterval('P1M'), 1);
$iterator = $period->getIterator();
$returned = $iterator->current();
$returned->setTimestamp(0);
$properties = get_object_vars($period);
echo $returned->format('Y-m-d'), '|', $properties['current']->format('Y-m-d');
"#,
    );
    assert_eq!(out, "1970-01-01|2018-12-31");
}

/// Replays php-src `bug75002.phpt`: an uncaught uninitialized DatePeriod iterator guard reports
/// the foreach call site, the concrete DateObjectError payload, and a main-only stack trace.
#[test]
fn test_dateperiod_uninitialized_foreach_uncaught_trace_matches_php_src() {
    let output = compile_and_run_capture(
        r#"<?php
class MissingPeriodIteratorCtor extends DatePeriod { public function __construct() {} }
$period = new MissingPeriodIteratorCtor();
foreach ($period as $value) {}
"#,
    );
    assert!(!output.success, "uncaught DateObjectError unexpectedly succeeded");
    assert_eq!(output.stdout, "");
    assert!(
        output.stderr.starts_with(
            "\nFatal error: Uncaught DateObjectError: Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor in "
        ),
        "unexpected DatePeriod fatal header: {}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains(":4\nStack trace:\n#0 {main}\n  thrown in "),
        "unexpected DatePeriod fatal stack: {}",
        output.stderr
    );
    assert!(
        output.stderr.ends_with(" on line 4\n"),
        "unexpected DatePeriod thrown location: {}",
        output.stderr
    );
}

/// Verifies php-src rejects DatePeriod iteration by reference at runtime with a catchable Error,
/// instead of turning the PHP program into an elephc compile-time diagnostic.
#[test]
fn test_dateperiod_foreach_by_reference_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
$period = new DatePeriod(
    new DateTimeImmutable("2024-01-01"),
    new DateInterval("P1D"),
    1
);
try {
    foreach ($period as &$date) {}
} catch (Throwable $exception) {
    echo get_class($exception), ":", $exception->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Error:An iterator cannot be used with foreach by reference"
    );
}

/// Verifies date comparison remains ordered for timestamps whose microsecond-scaled value would
/// overflow a signed 64-bit integer.
#[test]
fn test_datetime_large_timestamp_comparison_avoids_overflow() {
    let out = compile_and_run(
        r#"<?php
$left = DateTimeImmutable::createFromTimestamp(10000000000000);
$right = DateTimeImmutable::createFromTimestamp(10000000000001);
var_dump($left < $right, $left <=> $right, $right > $left, $left == $right);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nint(-1)\nbool(true)\nbool(false)\n"
    );
}

/// Verifies the runtime serializer uses ext/date magic hooks instead of exposing synthetic private
/// storage for ordinary initialized date and timezone objects.
#[test]
fn test_datetime_generic_serialize_uses_magic_hooks() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
echo serialize(new DateTime("2024-01-02 03:04:05.123456 UTC")), "|";
echo serialize(new DateTimeZone("Europe/Paris"));
"#,
    );
    assert_eq!(
        out,
        "O:8:\"DateTime\":3:{s:4:\"date\";s:26:\"2024-01-02 03:04:05.123456\";s:13:\"timezone_type\";i:3;s:8:\"timezone\";s:3:\"UTC\";}|\
O:12:\"DateTimeZone\":2:{s:13:\"timezone_type\";i:3;s:8:\"timezone\";s:12:\"Europe/Paris\";}"
    );
}

/// Verifies inherited ext/date serialization preserves promoted custom properties and that
/// `parent::__construct()` materializes every internal DateTime-family constructor body.
#[test]
fn test_datetime_inherited_serialization_preserves_custom_properties() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/London");

class AuditDateTime extends DateTime {
    public function __construct(
        string $datetime = "now",
        ?DateTimeZone $timezone = null,
        public ?bool $myProperty = null,
    ) {
        parent::__construct($datetime, $timezone);
    }
}

class AuditDateTimeImmutable extends DateTimeImmutable {
    public function __construct(
        string $datetime = "now",
        ?DateTimeZone $timezone = null,
        public ?bool $myProperty = null,
    ) {
        parent::__construct($datetime, $timezone);
    }
}

class AuditDateTimeZone extends DateTimeZone {
    public function __construct(
        string $timezone = "Europe/Kyiv",
        public ?bool $myProperty = null,
    ) {
        parent::__construct($timezone);
    }
}

class AuditDateInterval extends DateInterval {
    public function __construct(
        string $duration,
        public ?bool $myProperty = null,
    ) {
        parent::__construct($duration);
    }
}

class AuditDatePeriod extends DatePeriod {
    public function __construct(
        DateTimeInterface $start,
        DateInterval $interval,
        int $recurrences,
        int $options = 0,
        public ?bool $myProperty = null,
    ) {
        parent::__construct($start, $interval, $recurrences, $options);
    }
}

$dateTime = new AuditDateTime("2023-01-25 16:32:55", myProperty: true);
$dateTimeCopy = unserialize(serialize($dateTime));
var_dump($dateTimeCopy->myProperty);

$immutable = new AuditDateTimeImmutable("2023-01-25 16:32:55", myProperty: true);
$immutableCopy = unserialize(serialize($immutable));
var_dump($immutableCopy->myProperty);

$zone = new AuditDateTimeZone("Europe/London", myProperty: true);
$zoneCopy = unserialize(serialize($zone));
var_dump($zoneCopy->myProperty);

$interval = new AuditDateInterval("P1W2D", myProperty: true);
$intervalCopy = unserialize(serialize($interval));
var_dump($intervalCopy->myProperty);

$period = new AuditDatePeriod(
    new DateTimeImmutable(),
    new DateInterval("PT5S"),
    5,
    myProperty: true,
);
$periodCopy = unserialize(serialize($period));
var_dump($periodCopy->myProperty);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n"
    );
}

/// Verifies inherited date serialization preserves public, protected, and private properties
/// across the concrete string, indexed-array, object, and boxed-Mixed runtime layouts.
#[test]
fn test_datetime_inherited_serialization_preserves_typed_property_layouts() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");

class RichDateTime extends DateTime {
    public string $text = "default";
    protected array $list = [];
    private DateTimeZone $zone;
    public mixed $assoc = null;

    public function __construct() {
        parent::__construct("2024-01-02 03:04:05 UTC");
        $this->text = "custom";
        $this->list = [3, 5, 8];
        $this->zone = new DateTimeZone("Europe/Paris");
        $this->assoc = ["alpha" => 11, "beta" => "ok"];
    }

    public function snapshot(): string {
        return $this->text
            . "|" . implode(",", $this->list)
            . "|" . $this->zone->getName()
            . "|" . $this->assoc["alpha"]
            . "|" . $this->assoc["beta"];
    }
}

$original = new RichDateTime();
$copy = unserialize(serialize($original));
echo $original->snapshot(), "\n", $copy->snapshot(), "\n";
"#,
    );
    assert_eq!(
        out,
        "custom|3,5,8|Europe/Paris|11|ok\ncustom|3,5,8|Europe/Paris|11|ok\n"
    );
}

/// Reproduces php-src `ext/date/tests/gh7758.phpt`: epoch-string fractions use floor semantics
/// before zero and retain the fixed-offset timezone abbreviation.
#[test]
fn test_datetime_epoch_string_negative_fractions_match_php_src() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
foreach ([0.4, 0, -0.4, -1, -1.4] as $timestamp) {
    echo (new DateTime("@" . $timestamp))->format("Y-m-d H:i:s.u T"), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "1970-01-01 00:00:00.400000 GMT+0000\n\
1970-01-01 00:00:00.000000 GMT+0000\n\
1969-12-31 23:59:59.600000 GMT+0000\n\
1969-12-31 23:59:59.000000 GMT+0000\n\
1969-12-31 23:59:58.600000 GMT+0000\n"
    );
}

/// Verifies php-src's six-decimal rounding carry for positive and negative float timestamps.
#[test]
fn test_datetime_create_from_timestamp_rounding_carry_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
foreach ([0.9999991, 0.9999998, 1.0000001, 1.0000008, 1.0000018] as $timestamp) {
    echo DateTime::createFromTimestamp($timestamp)->format("s.u"), "|";
    echo DateTime::createFromTimestamp(-$timestamp)->format("s.u"), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "00.999999|59.000001\n\
01.000000|59.000000\n\
01.000000|59.000000\n\
01.000001|58.999999\n\
01.000002|58.999998\n"
    );
}

/// Verifies mixed-backed factory results retain their DateTime method result type and compare
/// negative instants lexicographically by seconds and microseconds.
#[test]
fn test_datetime_negative_fraction_comparison_after_factory_chain() {
    let out = compile_and_run(
        r#"<?php
$earlyDate1 = DateTime::createFromFormat("U.u", "1.8642")->modify("-5 seconds");
$earlyDate2 = DateTime::createFromFormat("U.u", "1.2768")->modify("-5 seconds");
$earlyDate3 = DateTime::createFromFormat("U.u", "1.2768")->modify("-5 seconds");
var_dump(
    $earlyDate1 == $earlyDate2,
    $earlyDate1 > $earlyDate2,
    $earlyDate2 < $earlyDate1,
    $earlyDate2 == $earlyDate3,
);
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(true)\nbool(true)\nbool(true)\n"
    );
}

/// Verifies the timelib bridge preserves both endpoints of PHP's signed 64-bit timestamp range.
#[test]
fn test_datetime_signed_timestamp_endpoints_match_php_src() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
var_dump((new DateTime("@-9223372036854775808"))->getTimestamp());
var_dump((new DateTime("-292277022657-01-27 08:29:52 UTC"))->getTimestamp());
var_dump((new DateTime("@9223372036854775807"))->getTimestamp());
var_dump((new DateTime("+292277026596-12-04 15:30:07 UTC"))->getTimestamp());
"#,
    );
    assert_eq!(
        out,
        "int(-9223372036854775808)\n\
int(-9223372036854775808)\n\
int(9223372036854775807)\n\
int(9223372036854775807)\n"
    );
}

/// Reproduces php-src `DateTime_modify_invalid_format.phpt`: the procedural wrapper warns and
/// returns false, `@` suppresses that warning, and the object method continues to throw.
#[test]
fn test_date_modify_invalid_warning_and_exception_match_php_src() {
    let output = compile_and_run_capture(
        r#"<?php
$datetime = new DateTime();
var_dump(date_modify($datetime, ""));
var_dump(@date_modify($datetime, ""));
try {
    $datetime->modify("");
} catch (DateMalformedStringException $error) {
    echo $error::class, ": ", $error->getMessage(), "\n";
}
"#,
    );
    assert!(output.success, "date_modify fixture failed: {}", output.stderr);
    assert_eq!(
        output.stdout,
        "bool(false)\nbool(false)\n\
DateMalformedStringException: DateTime::modify(): Failed to parse time string () at position 0 ( ): Empty string\n"
    );
    assert!(
        output.stderr.starts_with(
            "\nWarning: date_modify(): Failed to parse time string () at position 0 ( ): Empty string in "
        ) && output.stderr.ends_with(" on line 3\n"),
        "unexpected date_modify warning: {}",
        output.stderr
    );
}

/// Reproduces php-src `DateTime_wakeup_exception.phpt`: a mismatched serialized string length
/// fails before `DateTime::__wakeup()`, returns false, and reports Zend's exact byte offset.
#[test]
fn test_datetime_unserialize_malformed_length_warns_before_wakeup() {
    let output = compile_and_run_capture(
        r#"<?php
$bad = 'O:8:"DateTime":3:{s:4:"date";s:26:"2023-01-13 14:48:01.705516";s:13:"timezone_type";i:3;s:8:"timezone";s:1:"Europe/Kyiv";}';
$value = unserialize($bad);
echo gettype($value);
"#,
    );
    assert!(output.success, "unserialize fixture failed: {}", output.stderr);
    assert_eq!(output.stdout, "boolean");
    assert!(
        output
            .stderr
            .starts_with("\nWarning: unserialize(): Error at offset 109 of 122 bytes in "),
        "unexpected warning prefix: {}",
        output.stderr
    );
    assert!(
        output.stderr.ends_with(" on line 3\n"),
        "unexpected warning source line: {}",
        output.stderr
    );
}

/// Replays the truncated DateInterval/DateTime OSS-Fuzz payloads from php-src:
/// reference indices include aliases, parse warnings precede magic hooks, and a
/// malformed empty dynamic property still emits its deprecation before false.
#[test]
fn test_datetime_unserialize_truncated_object_fuzz_payloads_match_php_src() {
    let output = compile_and_run_capture(
        r#"<?php
$payloads = [
    'O:12:"DaTeInterval":2:{i:2;r:1;i:0;R:2;',
    'O:8:"DateTime":1:{i:1;d:2;',
    'O:12:"DateInterval":1:{s:0:"";s:2:"  ";',
];
foreach ($payloads as $payload) {
    $value = false;
    try { $value = unserialize($payload); }
    catch (Error $error) { echo $error->getMessage(), "\n"; }
    var_dump($value);
}
"#,
    );
    assert!(output.success, "unserialize fuzz fixture failed: {}", output.stderr);
    assert_eq!(
        output.stdout,
        "bool(false)\nInvalid serialization data for DateTime object\nbool(false)\nbool(false)\n"
    );
    assert!(
        output.stderr.contains("Error at offset 39 of 39 bytes")
            && output.stderr.contains("Error at offset 26 of 26 bytes")
            && output
                .stderr
                .contains("Creation of dynamic property DateInterval::$ is deprecated"),
        "unexpected OSS-Fuzz diagnostics: {}",
        output.stderr
    );
    assert_eq!(
        output.stderr.matches(" on line 9\n").count(),
        4,
        "unexpected OSS-Fuzz diagnostic call sites: {}",
        output.stderr
    );
}

/// Replays php-src `DatePeriod_no_advance_on_valid.phpt`, including factory results whose
/// declared type is `DateTime|false` and repeated iteration after explicit `valid()` calls.
#[test]
fn test_dateperiod_factory_union_and_valid_do_not_advance() {
    let out = compile_and_run(
        r#"<?php
$start = DateTime::createFromFormat("Y-m-d H:i:s", "2022-01-01 00:00:00");
$end = DateTime::createFromFormat("Y-m-d H:i:s", "2022-01-04 00:00:00");
$interval = DateInterval::createFromDateString("1 day");
$period = new DatePeriod($start, $interval, $end);
$iterator = $period->getIterator();
foreach ($iterator as $item) {
    echo $item->format("Y-m-d"), "\n";
}
echo "---------STEP 2\n";
foreach ($iterator as $item) {
    $iterator->valid();
    echo $item->format("Y-m-d"), "\n";
}
$period = new DatePeriod($start, $interval, $end, DatePeriod::EXCLUDE_START_DATE);
$iterator = $period->getIterator();
echo "---------STEP 3\n";
foreach ($iterator as $item) {
    echo $item->format("Y-m-d"), "\n";
}
echo "---------STEP 4\n";
foreach ($iterator as $item) {
    $iterator->valid();
    echo $item->format("Y-m-d"), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "2022-01-01\n\
2022-01-02\n\
2022-01-03\n\
---------STEP 2\n\
2022-01-01\n\
2022-01-02\n\
2022-01-03\n\
---------STEP 3\n\
2022-01-02\n\
2022-01-03\n\
---------STEP 4\n\
2022-01-02\n\
2022-01-03\n"
    );
}

/// Verifies DatePeriod runtime-validates the `false` arm of a DateTime factory union with
/// php-src's catchable overload `TypeError` instead of rejecting the program statically.
#[test]
fn test_dateperiod_factory_false_arm_throws_php_src_type_error() {
    let out = compile_and_run(
        r#"<?php
try {
    $start = @DateTime::createFromFormat("Y-m-d", "invalid");
    new DatePeriod($start, new DateInterval("P1D"), 2);
} catch (Throwable $error) {
    echo $error::class, ":", $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "TypeError:DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or \
(DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
    );
}

/// Verifies the procedural timezone-offset wrapper rejects both invalid arguments with php-src's
/// precise catchable `TypeError` messages instead of dispatching a method on an invalid receiver.
#[test]
fn test_timezone_offset_get_runtime_argument_types() {
    let out = compile_and_run(
        r#"<?php
$timezone = timezone_open("Europe/London");
$date = date_create("GMT");
foreach ([new stdClass(), 10, null] as $invalid) {
    try {
        timezone_offset_get($invalid, $date);
    } catch (Error $error) {
        echo $error->getMessage(), "\n";
    }
}
foreach ([new stdClass(), 10, null] as $invalid) {
    try {
        timezone_offset_get($timezone, $invalid);
    } catch (Error $error) {
        echo $error->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "timezone_offset_get(): Argument #1 ($object) must be of type DateTimeZone, stdClass given\n\
timezone_offset_get(): Argument #1 ($object) must be of type DateTimeZone, int given\n\
timezone_offset_get(): Argument #1 ($object) must be of type DateTimeZone, null given\n\
timezone_offset_get(): Argument #2 ($datetime) must be of type DateTimeInterface, stdClass given\n\
timezone_offset_get(): Argument #2 ($datetime) must be of type DateTimeInterface, int given\n\
timezone_offset_get(): Argument #2 ($datetime) must be of type DateTimeInterface, null given\n"
    );
}
