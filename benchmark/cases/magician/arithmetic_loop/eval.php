<?php
$value = 1;
$i = 0;
$fragment = '$value = ($value * 3 + $i) % 1000003;';
while ($i < 4000) {
    eval($fragment);
    $i += 1;
}
echo $value . "\n";
