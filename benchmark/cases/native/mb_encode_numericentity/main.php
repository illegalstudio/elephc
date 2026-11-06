<?php
function ne_source(): string { return "café 猫 😀 <&>"; }
$map = [128, 1114111, 0, 2097151];
$limit = 499999 + $argc;
$n = 0;
for ($i = 0; $i < $limit; $i++) {
    $encoded = mb_encode_numericentity(ne_source(), $map, "UTF-8");
    $n += strlen($encoded) + ord($encoded[0]);
}
echo $n, "\n";
