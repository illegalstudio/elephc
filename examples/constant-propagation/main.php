<?php

if ($argc > 1) {
    $base = 2;
} else {
    $base = 2;
}

$exp = 3;
echo $base ** $exp . "\n";

[$left, $right] = [2, 3];
echo $left ** $right . "\n";

$stable = 2;
for ($i = 0; $i < 3; $i++) {
    echo $i;
}
echo $stable ** 3 . "\n";

$stableTry = 2;
for ($i = 0; $i < 2; $i++) {
    try {
        echo $i;
    } catch (Exception $e) {
        echo 9;
    } finally {
    }
}
echo $stableTry ** 3 . "\n";

$stableForeach = 2;
foreach ([1, 2, 3] as $k => $value) {
    echo $value;
}
echo $stableForeach ** 3 . "\n";

$baseInit = 2;
$i = 0;
for ($exp = 3; $i < 2; $i++) {
    echo $baseInit ** $exp;
}
echo $exp . "\n";

$nestedBase = 2;
$i = 0;
for (; $i < 2; $i++) {
    $j = 0;
    while ($j < 2) {
        echo $j;
        $j++;
    }
}
echo $nestedBase ** 3 . "\n";

$arrayBase = 2;
$items = [];
$i = 0;
for (; $i < 3; $i++) {
    $items[] = $i;
    $items[0] = $i;
    echo $i;
}
echo $arrayBase ** 3 . "\n";
