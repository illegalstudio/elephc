<?php

namespace Showcases\Doom\App;

class Config {
    public $windowWidth;
    public $windowHeight;
    public $windowTitle;
    public $wadPath;
    public $backgroundR;
    public $backgroundG;
    public $backgroundB;
    public $targetFrameMs;
    public $bootDurationMs;

    public function __construct() {
        $this->windowWidth = 960;
        $this->windowHeight = 600;
        $this->windowTitle = "elephc DOOM showcase";
        $this->wadPath = "DOOM1.WAD";
        $this->backgroundR = 18;
        $this->backgroundG = 22;
        $this->backgroundB = 30;
        $this->targetFrameMs = 16;
        $this->bootDurationMs = 3000;
    }
}
