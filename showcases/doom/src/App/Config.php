<?php

namespace Showcases\Doom\App;

final class Config {
    public int $windowWidth;
    public int $windowHeight;
    public int $renderWidth;
    public int $renderHeight;
    public string $windowTitle;
    public string $wadPath;
    public string $startupMap;
    public int $tickRate;
    public bool $vsync;
    public int $bootDurationMs;
    // load and map messages on stdout
    public bool $verbose;

    public function __construct() {
        // DOOM's 320x200 frame, shown at 4:3 like on the monitors it was drawn for
        $this->windowWidth = 960;
        $this->windowHeight = 720;
        $this->renderWidth = 320;
        $this->renderHeight = 200;
        $this->windowTitle = "elephc DOOM showcase";
        $this->wadPath = "DOOM1.WAD";
        $this->startupMap = "E1M1";
        // the game runs at DOOM's 35 tics per second whatever the frame rate
        $this->tickRate = 35;
        $this->vsync = true;
        $this->bootDurationMs = 0;
        $this->verbose = true;
    }
}
