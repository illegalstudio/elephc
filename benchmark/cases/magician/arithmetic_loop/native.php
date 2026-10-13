<?php
$value = 1;
$i = 0;
while ($i < 4000) {
    $value = ($value * 3 + $i) % 1000003;
    $i += 1;
}
echo $value . "\n";
