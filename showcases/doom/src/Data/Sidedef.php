<?php

namespace Showcases\Doom\Data;

// SIDEDEFS record with texture names resolved to texture indices (-1 = none).
packed class Sidedef {
    public int $x_offset;
    public int $y_offset;
    public int $upper;
    public int $lower;
    public int $middle;
    public int $sector;
}
