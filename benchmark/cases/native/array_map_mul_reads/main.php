<?php
$values = range(1, 100);
$n = 0;
for ($i = 0; $i < 4999 + $argc; $i++) {
    $mapped = array_map(fn($v) => $v * 3, $values);
    $n += $mapped[10] + $mapped[50] + $mapped[90];
}
echo $n, "\n";
