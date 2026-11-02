<?php
// C storage widths in packed classes and buffers.
//
// A packed field or a buffer element declared with a C width name (i8, u8, i16, u16, i32, u32,
// f32; i64, u64 and f64 are the plain int and float) keeps that width in memory, so a packed
// class has the layout of the C struct it mirrors and a buffer carries only the bytes the data
// needs. The value still reads as int or float; a write keeps the declared width like a C
// conversion (an out-of-range int keeps its low bits, a float written to f32 rounds).

// SDL2's SDL_Vertex: an SDL_FPoint position, an SDL_Color and an SDL_FPoint tex_coord, 20 bytes.
packed class Vertex {
    public f32 $x;
    public f32 $y;
    public u8 $r;
    public u8 $g;
    public u8 $b;
    public u8 $a;
    public f32 $u;
    public f32 $v;
}

// A broadphase box: an id, four float corners and four int cell bounds.
// With int and float fields this record is 72 bytes; with widths it is 36.
packed class GridBox {
    public i32 $id;
    public f32 $x0;
    public f32 $y0;
    public f32 $x1;
    public f32 $y1;
    public i32 $cx0;
    public i32 $cy0;
    public i32 $cx1;
    public i32 $cy1;
}

echo "sizeof(Vertex) = ", ptr_sizeof("Vertex"), "\n";     // 20, the C sizeof
echo "sizeof(GridBox) = ", ptr_sizeof("GridBox"), "\n";   // 36

// A textured quad: a vertex buffer in the SDL_Vertex shape and a 16-bit index list (the
// size_indices = 2 form of SDL_RenderGeometryRaw). Both reach C through buffer_ptr(): the
// vertex payload as a ptr<Vertex> and the index payload as the const void* of uint16_t.
$quad = buffer_new<Vertex>(4);
$xs = [0.0, 1.0, 1.0, 0.0];
$ys = [0.0, 0.0, 1.0, 1.0];
for ($i = 0; $i < 4; $i++) {
    $quad[$i]->x = $xs[$i] * 320.0;
    $quad[$i]->y = $ys[$i] * 240.0;
    $quad[$i]->u = $xs[$i];
    $quad[$i]->v = $ys[$i];
    $quad[$i]->r = 255;
    $quad[$i]->g = $i * 85;
    $quad[$i]->b = 0;
    $quad[$i]->a = 255;
}
$indices = buffer_new<u16>(6);
$order = [0, 1, 2, 0, 2, 3];
for ($i = 0; $i < 6; $i++) {
    $indices[$i] = $order[$i];
}
for ($i = 0; $i < 6; $i++) {
    $v = $quad[$indices[$i]];
    echo "vertex ", $indices[$i], ": (", $v->x, ", ", $v->y, ") uv (", $v->u, ", ", $v->v, ") rgba ",
        $v->r, " ", $v->g, " ", $v->b, " ", $v->a, "\n";
}

// The payload pointers C would receive, read back here through the raw pointer builtins.
$vertices = buffer_ptr($quad);                          // ptr<Vertex>: the address of $quad[0]
$second = ptr_offset($vertices, ptr_sizeof("Vertex"));
echo "second vertex through its pointer: (", $second->x, ", ", $second->y, ")\n";
$indexData = buffer_ptr($indices);                      // ptr: six consecutive uint16_t
echo "index 4 through its pointer: ", ptr_read16(ptr_offset($indexData, 2 * 4)), "\n";

// Width rules: integers keep their low bits, f32 rounds to single precision.
$quad[0]->g = 300;      // 44, like (uint8_t) 300 in C
$quad[0]->x = 0.1;      // 0.10000000149012, the nearest single
echo "g = ", $quad[0]->g, ", x = ", $quad[0]->x, "\n";

// A broadphase scan over half-size records: the loop reads 36 bytes per box instead of 72.
$boxes = buffer_new<GridBox>(1000);
for ($i = 0; $i < buffer_len($boxes); $i++) {
    $boxes[$i]->id = $i;
    $boxes[$i]->x0 = ($i % 40) * 16.0;
    $boxes[$i]->y0 = intdiv($i, 40) * 16.0;
    $boxes[$i]->x1 = $boxes[$i]->x0 + 24.0;
    $boxes[$i]->y1 = $boxes[$i]->y0 + 24.0;
    $boxes[$i]->cx0 = intdiv($i % 40, 4);
    $boxes[$i]->cy0 = intdiv(intdiv($i, 40), 4);
    $boxes[$i]->cx1 = $boxes[$i]->cx0 + 1;
    $boxes[$i]->cy1 = $boxes[$i]->cy0 + 1;
}
$qx0 = 100.0;
$qy0 = 100.0;
$qx1 = 140.0;
$qy1 = 140.0;
$overlaps = 0;
for ($i = 0; $i < buffer_len($boxes); $i++) {
    if ($boxes[$i]->x0 < $qx1 && $boxes[$i]->x1 > $qx0 && $boxes[$i]->y0 < $qy1 && $boxes[$i]->y1 > $qy0) {
        $overlaps++;
    }
}
echo "boxes overlapping the query: ", $overlaps, "\n";

buffer_free($boxes);
buffer_free($indices);
buffer_free($quad);
