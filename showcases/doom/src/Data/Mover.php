<?php

namespace Showcases\Doom\Data;

// A door, lift or floor in motion. Heights are whole map units, speed is units per tic.
packed class Mover {
    public int $active;
    public int $sector;
    public int $kind;
    public int $type;
    public int $direction;
    public int $speed;
    public int $top;
    public int $bottom;
    public int $wait;
    public int $countdown;
    public int $texture;
}
