<?php
// Equivalent position updates; the runner measures complete process execution.

$count = 4000;
$steps = 40;

buffer<float> $x = buffer_new<float>($count);
buffer<float> $y = buffer_new<float>($count);
buffer<float> $vx = buffer_new<float>($count);
buffer<float> $vy = buffer_new<float>($count);

for ($i = 0; $i < $count; $i++) {
    $x[$i] = 0.0;
    $y[$i] = 0.0;
    $vx[$i] = 1.0;
    $vy[$i] = 0.5;
}

for ($step = 0; $step < $steps; $step++) {
    for ($i = 0; $i < $count; $i++) {
        $x[$i] = $x[$i] + $vx[$i];
        $y[$i] = $y[$i] + $vy[$i];
    }
}

$sum = 0;
for ($i = 0; $i < $count; $i++) {
    $sum += (int) $x[$i];
}
echo $sum . "\n";
