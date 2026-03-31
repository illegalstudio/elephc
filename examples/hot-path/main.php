<?php
// Hot-path data example: packed records stored in a contiguous buffer.

packed class Particle {
    public float $x;
    public float $y;
    public float $vx;
    public float $vy;
    public bool $active;
}

$particles = buffer_new<Particle>(4);

$particles[0]->x = 10.0;
$particles[0]->y = 20.0;
$particles[0]->vx = 1.5;
$particles[0]->vy = -0.5;
$particles[0]->active = true;

$particles[1]->x = 3.0;
$particles[1]->y = 7.0;
$particles[1]->vx = 0.25;
$particles[1]->vy = 0.75;
$particles[1]->active = true;

$particles[2]->x = 100.0;
$particles[2]->y = 100.0;
$particles[2]->vx = 9.0;
$particles[2]->vy = 9.0;
$particles[2]->active = false;

$particles[3]->x = -5.0;
$particles[3]->y = 1.0;
$particles[3]->vx = 2.0;
$particles[3]->vy = 1.0;
$particles[3]->active = true;

for ($step = 0; $step < 5; $step = $step + 1) {
    for ($i = 0; $i < buffer_len($particles); $i = $i + 1) {
        if ($particles[$i]->active) {
            $particles[$i]->x = $particles[$i]->x + $particles[$i]->vx;
            $particles[$i]->y = $particles[$i]->y + $particles[$i]->vy;
        }
    }
}

for ($i = 0; $i < buffer_len($particles); $i = $i + 1) {
    echo "particle ";
    echo $i;
    echo ": ";
    echo (int) $particles[$i]->x;
    echo ",";
    echo (int) $particles[$i]->y;
    echo " active=";
    echo $particles[$i]->active ? "yes" : "no";
    echo "\n";
}
