<?php
/**
 * Generator for the elephc-tz bridge's embedded IANA tables.
 *
 * elephc's DateTimeZone::getLocation()/getTransitions()/listAbbreviations() must
 * be byte-for-byte identical to PHP. getLocation and listAbbreviations are
 * PHP-internal static tables (timelib) with no algorithm to recompute, and
 * getTransitions cannot be recomputed from the slim IANA TZif that Rust tz
 * crates ship (PHP uses fat data with every recurring DST transition expanded,
 * and Ramadan zones are not POSIX-expressible). So we bake all three directly
 * from the running PHP interpreter into committed data files that the crate
 * embeds with include_str!.
 *
 * The transition `time` string is NOT baked: the crate recomputes it from `ts`
 * at runtime with a proleptic-Gregorian formatter, saving ~1 MB and staying
 * exact even at i64::MIN.
 *
 * Run from the crate root (requires the PHP CLI whose tables you want to match):
 *     php data/generate.php
 *
 * Re-run only to re-pin to a new PHP/timelib tz release; commit the outputs.
 */

$dir = __DIR__;
$zones = DateTimeZone::listIdentifiers(DateTimeZone::ALL_WITH_BC);

// ---- transitions.data --------------------------------------------------------
// One line per zone:
//   "<zone>\tF"                 zone whose getTransitions() returns false
//   "<zone>\t=<canonical>"      identical transition block to an earlier zone
//   "<zone>\t<rows>"            rows = ts,off,dst,abbr;ts,off,dst,abbr;...
// `ts` is the decimal timestamp (PHP_INT_MIN for row 0), `dst` is 0/1. The
// `time` field is intentionally omitted (recomputed by the crate from `ts`).
$transLines = [];
$seenBlocks = [];
foreach ($zones as $zone) {
    $t = (new DateTimeZone($zone))->getTransitions();
    if ($t === false) {
        $transLines[] = $zone . "\tF";
        continue;
    }
    $rows = [];
    foreach ($t as $r) {
        $abbr = $r['abbr'];
        if (strpbrk($abbr, ",;\t\n") !== false) {
            fwrite(STDERR, "FATAL: abbr '$abbr' in $zone contains a delimiter\n");
            exit(1);
        }
        $rows[] = (string) $r['ts'] . ',' . (string) $r['offset'] . ',' . ($r['isdst'] ? '1' : '0') . ',' . $abbr;
    }
    $block = implode(';', $rows);
    if (isset($seenBlocks[$block])) {
        $transLines[] = $zone . "\t=" . $seenBlocks[$block];
    } else {
        $seenBlocks[$block] = $zone;
        $transLines[] = $zone . "\t" . $block;
    }
}
file_put_contents($dir . '/transitions.data', implode("\n", $transLines) . "\n");

// ---- location.data -----------------------------------------------------------
// One line per zone:
//   "<zone>\tF"                                 getLocation() returns false
//   "<zone>\t<cc>\t<lat>\t<lon>\t<comments>"    otherwise
// lat/lon use json_encode (shortest round-trip; (float) of it reproduces the
// exact IEEE-754 value). comments are verified tab/newline-free.
$locLines = [];
foreach ($zones as $zone) {
    $l = (new DateTimeZone($zone))->getLocation();
    if ($l === false) {
        $locLines[] = $zone . "\tF";
        continue;
    }
    $comments = $l['comments'];
    if (strpbrk($comments, "\t\n") !== false) {
        fwrite(STDERR, "FATAL: comments for $zone contain a tab/newline\n");
        exit(1);
    }
    $locLines[] = implode("\t", [
        $zone,
        $l['country_code'],
        json_encode($l['latitude']),
        json_encode($l['longitude']),
        $comments,
    ]);
}
file_put_contents($dir . '/location.data', implode("\n", $locLines) . "\n");

// ---- abbreviations.data ------------------------------------------------------
// One line per abbreviation, in PHP's iteration order:
//   "<abbr>\t<dst>:<off>:<id>;<dst>:<off>:<id>;..."
// `dst` is 0/1, `id` is the timezone identifier or empty for a null id (PHP
// never emits an empty-string id, so empty unambiguously decodes back to null).
$abbrLines = [];
foreach (timezone_abbreviations_list() as $abbr => $rows) {
    if (strpbrk((string) $abbr, ":;\t\n") !== false) {
        fwrite(STDERR, "FATAL: abbreviation key '$abbr' contains a delimiter\n");
        exit(1);
    }
    $encoded = [];
    foreach ($rows as $r) {
        $id = $r['timezone_id'];
        if ($id !== null && strpbrk($id, ":;\t\n") !== false) {
            fwrite(STDERR, "FATAL: timezone_id '$id' contains a delimiter\n");
            exit(1);
        }
        $encoded[] = ($r['dst'] ? '1' : '0') . ':' . (string) $r['offset'] . ':' . ($id ?? '');
    }
    $abbrLines[] = $abbr . "\t" . implode(';', $encoded);
}
file_put_contents($dir . '/abbreviations.data', implode("\n", $abbrLines) . "\n");

// ---- version.data -------------------------------------------------------------
// The timelib/IANA release the running PHP reports via timezone_version_get(),
// baked alongside the tables above so the compiler's timezone_version_get() and
// the embedded transitions/location/abbreviations data always share one provenance.
// A single line, trimmed at read time; re-running this script re-pins it in lockstep.
file_put_contents($dir . '/version.data', timezone_version_get() . "\n");

fwrite(STDERR, sprintf(
    "generated: %d zones (transitions+location), %d abbreviations, version %s\n",
    count($zones),
    count($abbrLines),
    timezone_version_get()
));
