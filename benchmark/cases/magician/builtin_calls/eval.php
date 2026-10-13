<?php
$sum = 0;
$i = 0;
$fragment = '$sum += strlen("abcdef") + intval("7");';
while ($i < 3000) {
    eval($fragment);
    $i += 1;
}
echo $sum . "\n";
