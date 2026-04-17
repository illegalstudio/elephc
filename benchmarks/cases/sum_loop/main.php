<?php
$sum = 0;
$i = 1;
while ($i <= 200000) {
    $sum += $i;
    $i += 1;
}
echo $sum . "\n";
