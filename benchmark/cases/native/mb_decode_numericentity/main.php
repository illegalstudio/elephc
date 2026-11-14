<?php
function nd_source(): string { return "caf&#233; &#29483; &#128512; <&>"; }
$map = [128, 1114111, 0, 2097151];
$limit = 499999 + $argc;
$n = 0;
for ($i = 0; $i < $limit; $i++) {
    $decoded = mb_decode_numericentity(nd_source(), $map, "UTF-8");
    $n += strlen($decoded) + ord($decoded[0]);
}
echo $n, "\n";
