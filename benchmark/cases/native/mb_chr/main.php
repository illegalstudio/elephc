<?php
$codepoints = [0x41, 0xA9, 0x20AC, 0x1F600];
$limit = 999999 + $argc;
$n = 0;
for ($i = 0; $i < $limit; $i++) {
    $encoded = mb_chr($codepoints[$i & 3], "UTF-8");
    $n += strlen($encoded) + ord($encoded[0]);
}
echo $n, "\n";
