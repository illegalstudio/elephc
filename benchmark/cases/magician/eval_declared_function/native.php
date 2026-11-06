<?php
function magician_declared_add($x) {
    return $x + 4;
}

$sum = 0;
$i = 0;
while ($i < 3000) {
    $sum += magician_declared_add($i);
    $i += 1;
}
echo $sum . "\n";
