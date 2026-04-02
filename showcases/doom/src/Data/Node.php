<?php

namespace Showcases\Doom\Data;

packed class Node {
    public int $partition_x;
    public int $partition_y;
    public int $delta_x;
    public int $delta_y;
    public int $right_top;
    public int $right_bottom;
    public int $right_left;
    public int $right_right;
    public int $left_top;
    public int $left_bottom;
    public int $left_left;
    public int $left_right;
    public int $right_child;
    public int $left_child;
}
