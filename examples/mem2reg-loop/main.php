<?php

$limit = ($argc + 4) & 65535;
$counter = 0;
$sum = 0;

while ($counter < $limit) {
    $sum = ($sum + $counter) & 65535;
    $counter++;
}

echo $sum;
