<?php

namespace Showcases\Doom\Support;

class Direction {
    public $sinTable;

    public function __construct() {
        $this->sinTable = [
            0, 18, 36, 54, 71, 89, 107, 125, 143, 160, 178, 195, 213, 230, 248,
            265, 282, 299, 316, 333, 350, 367, 384, 400, 416, 433, 449, 465, 481,
            496, 512, 527, 543, 558, 573, 587, 602, 616, 630, 644, 658, 672, 685,
            698, 711, 724, 737, 749, 761, 773, 784, 796, 807, 818, 828, 839, 849,
            859, 868, 878, 887, 896, 904, 912, 920, 928, 935, 943, 949, 956, 962,
            968, 974, 979, 984, 989, 994, 998, 1002, 1005, 1008, 1011, 1014, 1016,
            1018, 1020, 1022, 1023, 1023, 1024, 1024
        ];
    }

    public function normalizeAngle(int $angle): int {
        while ($angle < 0) {
            $angle += 360;
        }
        while ($angle >= 360) {
            $angle -= 360;
        }

        return $angle;
    }

    public function unitX(int $angle): int {
        $angle = $this->normalizeAngle($angle);
        if ($angle <= 90) {
            return $this->sinTable[$angle];
        }
        if ($angle <= 180) {
            return $this->sinTable[180 - $angle];
        }
        if ($angle <= 270) {
            return -$this->sinTable[$angle - 180];
        }

        return -$this->sinTable[360 - $angle];
    }

    public function unitY(int $angle): int {
        return -$this->unitX($angle + 90);
    }
}
