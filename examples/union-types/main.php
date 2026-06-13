<?php

function show_union_state() {
    ?int $count = null;
    echo $count ?? 41;
    echo ":";

    int|string $status = "ready";
    echo gettype($status);
    echo ":";
    echo $status;
}

// `int|false` is PHP's classic "index, or false when not found" return shape.
function index_of(string $haystack, string $needle): int|false {
    return strpos($haystack, $needle);
}

// `string|null` is identical to the nullable shorthand `?string`.
function trimmed_or_null(string $value): string|null {
    $value = trim($value);
    return $value === "" ? null : $value;
}

show_union_state();
echo ":";
$pos = index_of("elephant", "ph");
echo $pos === false ? "miss" : $pos;
echo ":";
echo trimmed_or_null("  hi  ") ?? "none";
echo ":";
echo trimmed_or_null("   ") ?? "none";
echo "\n";
