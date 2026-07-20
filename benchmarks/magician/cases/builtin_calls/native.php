<?php
$sum = 0;
$i = 0;
while ($i < 3000) {
    $sum += strlen("abcdef") + intval("7");
    $i += 1;
}
echo $sum . "\n";
