<?php
$input = "ＡＢＣ１２３ａｂｃ";
$limit = 999999 + $argc;
$n = 0;
for ($i = 0; $i < $limit; $i++) {
    $converted = mb_convert_kana($input, "a", "UTF-8");
    $n += strlen($converted) + ord($converted[0]);
}
echo $n, "\n";
