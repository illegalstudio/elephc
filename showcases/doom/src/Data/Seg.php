<?php

namespace Showcases\Doom\Data;

// SEGS record plus endpoints, sectors, sidedef and length resolved at load time.
packed class Seg {
    public int $start_vertex;
    public int $end_vertex;
    public int $angle;
    public int $linedef;
    public int $direction;
    public int $offset;
    public int $x1;
    public int $y1;
    public int $x2;
    public int $y2;
    public int $front_sector;
    public int $back_sector;
    public int $sidedef;
    public int $length;
}
