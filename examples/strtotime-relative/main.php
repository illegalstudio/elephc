<?php

$keyword = strtotime("today");
echo "today midnight = " . date("Y-m-d H:i:s", $keyword) . "\n";

$offset = strtotime("+1 day 2 hours");
echo "+1 day 2 hours = " . date("Y-m-d H:i:s", $offset) . "\n";

$past = strtotime("3 days ago");
echo "3 days ago = " . date("Y-m-d", $past) . "\n";

$article = strtotime("a day ago");
echo "a day ago = " . date("Y-m-d H:i", $article) . "\n";

$hour = strtotime("an hour");
echo "an hour = " . date("H:i", $hour) . "\n";

$weekday = strtotime("next Monday");
echo "next Monday = " . date("Y-m-d (D)", $weekday) . "\n";

$noon = strtotime("noon");
echo "noon = " . date("H:i:s", $noon) . "\n";

$time_only = strtotime("14:30");
echo "14:30 = " . date("H:i:s", $time_only) . "\n";

$iso = strtotime("2024-06-15 09:00:00");
echo "ISO datetime = " . date("Y-m-d H:i:s", $iso) . "\n";

// A trailing timezone on an ISO datetime is honored: a numeric UTC offset or an IANA zone
// name (resolved with daylight-saving from the system database). Shown in UTC so the wall-clock
// shift is visible: 12:00 +0200 is 10:00 UTC; 12:00 in New York (EDT in June) is 16:00 UTC.
$with_offset = strtotime("2024-06-15 12:00:00 +0200");
echo "with +0200  = " . gmdate("Y-m-d H:i:s", $with_offset) . " UTC\n";
$in_new_york = strtotime("2024-06-15 12:00:00 America/New_York");
echo "in New York = " . gmdate("Y-m-d H:i:s", $in_new_york) . " UTC\n";

// Absolute formats (deterministic, independent of the current time).
echo "@epoch = " . strtotime("@1700000000") . "\n";
echo "US slash = " . date("Y-m-d", strtotime("12/25/2024")) . "\n";
echo "textual = " . date("Y-m-d", strtotime("25 December 2024")) . "\n";

// Calendar phrases resolved against a fixed base timestamp (2024-06-15 12:00).
$base = mktime(12, 0, 0, 6, 15, 2024);
echo "first day of next month = " . date("Y-m-d", strtotime("first day of next month", $base)) . "\n";
echo "last day of this month  = " . date("Y-m-d", strtotime("last day of this month", $base)) . "\n";
echo "first monday of next month = " . date("Y-m-d", strtotime("first monday of next month", $base)) . "\n";
echo "last friday of this month  = " . date("Y-m-d", strtotime("last friday of this month", $base)) . "\n";
