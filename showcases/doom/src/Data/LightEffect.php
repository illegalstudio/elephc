<?php

namespace Showcases\Doom\Data;

// A sector whose light level blinks, strobes, glows or flickers.
packed class LightEffect {
    public int $sector;
    public int $kind;
    public int $min_light;
    public int $max_light;
    public int $count;
    public int $dark_time;
    public int $bright_time;
    public int $direction;
}
