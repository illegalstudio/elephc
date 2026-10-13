<?php
$sum = 0;
$i = 0;
$fragment = 'include "piece.php";';
while ($i < 1000) {
    eval($fragment);
    $i += 1;
}
echo $sum . "\n";
