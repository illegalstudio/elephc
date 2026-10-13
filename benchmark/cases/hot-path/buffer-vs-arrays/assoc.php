<?php
// Equivalent position updates; the runner measures complete process execution.

$count = 4000;
$steps = 40;

$x = [];
$y = [];
$vx = [];
$vy = [];

for ($i = 0; $i < $count; $i++) {
    $key = "p" . $i;
    $x[$key] = 0.0;
    $y[$key] = 0.0;
    $vx[$key] = 1.0;
    $vy[$key] = 0.5;
}

for ($step = 0; $step < $steps; $step++) {
    for ($i = 0; $i < $count; $i++) {
        $key = "p" . $i;
        $x[$key] = $x[$key] + $vx[$key];
        $y[$key] = $y[$key] + $vy[$key];
    }
}

$sum = 0;
for ($i = 0; $i < $count; $i++) {
    $key = "p" . $i;
    $sum += (int) $x[$key];
}
echo $sum . "\n";
