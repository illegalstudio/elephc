<?php

namespace Showcases\Doom\Data;

// A spawned map object: position, look (sprite frames cycled every $tics) and size.
// $active drops to 0 when the player picks it up.
packed class Mobj {
    public int $active;
    public int $x;
    public int $y;
    public int $z;
    public int $angle;
    public int $type;
    public int $kind;
    public int $sector;
    public int $first_definition;
    public int $frames;
    public int $tics;
    public int $phase;
    public int $bright;
    public int $radius;
    public int $height;
    public int $solid;
}
