<?php
$sum = 0;
$i = 0;
while ($i < 5000) {
    eval('$sum = $sum + 3;');
    $i += 1;
}
echo $sum . "\n";
