<?php
$sum = 0;
$a = $argc + 5;
$b = $argc + 2;
for ($i = 0; $i < 4999999 + $argc; $i++) {
    $sum += $a / $b;
}
echo $sum, "\n";
