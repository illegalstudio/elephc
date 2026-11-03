<?php
// Random pairs concentrated on the boundaries that broke the previous model: the i64
// edges, 2^53, and strings that are numeric, nearly numeric, or not at all.
mt_srand(20260807);

function pick() {
    switch (mt_rand(0, 7)) {
        case 0: return mt_rand(-100, 100);
        case 1: return (int)(PHP_INT_MAX - mt_rand(0, 5));
        case 2: return (int)(PHP_INT_MIN + mt_rand(0, 5));
        case 3: return (mt_rand(0, 1) ? 1 : -1) * (9007199254740992 + mt_rand(-3, 3));
        case 4: return mt_rand(-1000, 1000) / (mt_rand(1, 8));
        case 5: return (string)(mt_rand(-1000, 1000));
        case 6: {
            $pool = ["abc", "42abc", " 42", "42 ", "0x1A", "1_000", "", "0", "007",
                     "9223372036854775807", "9223372036854775808", "1e3", ".5", "INF"];
            return $pool[mt_rand(0, count($pool) - 1)];
        }
        default: {
            $pool = [true, false, null, INF, -INF, NAN, 0.0, -0.0];
            return $pool[mt_rand(0, count($pool) - 1)];
        }
    }
}

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
    return "str:" . str_replace(["\\", "\t", "\n", "\r"], ["\\\\", "\\t", "\\n", "\\r"], $v);
}

for ($i = 0; $i < 4000; $i++) {
    $a = pick();
    $b = pick();
    echo lit($a), "\t", lit($b), "\t", ($a <=> $b), "\n";
}
