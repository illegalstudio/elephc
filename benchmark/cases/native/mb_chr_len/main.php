<?php
$codepoints = [0x41, 0xA9, 0x20AC, 0x1F600];
$n = 0;
for ($i = 0; $i < 999999 + $argc; $i++) {
    $encoded = mb_chr($codepoints[$i & 3], "UTF-8");
    $n += strlen($encoded);
}
echo $n, "\n";
