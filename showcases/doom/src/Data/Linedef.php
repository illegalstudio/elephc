<?php

namespace Showcases\Doom\Data;

// LINEDEFS record plus endpoints and sectors resolved at load time; $mapped turns
// on once the line has been drawn, so the automap shows only what was seen.
packed class Linedef {
    public int $mapped;
    public int $start_vertex;
    public int $end_vertex;
    public int $flags;
    public int $special;
    public int $tag;
    public int $right_side;
    public int $left_side;
    public int $front_sector;
    public int $back_sector;
    public int $x1;
    public int $y1;
    public int $x2;
    public int $y2;
}
