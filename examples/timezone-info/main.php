<?php

// Timezone-database introspection: getLocation(), getTransitions(), and
// listAbbreviations() expose the IANA data PHP ships in its timelib tables.
// elephc bakes those exact tables into a small bridge that is linked only into
// programs that use one of these three methods (or their procedural aliases),
// so other binaries stay lean.

date_default_timezone_set("UTC");

// --- getLocation(): country, coordinates, comment ---------------------------
$paris = new DateTimeZone("Europe/Paris");
$loc = $paris->getLocation();
echo "Paris is in {$loc['country_code']} at {$loc['latitude']}, {$loc['longitude']}\n";

// Zones without a location (the legacy abbreviation-zones) return false.
var_dump((new DateTimeZone("CET"))->getLocation());

// --- getTransitions(): daylight-saving history ------------------------------
// With no arguments, every stored transition is returned. Row 0 is the synthetic
// "before the first transition" row stamped at PHP_INT_MIN.
$all = $paris->getTransitions();
echo "Paris has ", count($all), " transitions; the first abbreviation is ", $all[0]["abbr"], "\n";

// A window returns the state active at the start plus the transitions inside it.
$summer2024 = $paris->getTransitions(
    mktime(0, 0, 0, 1, 1, 2024),
    mktime(0, 0, 0, 12, 31, 2024)
);
foreach ($summer2024 as $t) {
    echo "  ", $t["time"], "  ", $t["abbr"], " (", ($t["isdst"] ? "DST" : "standard"), ")\n";
}

// --- listAbbreviations(): the abbreviation -> zone table --------------------
$abbr = DateTimeZone::listAbbreviations();
echo "Known abbreviations: ", count($abbr), "\n";
echo "CEST first maps to: ", $abbr["cest"][0]["timezone_id"], "\n";

// The procedural aliases call the same tables.
$proc = timezone_location_get(new DateTimeZone("Asia/Tokyo"));
echo "Tokyo country code: ", $proc["country_code"], "\n";
