<?php
$sum = 0;
$i = 0;
while ($i < 1000) {
    $sum += $i % 11;
    $i += 1;
}
echo $sum . "\n";
