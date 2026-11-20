<?php
$n = 0;
for ($i = 0; $i < 499999 + $argc; $i++) {
    $formatted = sprintf("%d:%s:%.2f", $i, "x", 1.5);
    $n += strlen($formatted) + ord($formatted[0]);
}
echo $n, "\n";
