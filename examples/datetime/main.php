<?php

// Time zones
$tz = new DateTimeZone("Europe/Paris");
echo "Zone: " . $tz->getName() . "\n";

// Build a specific moment. setDate()/setTime() fix the wall clock, so the
// output is independent of the machine's time zone.
$dt = new DateTime();
$dt->setDate(2024, 1, 15);
$dt->setTime(9, 30, 0);
echo "Date: " . $dt->format("Y-m-d H:i:s") . "\n";

// setISODate() sets the date from an ISO 8601 week date (year, week, weekday 1=Mon).
$iso = new DateTime("2024-01-01 12:00:00");
$iso->setISODate(2024, 23, 3);
echo "ISO 2024-W23-3: " . $iso->format("Y-m-d") . "\n";

// Passing a timezone interprets the wall-clock string as local time in that zone;
// createFromMutable() then makes an immutable copy, preserving the instant and zone.
$parisNoon = new DateTime("2024-07-01 12:00:00", new DateTimeZone("Europe/Paris"));
$frozen = DateTimeImmutable::createFromMutable($parisNoon);
echo "Noon in Paris: " . $frozen->format("H:i") . " " . $frozen->getTimezone()->getName() . "\n";

// Add a calendar interval (mutates $dt). mktime() normalizes month/day overflow.
$dt->add(new DateInterval("P1Y2M10D"));
echo "Plus 1y2m10d: " . $dt->format("Y-m-d") . "\n";

// Subtract an interval.
$dt->sub(new DateInterval("P3M"));
echo "Minus 3m: " . $dt->format("Y-m-d") . "\n";

// modify() applies a relative string (parsed like strtotime) to the current value.
$dt->modify("+10 days");
echo "Plus 10 days: " . $dt->format("Y-m-d") . "\n";

// DateTimeImmutable returns a NEW instance from each modifier; the original is untouched.
$base = (new DateTimeImmutable())->setDate(2024, 1, 15)->setTime(0, 0, 0);
$later = $base->add(new DateInterval("PT2H30M"));
echo "Immutable base:    " . $base->format("H:i:s") . "\n";
echo "Immutable +2h30m:  " . $later->format("H:i:s") . "\n";

// DateInterval parses an ISO 8601 duration into its components.
$iv = new DateInterval("P1Y2M3DT4H5M6S");
echo "Interval: " . $iv->y . "y " . $iv->m . "m " . $iv->d . "d "
    . $iv->h . "h " . $iv->i . "m " . $iv->s . "s\n";

// createFromDateString() builds an interval from a relative phrase ("2 weeks" = 14 days).
$rel = DateInterval::createFromDateString("2 weeks 3 days");
echo "Relative: " . $rel->d . " days\n";

// diff() returns a DateInterval with the calendar breakdown AND the total day count.
$a = new DateTime();
$a->setDate(2020, 1, 1);
$a->setTime(0, 0, 0);
$b = new DateTime();
$b->setDate(2021, 3, 15);
$b->setTime(0, 0, 0);
$d = $a->diff($b);
echo "Diff: " . $d->y . "y " . $d->m . "m " . $d->d . "d ("
    . $d->days . " days total)\n";

// DateInterval::format() renders an interval with PHP's % specifiers.
echo "Formatted: " . $d->format("%a days = %y years, %m months, %d days") . "\n";

// $days is int|false: diff() fills the real total, but a directly-built interval
// has days === false (so %a renders "(unknown)").
$twoWeeks = new DateInterval("P2W");
echo "P2W days: " . ($twoWeeks->days === false ? "unknown" : $twoWeeks->days)
    . " (%a = " . $twoWeeks->format("%a") . ")\n";

// DateTime/DateTimeImmutable compare by their absolute instant (timestamp +
// microsecond), independent of timezone and across both classes. Identity ===
// still compares object references.
$utc = new DateTime("2024-06-15 12:00:00", new DateTimeZone("UTC"));
$ny = new DateTime("2024-06-15 08:00:00", new DateTimeZone("America/New_York"));
echo "Same instant, diff zone: " . ($utc == $ny ? "equal" : "differ")
    . " (<=> " . ($utc <=> $ny) . ")\n";
$earlier = new DateTimeImmutable("2024-01-01");
$later2 = new DateTime("2024-06-01");
echo "Jan < Jun: " . ($earlier < $later2 ? "yes" : "no") . "\n";

// usort() sorts an array of objects: the comparator's parameters are typed from
// the array element, so an unannotated `$a <=> $b` compares DateTimes by instant.
$schedule = [
    new DateTime("2024-06-01"),
    new DateTime("2024-01-15"),
    new DateTime("2024-03-20"),
];
usort($schedule, fn($a, $b) => $a <=> $b);
echo "Sorted:";
foreach ($schedule as $when) {
    echo " " . $when->format("m-d");
}
echo "\n";

// DatePeriod implements Iterator: foreach walks each step of a date range.
// The end date is exclusive by default.
$period = new DatePeriod(
    new DateTime("2024-01-01"),
    new DateInterval("P1M"),
    new DateTime("2024-04-01")
);
echo "Quarter starts:";
foreach ($period as $month) {
    echo " " . $month->format("M");
}
echo "\n";

// Passing an integer instead of an end date repeats the interval that many times:
// the start plus 4 more days here. getRecurrences() reports the count.
$week = new DatePeriod(new DateTime("2024-01-01"), new DateInterval("P1D"), 4);
echo "Next 5 days:";
foreach ($week as $day) {
    echo " " . $day->format("d");
}
echo " (recurrences: " . $week->getRecurrences() . ")\n";

// Default timezone: date_default_timezone_set() makes date() format in any IANA
// zone, with the correct UTC offset and daylight-saving rules from the system
// timezone database. 2024-07-01 12:00 UTC is summer (CEST = UTC+2 in Paris).
$summer = 1719835200;
date_default_timezone_set("Europe/Paris");
// The T/e specifiers name the zone (abbreviation / identifier).
echo "Paris:    " . date("Y-m-d H:i", $summer) . " " . date("T", $summer) . " (" . date("e", $summer) . ")\n";
// The O/P/Z specifiers render the zone's UTC offset (DST-aware): Paris is +02:00 in summer.
echo "ISO 8601: " . date('Y-m-d\TH:i:sP', $summer) . " (" . date("Z", $summer) . "s)\n";
date_default_timezone_set("America/New_York");
echo "New York: " . date("Y-m-d H:i", $summer) . " " . date("P", $summer) . "\n";
echo "Zone is now: " . date_default_timezone_get() . "\n";

// A DateTime carries its own timezone. setTimezone() re-projects the same absolute
// instant onto a different zone without changing the underlying moment.
$moment = new DateTime("@$summer");
$moment->setTimezone(new DateTimeZone("Europe/Paris"));
echo "Moment in Paris:    " . $moment->format("Y-m-d H:i") . "\n";
$moment->setTimezone(new DateTimeZone("America/New_York"));
echo "Same instant in NY: " . $moment->format("Y-m-d H:i") . "\n";

// DateTimeZone::getOffset() reports a zone's UTC offset (seconds) for a given instant, DST-aware.
$paris = new DateTimeZone("Europe/Paris");
echo "Paris offset: " . $paris->getOffset(new DateTime("@$summer")) . "s\n";

// createFromFormat() is the inverse of format(): it parses a string with an explicit format.
date_default_timezone_set("UTC");
$parsed = DateTime::createFromFormat("d/m/Y H:i", "15/03/2024 14:30");
echo "Parsed:   " . $parsed->format("Y-m-d H:i:s") . "\n";
// A leading '!' zeroes the fields the format does not mention; an unmatched subject yields false.
$dayOnly = DateTime::createFromFormat("!Y-m-d", "2024-03-15");
echo "Day only: " . $dayOnly->format("Y-m-d H:i:s") . "\n";
var_dump(DateTime::createFromFormat("Y-m-d", "not a date") === false);

// createFromISO8601String() builds a DatePeriod from an RFC 5545 repeating-interval
// specification: `R<n>/<start>[/<interval>[/<end>]]` (PHP 8.3+). Forwards to the
// regular (start, interval, end|recurrences) constructor; throws
// DateMalformedPeriodStringException on a malformed specification (PHP 8.3+).
foreach (DatePeriod::createFromISO8601String("R4/2024-01-01T00:00:00Z/P1D/2024-01-10T00:00:00Z") as $d) {
    echo "ISO period: " . $d->format("Y-m-d") . "\n";
}
try {
    DatePeriod::createFromISO8601String("R0/2024-01-01T00:00:00Z/P1D");
} catch (DateMalformedPeriodStringException $e) {
    echo "ISO error: " . $e->getMessage() . "\n";
}

// listIdentifiers() can filter the IANA zone list by region-group bitmask, or by
// country with DateTimeZone::PER_COUNTRY (timezone_identifiers_list() is the alias).
$europe = DateTimeZone::listIdentifiers(DateTimeZone::EUROPE);
echo "Europe zones: " . count($europe) . " (first " . $europe[0] . ")\n";
$france = DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY, "FR");
echo "France zones: " . implode(", ", $france) . "\n";
