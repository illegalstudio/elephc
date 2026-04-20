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
