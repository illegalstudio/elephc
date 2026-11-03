<?php
// Adversarial set: the cases that broke the previous model were EXTREME magnitudes and
// strings that look numeric but are not. Beyond 2^53 a double comparison silently agrees
// with an exact one on small values and diverges on large ones, so both are present.
$vals = [
    0, 1, -1, 42, -42, PHP_INT_MAX, PHP_INT_MIN, 9007199254740993, -9007199254740993,
    0.0, 1.0, -1.0, 0.5, -0.5, 1.5, 42.0, INF, -INF, NAN, 1e300, -1e300, 9.007199254740993e15,
    true, false, null,
    "", " ", "0", "1", "-1", "42", "42.0", " 42", "42 ", "\t42\n", "0.5", "1e3", "1E3",
    "+1", ".5", "5.", "0x1A", "1_000", "007",
    "9007199254740993", "9223372036854775807", "9223372036854775808", "-9223372036854775808",
    "abc", "42abc", "abc42", "ABC", "a", "b", "Z", "true", "false", "null",
    "INF", "NAN", "1e400", "1e500", "-1e400", "-1e500", "1e309", "-0",
];

function lit($v): string {
    if ($v === null) return "null";
    if ($v === true) return "true";
    if ($v === false) return "false";
    if (is_int($v)) return "int:" . $v;
    if (is_float($v)) {
        if (is_nan($v)) return "float:NAN";
        if (is_infinite($v)) return "float:" . ($v > 0 ? "INF" : "-INF");
        return "float:" . var_export($v, true);
    }
    // A raw tab or newline inside a value would split the TSV and produce impossible
    // readings — `0 <=> ""` came back as 42 before this.
    return "str:" . str_replace(["\\", "\t", "\n", "\r"], ["\\\\", "\\t", "\\n", "\\r"], $v);
}

foreach ($vals as $a) {
    foreach ($vals as $b) {
        echo lit($a), "\t", lit($b), "\t", ($a <=> $b), "\n";
    }
}
