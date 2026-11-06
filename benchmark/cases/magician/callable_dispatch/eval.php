<?php
eval('function magician_callback_add($x) { return $x + 2; }');

$sum = 0;
$i = 0;
$callback = "magician_callback_add";
$fragment = '$sum += call_user_func($callback, $i);';
while ($i < 2500) {
    eval($fragment);
    $i += 1;
}
echo $sum . "\n";
