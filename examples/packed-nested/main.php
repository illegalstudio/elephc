<?php
// Nested packed records: a packed field whose type is another packed class is embedded in
// place, so `$bodies[$i]->pos->x` is one load at a compile-time offset.

extern function calloc(u64 $count, u64 $size): ptr;
extern function free(ptr $p): void;

packed class Vec2 {
    public float $x;
    public float $y;
}

packed class Body {
    public Vec2 $pos;
    public Vec2 $vel;
    public int $id;
}

packed class Transform {
    public Vec2 $position;
    public Vec2 $scale;
    public float $rotation;
}

echo "Vec2 = ", ptr_sizeof("Vec2"), " bytes, Body = ", ptr_sizeof("Body"), " bytes\n";

$bodies = buffer_new<Body>(3);
for ($i = 0; $i < buffer_len($bodies); $i++) {
    $bodies[$i]->id = $i + 1;
    $bodies[$i]->vel->x = 1.5 * (float) ($i + 1);
    $bodies[$i]->vel->y = -0.5;
}

// Integrate a few frames through the nested fields.
for ($frame = 0; $frame < 4; $frame++) {
    for ($i = 0; $i < buffer_len($bodies); $i++) {
        $bodies[$i]->pos->x += $bodies[$i]->vel->x * 0.5;
        $bodies[$i]->pos->y += $bodies[$i]->vel->y * 0.5;
    }
}

for ($i = 0; $i < buffer_len($bodies); $i++) {
    echo "body ", $bodies[$i]->id, " at ", $bodies[$i]->pos->x, ",", $bodies[$i]->pos->y, "\n";
}

// Assigning a whole nested record copies its bytes.
$transforms = buffer_new<Transform>(1);
$transforms[0]->position = $bodies[2]->pos;
$transforms[0]->scale->x = 2.0;
$transforms[0]->scale->y = 2.0;
$transforms[0]->rotation = 0.25;
$bodies[2]->pos->x = 0.0;
echo "transform at ", $transforms[0]->position->x, ",", $transforms[0]->position->y;
echo " scale ", $transforms[0]->scale->x, " rotation ", $transforms[0]->rotation, "\n";

// The same layout works through a typed pointer into raw memory.
$raw = calloc(1, ptr_sizeof("Body"));
$native = ptr_cast<Body>($raw);
$native->pos->y = 4.5;
$native->id = 99;
echo "native body ", $native->id, " y=", $native->pos->y, " word[1]=", ptr_read_f64(ptr_offset($raw, 8)), "\n";
free($raw);
