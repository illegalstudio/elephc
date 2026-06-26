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
     date_diff(date_create("@1704067200"), date_create("@1719835200"))->days;
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
$d = date_create();
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

/// Verifies the cross-conversion factories preserve the source instant and display timezone
/// while switching mutability: `createFromInterface`/`createFromImmutable` build a `DateTime`,
/// `createFromMutable`/`createFromInterface` build a `DateTimeImmutable`.
#[test]
fn test_datetime_create_from_object_conversions() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$src = new DateTime("2024-06-15 12:00:00");
$src->setTimezone(new DateTimeZone("Europe/Paris"));
$im = DateTimeImmutable::createFromMutable($src);
$back = DateTime::createFromInterface($im);
$plain = new DateTimeImmutable("2024-03-10 08:30:00");
$dt = DateTime::createFromImmutable($plain);
echo $im->format("Y-m-d H:i"), " ", $im->getTimezone()->getName(), "|",
     $back->format("H:i"), "|", $dt->format("Y-m-d H:i:s");
"#,
    );
    assert_eq!(out, "2024-06-15 14:00 Europe/Paris|14:00|2024-03-10 08:30:00");
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
$d->setISODate(2024, 10, 3);
$im = new DateTimeImmutable("2020-06-15 12:00:00");
$im2 = $im->setISODate(2026, 1, 1);
$e = new DateTime("2024-06-15 00:00:00");
date_isodate_set($e, 2024, 53, 1);
echo $d->format("Y-m-d H:i:s"), "|", $im2->format("Y-m-d"), "|", $im->format("Y-m-d"), "|",
     $e->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2024-03-06 09:30:15|2025-12-29|2020-06-15|2024-12-30");
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
/// `T` (3- or 4-letter abbreviation, matched greedily), and `e` (IANA name) parse and validate the subject substring
/// against the rendered `date("X", $ts)` output of the constructed instant. A mismatch (e.g. `O`
/// `+0500` against a Europe/Paris instant) returns `false`; a round-trip match yields a DateTime
/// that re-formats to the same wall-clock.
#[test]
fn test_create_from_format_tz_specifiers() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
// Paris 2024-07-15 12:00:00 = UTC 10:00, offset +02:00.
$paris = new DateTimeZone("Europe/Paris");
$ts = (new DateTime("2024-07-15 12:00:00", $paris))->getTimestamp();

// O: +0200 — round-trip and mismatch.
$a = DateTime::createFromFormat("Y-m-d H:i:s O", "2024-07-15 12:00:00 +0200", $paris);
echo ($a === false) ? "?" : "O:" . $a->format("Y-m-d H:i:s O") . "|";
$badO = DateTime::createFromFormat("Y-m-d H:i:s O", "2024-07-15 12:00:00 +0500", $paris);
echo ($badO === false) ? "O-bad|": "?|";

// P: +02:00.
$b = DateTime::createFromFormat("Y-m-d H:i:s P", "2024-07-15 12:00:00 +02:00", $paris);
echo ($b === false) ? "?" : "P:" . $b->format("Y-m-d H:i:s P") . "|";
$badP = DateTime::createFromFormat("Y-m-d H:i:s P", "2024-07-15 12:00:00 -05:00", $paris);
echo ($badP === false) ? "P-bad|": "?|";

// Z: +7200 (Paris summer, no colon). Both signed and unsigned accepted.
$c = DateTime::createFromFormat("Y-m-d H:i:s Z", "2024-07-15 12:00:00 +7200", $paris);
echo ($c === false) ? "?" : "Z:" . $c->format("Y-m-d H:i:s Z") . "|";
$d = DateTime::createFromFormat("Y-m-d H:i:s Z", "2024-01-15 12:00:00 +3600", $paris);
echo ($d === false) ? "?" : "Zw:" . $d->format("Y-m-d H:i:s Z") . "|";

// T: 3- or 4-letter abbreviation. libc resolves this to "CEST" for Paris summer.
$e = DateTime::createFromFormat("Y-m-d H:i:s T", "2024-07-15 12:00:00 CEST", $paris);
echo ($e === false) ? "T-false|" : "T:" . $e->format("Y-m-d H:i:s T") . "|";

// e: IANA name. Round-trip with the same zone.
$f = DateTime::createFromFormat("Y-m-d H:i:s e", "2024-07-15 12:00:00 Europe/Paris", $paris);
echo ($f === false) ? "?" : "e:" . $f->format("Y-m-d H:i:s e");
"#,
    );
    // The exact T value is libc-defined (CEST on most platforms); we only assert the parser
    // round-trips whatever date("T") reports, so accept either "T:CEST..." or "T-false|".
    // PHP's Z specifier renders the offset WITHOUT a leading + for positive values
    // (matches our impl); the O specifier DOES include the leading +.
    assert!(
        out == "O:2024-07-15 12:00:00 +0200|O-bad|P:2024-07-15 12:00:00 +02:00|P-bad|Z:2024-07-15 12:00:00 7200|Zw:2024-01-15 12:00:00 3600|T:2024-07-15 12:00:00 CEST|e:2024-07-15 12:00:00 Europe/Paris"
            || out == "O:2024-07-15 12:00:00 +0200|O-bad|P:2024-07-15 12:00:00 +02:00|P-bad|Z:2024-07-15 12:00:00 7200|Zw:2024-01-15 12:00:00 3600|T-false|e:2024-07-15 12:00:00 Europe/Paris",
        "unexpected output: {out}"
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
date_default_timezone_set("UTC");
echo DateTime::createFromTimestamp(1718452800)->format("Y-m-d H:i:s"), "|",
     DateTimeImmutable::createFromTimestamp(1718452800)->getTimestamp(), "|",
     DateTime::createFromTimestamp(0)->format("Y-m-d");
"#,
    );
    assert_eq!(out, "2024-06-15 12:00:00|1718452800|1970-01-01");
}

/// Verifies sub-second support: set/getMicrosecond, `format('u')`/`format('v')` reflecting the stored
/// microseconds (escaped `\u` stays literal), preservation across a mutable setTimestamp and an
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
        "123456|12:00:00.123456|12:00:00.123|12:00:00 u|123456|2025-03-04.000007|654321|0"
    );
}

/// Verifies `getLastErrors()` / `date_get_last_errors()` report whether the last `createFromFormat()`
/// on the class succeeded (`error_count` 0) or failed (`error_count` 1 with one error message). Also
/// exercises the synthetic-class static-property storage fix — `lastErrorCount` is a static on a
/// builtin class, whose `.comm` slot now emits and initializes correctly for the used class.
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
echo $ok["error_count"], "|", $ok["warning_count"], "|", $bad["error_count"], "|",
     count($bad["errors"]), "|", $alias["error_count"];
"#,
    );
    assert_eq!(out, "0|0|1|1|1");
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
date_default_timezone_set("UTC");
// R4 + 7-day interval, no end bound.
$p = DatePeriod::createFromISO8601String("R4/2012-07-01T00:00:00Z/P7D");
$dates = [];
foreach ($p as $d) { $dates[] = $d->format("Y-m-d"); }
echo count($dates), "|", $dates[0], "|", $dates[3], "|";
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
    assert_eq!(out, "5|2012-07-01|2012-07-22|4|2024-01-01|2024-01-03|4|2012-07-01|2012-07-22|111");
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
    assert_eq!(out, "2026.1|1");
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

/// Verifies `timezone_name_from_abbr()` maps common timezone abbreviations to the IANA zone PHP
/// returns (case-insensitively) and yields `false` for unknown abbreviations — including ones PHP
/// itself does not resolve (e.g. "SGT"). Values cross-checked against PHP.
#[test]
fn test_timezone_name_from_abbr() {
    let out = compile_and_run(
        r#"<?php
echo timezone_name_from_abbr("CEST"), "|", timezone_name_from_abbr("est"), "|";
echo timezone_name_from_abbr("JST"), "|", timezone_name_from_abbr("MSK"), "|";
echo (timezone_name_from_abbr("ZZZ") === false ? "F" : "x"), "|";
echo (timezone_name_from_abbr("SGT") === false ? "F" : "x"), "|";
echo function_exists("timezone_name_from_abbr") ? "1" : "0";
"#,
    );
    assert_eq!(out, "Europe/Berlin|America/New_York|Asia/Tokyo|Europe/Moscow|F|F|1");
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

/// Verifies `DatePeriod::getIterator()` returns an iterator over the period's
/// dates, usable with `foreach` and `iterator_to_array` (PHP `IteratorAggregate`
/// surface; elephc's DatePeriod is itself an Iterator and returns `$this`).
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
/// elephc has no `setlocale()`, so the C locale is the only reachable behavior; cross-checked
/// against PHP 8.5.6.
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

/// Verifies modify() applies a "microseconds"/"usec" relative unit (singular and
/// plural, positive and negative) with carry/borrow into the whole second, alone
/// or combined with other clauses, while leaving micro-free modifiers unchanged.
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
echo t("12:00:00", 0, "+1 day");
"#,
    );
    assert_eq!(
        out,
        "00:00:00.500000|00:00:01.500000|00:00:00.900000|01:00:00.500000|00:00:00.000001|00:00:00.500000|12:00:00.000000"
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
$period->rewind();
echo get_class($period->getStartDate()), "|", get_class($period->current());
"#,
    );
    assert_eq!(out, "DateTimeImmutable|DateTimeImmutable");
}

/// Verifies `DateTime::format()` handles PHP 8.2+ expanded-year specifiers `X` and `x`.
#[test]
fn test_datetime_format_expanded_year() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
$a = new DateTime("2024-03-15 12:00:00");
echo $a->format("X-m-d"), "|", $a->format("x-m-d");
"#,
    );
    assert_eq!(out, "+2024-03-15|2024-03-15");
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

/// Verifies `DateUnknownException` exists in the PHP 8.3+ date exception hierarchy.
#[test]
fn test_date_unknown_exception_exists() {
    let out = compile_and_run(
        r#"<?php
try {
    throw new DateUnknownException("test");
} catch (DateException $e) {
    echo get_class($e);
}
"#,
    );
    assert_eq!(out, "DateUnknownException");
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

/// Verifies `date_parse_from_format()` handles textual month names, microseconds, Unix timestamp,
/// and timezone specifiers beyond the numeric fields.
#[test]
fn test_date_parse_from_format_textual_and_extended() {
    let out = compile_and_run(
        r#"<?php
$a = date_parse_from_format("j F Y H:i:s.u", "15 March 2024 14:30:45.123456");
echo $a["year"], "-", $a["month"], "-", $a["day"], " ", $a["hour"], ":", $a["minute"], ":", $a["second"], ".", $a["fraction"], "|", $a["error_count"];
echo "|";
$b = date_parse_from_format("Y-m-d\\TH:i:sP", "2024-06-15T14:30:45+02:00");
echo $b["year"], "-", $b["month"], "-", $b["day"], ",", $b["is_localtime"] ? "local" : "utc";
echo "|";
$c = date_parse_from_format("U", "1700000000");
echo $c["timestamp"];
"#,
    );
    assert_eq!(out, "2024-3-15 14:30:45.123456|0|2024-6-15,local|1700000000");
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
    assert_eq!(out, "2024-3-15|0|v0|1");
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

/// `DateTimeImmutable::getLastErrors()` is the per-class static mirror of
/// `DateTime::getLastErrors()`: after a failed `createFromFormat()` it returns an array whose
/// `error_count` is non-zero. The exact count depends on the specific parse failure and the shared
/// last-errors state, so this asserts only the shape (array + non-zero errors), which both elephc
/// and PHP agree on for an unparseable input.
#[test]
fn test_datetime_immutable_get_last_errors() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("UTC");
DateTimeImmutable::createFromFormat("Y-m-d", "not-a-date");
$e = DateTimeImmutable::getLastErrors();
echo is_array($e) ? "arr" : "no", "|", ($e["error_count"] > 0 ? "err" : "ok");
"#,
    );
    assert_eq!(out, "arr|err");
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
