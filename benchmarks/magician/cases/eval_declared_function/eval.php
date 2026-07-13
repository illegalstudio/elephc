<?php
eval('function magician_declared_add($x) { return $x + 4; }');

$sum = 0;
$i = 0;
$fragment = '$sum += magician_declared_add($i);';
while ($i < 3000) {
    eval($fragment);
    $i += 1;
}
echo $sum . "\n";
