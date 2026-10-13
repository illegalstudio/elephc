<?php
$sum = 0;
$i = 0;
$fragment = '$sum += 3;';
while ($i < 5000) {
    eval($fragment);
    $i += 1;
}
echo $sum . "\n";
