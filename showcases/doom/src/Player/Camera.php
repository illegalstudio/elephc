<?php

namespace Showcases\Doom\Player;

class Camera {
    public $x;
    public $y;
    public $angle;

    public function __construct() {
        $this->x = 0;
        $this->y = 0;
        $this->angle = 0;
    }
}
