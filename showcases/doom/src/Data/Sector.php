<?php

namespace Showcases\Doom\Data;

// SECTORS record with flat names resolved to flat indices; $mover is set while a door or lift moves it.
packed class Sector {
    public int $floor_height;
    public int $ceiling_height;
    public int $floor_pic;
    public int $ceiling_pic;
    public int $light_level;
    public int $special;
    public int $tag;
    public int $mover;
}
