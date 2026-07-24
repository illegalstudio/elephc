<?php

namespace Showcases\Doom\Player;

class Camera {
    public int $x = 0;
    public int $y = 0;
    public int $z = 0;
    public int $angle = 0;

    public function __construct() {
    }

    public function setSpawn(int $x, int $y, int $angle): void {
        $this->x = $x;
        $this->y = $y;
        $this->angle = $angle;
    }

    public function moveBy(int $dx, int $dy): void {
        $this->x = $this->x + $dx;
        $this->y = $this->y + $dy;
    }

    public function rotateBy(int $delta): void {
        $this->angle = $this->angle + $delta;
        if ($this->angle < 0) {
            $this->angle = $this->angle + 360;
        }
        if ($this->angle >= 360) {
            $this->angle = $this->angle - 360;
        }
    }
}
