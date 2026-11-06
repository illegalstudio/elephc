<?php
function magician_callback_add($x) {
    return $x + 2;
}

$sum = 0;
$i = 0;
while ($i < 2500) {
    $sum += call_user_func("magician_callback_add", $i);
    $i += 1;
}
echo $sum . "\n";
