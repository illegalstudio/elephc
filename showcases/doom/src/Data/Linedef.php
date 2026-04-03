<?php

namespace Showcases\Doom\Data;

packed class Linedef {
    public int $start_vertex;
    public int $end_vertex;
    public int $flags;
    public int $special_type;
    public int $sector_tag;
    public int $right_sidedef;
    public int $left_sidedef;
}
