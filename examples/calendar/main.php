<?php
// The ext/calendar extension: convert between calendars via the Julian Day Number.

// Gregorian <-> Julian Day, and back as an "m/d/y" string.
$jd = gregoriantojd(7, 14, 1789);          // storming of the Bastille
echo "Bastille day JD: ", $jd, "\n";
echo "Back to Gregorian: ", jdtogregorian($jd), "\n";

// The same day in the French Republican and Jewish calendars.
echo "Day of week: ", jddayofweek($jd, CAL_DOW_LONG), "\n";

// Easter (Western, Gregorian computation).
echo "Easter 2024 falls ", easter_days(2024), " days after March 21\n";
echo "Easter 2024 date: ", gmdate("Y-m-d", easter_date(2024)), "\n";

// Days in a month, across calendars.
echo "Feb 2024 (Gregorian): ", cal_days_in_month(CAL_GREGORIAN, 2, 2024), " days\n";
echo "Tishri 5784 (Jewish): ", cal_days_in_month(CAL_JEWISH, 1, 5784), " days\n";

// A full breakdown of a Julian Day.
$info = cal_from_jd(gregoriantojd(1, 1, 2000), CAL_GREGORIAN);
echo "Y2K was a ", $info["dayname"], " in ", $info["monthname"], "\n";

// Unix time <-> Julian Day.
echo "JD of the Unix epoch: ", unixtojd(0), "\n";
