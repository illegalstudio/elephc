<?php
$values = range(1, 100);
$n = 0;
for ($i = 0; $i < 4999 + $argc; $i++) {
    $n += array_map(fn($v) => $v * 3, $values)[10];
}
echo $n, "\n";
