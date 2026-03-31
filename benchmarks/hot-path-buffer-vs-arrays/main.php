<?php
// Micro-benchmark: buffer<T> vs indexed arrays vs associative arrays.

$count = 4000;
$steps = 40;

buffer<float> $buf_x = buffer_new<float>($count);
buffer<float> $buf_y = buffer_new<float>($count);
buffer<float> $buf_vx = buffer_new<float>($count);
buffer<float> $buf_vy = buffer_new<float>($count);

$idx_x = [];
$idx_y = [];
$idx_vx = [];
$idx_vy = [];

$assoc_keys = [];
$assoc_x = ["seed" => 0.0];
$assoc_y = ["seed" => 0.0];
$assoc_vx = ["seed" => 0.0];
$assoc_vy = ["seed" => 0.0];

for ($i = 0; $i < $count; $i = $i + 1) {
    $buf_x[$i] = 0.0;
    $buf_y[$i] = 0.0;
    $buf_vx[$i] = 1.0;
    $buf_vy[$i] = 0.5;

    $idx_x[] = 0.0;
    $idx_y[] = 0.0;
    $idx_vx[] = 1.0;
    $idx_vy[] = 0.5;

    $key = "p" . $i;
    $assoc_keys[] = $key;
    $assoc_x[$key] = 0.0;
    $assoc_y[$key] = 0.0;
    $assoc_vx[$key] = 1.0;
    $assoc_vy[$key] = 0.5;
}

$start = microtime(true);
for ($step = 0; $step < $steps; $step = $step + 1) {
    for ($i = 0; $i < $count; $i = $i + 1) {
        $buf_x[$i] = $buf_x[$i] + $buf_vx[$i];
        $buf_y[$i] = $buf_y[$i] + $buf_vy[$i];
    }
}
$buffer_elapsed = microtime(true) - $start;

$start = microtime(true);
for ($step = 0; $step < $steps; $step = $step + 1) {
    for ($i = 0; $i < $count; $i = $i + 1) {
        $idx_x[$i] = $idx_x[$i] + $idx_vx[$i];
        $idx_y[$i] = $idx_y[$i] + $idx_vy[$i];
    }
}
$indexed_elapsed = microtime(true) - $start;

$start = microtime(true);
for ($step = 0; $step < $steps; $step = $step + 1) {
    for ($i = 0; $i < $count; $i = $i + 1) {
        $key = $assoc_keys[$i];
        $assoc_x[$key] = $assoc_x[$key] + $assoc_vx[$key];
        $assoc_y[$key] = $assoc_y[$key] + $assoc_vy[$key];
    }
}
$assoc_elapsed = microtime(true) - $start;

$buffer_sum = 0;
for ($i = 0; $i < $count; $i = $i + 1) {
    $buffer_sum = $buffer_sum + (int) $buf_x[$i];
}

$indexed_sum = 0;
for ($i = 0; $i < $count; $i = $i + 1) {
    $indexed_sum = $indexed_sum + (int) $idx_x[$i];
}

$assoc_sum = 0;
for ($i = 0; $i < $count; $i = $i + 1) {
    $key = $assoc_keys[$i];
    $assoc_sum = $assoc_sum + (int) $assoc_x[$key];
}

echo "buffer elapsed: ";
echo $buffer_elapsed;
echo "\n";
echo "indexed elapsed: ";
echo $indexed_elapsed;
echo "\n";
echo "assoc elapsed: ";
echo $assoc_elapsed;
echo "\n";
echo "buffer checksum: ";
echo $buffer_sum;
echo "\n";
echo "indexed checksum: ";
echo $indexed_sum;
echo "\n";
echo "assoc checksum: ";
echo $assoc_sum;
echo "\n";
