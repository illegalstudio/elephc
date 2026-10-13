<?php
$data = [];
$i = 0;
while ($i < 50000) {
    $data[] = $i % 97;
    $i += 1;
}

$sum = 0;
$i = 0;
while ($i < count($data)) {
    $sum += $data[$i];
    $i += 1;
}

echo $sum . "\n";
