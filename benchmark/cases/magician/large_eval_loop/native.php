<?php
$sum = 0;
$i = 0;
while ($i < 6000) {
    $sum += ($i % 17) * 3;
    $i += 1;
}
echo $sum . "\n";
