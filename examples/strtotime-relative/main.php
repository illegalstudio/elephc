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
