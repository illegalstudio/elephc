<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\App\Config;
use Showcases\Doom\App\RenderMode;
use Showcases\Doom\Bsp\BspWalker;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Player\Camera;
use Showcases\Doom\SDL\SDL;

class Renderer {
    public $minimap;
    public $bspWalker;
    public $walls;
    public $lastTicks;
    public $fps;
    public $frameCount;

    public function __construct() {
        $this->minimap = new MinimapRenderer();
        $this->bspWalker = new BspWalker();
        $this->walls = new WallRenderer();
        $this->lastTicks = 0;
        $this->fps = 0;
        $this->frameCount = 0;
    }

    public function render(SDL $sdl, Config $config, MapData $map, Camera $camera, int $ticks): void {
        $order = $this->bspWalker->walk($map, $camera);
        $cameraSubSector = $this->bspWalker->findSubSectorIndex($map, $camera);
        $this->walls->render($sdl, $config, $map, $camera, $order, $cameraSubSector, $ticks);
        if ($config->renderMode !== RenderMode::World3D) {
            $this->minimap->renderInset($sdl, $config, $map, $camera, $order);
        }
        $this->renderThings3D($sdl, $config, $map, $camera);
        $this->renderCrosshair($sdl, $config);
        $this->renderHUD($sdl, $config, $camera, $ticks);
    }

    public function renderCrosshair(SDL $sdl, Config $config): void {
        $centerX = intdiv($config->windowWidth, 2);
        $centerY = intdiv($config->windowHeight, 2);
        $sdl->setDrawColor(255, 214, 102);
        $sdl->drawLine($centerX - 6, $centerY, $centerX - 2, $centerY);
        $sdl->drawLine($centerX + 2, $centerY, $centerX + 6, $centerY);
        $sdl->drawLine($centerX, $centerY - 6, $centerX, $centerY - 2);
        $sdl->drawLine($centerX, $centerY + 2, $centerX, $centerY + 6);
    }

    public function renderHUD(SDL $sdl, Config $config, Camera $camera, int $ticks): void {
        int $w = $config->windowWidth;
        int $h = $config->windowHeight;
        int $barHeight = 32;
        int $barTop = $h - $barHeight;

        // FPS tracking
        $this->frameCount = $this->frameCount + 1;
        int $elapsed = $ticks - $this->lastTicks;
        if ($elapsed >= 1000) {
            $this->fps = $this->frameCount;
            $this->frameCount = 0;
            $this->lastTicks = $ticks;
        }

        // dark background bar
        $sdl->setDrawColor(20, 18, 16);
        int $by = $barTop;
        while ($by < $h) {
            $sdl->drawLine(0, $by, $w - 1, $by);
            $by += 1;
        }

        // separator line
        $sdl->setDrawColor(80, 70, 60);
        $sdl->drawLine(0, $barTop, $w - 1, $barTop);

        // compass: angle indicator as a colored bar
        int $compassX = 8;
        int $compassW = 72;
        int $compassY = $barTop + 8;
        int $compassH = 16;

        // compass background
        $sdl->setDrawColor(40, 36, 32);
        int $cy = $compassY;
        while ($cy < $compassY + $compassH) {
            $sdl->drawLine($compassX, $cy, $compassX + $compassW, $cy);
            $cy += 1;
        }

        // compass needle: position based on angle
        int $needleX = $compassX + intdiv($camera->angle * $compassW, 360);
        $sdl->setDrawColor(255, 214, 102);
        $sdl->drawLine($needleX, $compassY, $needleX, $compassY + $compassH - 1);
        $sdl->drawLine($needleX + 1, $compassY, $needleX + 1, $compassY + $compassH - 1);

        // N/S/E/W markers
        $sdl->setDrawColor(120, 110, 100);
        int $nPos = $compassX + intdiv(90 * $compassW, 360);
        int $ePos = $compassX + intdiv(0 * $compassW, 360);
        int $sPos = $compassX + intdiv(270 * $compassW, 360);
        int $wPos = $compassX + intdiv(180 * $compassW, 360);
        $sdl->drawLine($nPos, $compassY, $nPos, $compassY + 3);
        $sdl->drawLine($sPos, $compassY + $compassH - 4, $sPos, $compassY + $compassH - 1);
        $sdl->drawLine($ePos, $compassY + 4, $ePos, $compassY + $compassH - 5);
        $sdl->drawLine($wPos, $compassY + 4, $wPos, $compassY + $compassH - 5);

        // height indicator: vertical bar showing Z
        int $heightX = $compassX + $compassW + 16;
        int $heightW = 8;
        int $zNorm = $camera->z + 128;
        if ($zNorm < 0) {
            $zNorm = 0;
        }
        if ($zNorm > 255) {
            $zNorm = 255;
        }
        int $fillH = intdiv($zNorm * $compassH, 255);
        $sdl->setDrawColor(40, 36, 32);
        $cy = $compassY;
        while ($cy < $compassY + $compassH) {
            $sdl->drawLine($heightX, $cy, $heightX + $heightW, $cy);
            $cy += 1;
        }
        $sdl->setDrawColor(102, 180, 140);
        $cy = $compassY + $compassH - $fillH;
        while ($cy < $compassY + $compassH) {
            $sdl->drawLine($heightX, $cy, $heightX + $heightW, $cy);
            $cy += 1;
        }

        // FPS number as vertical bar segments (digit display)
        int $fpsX = $w - 48;
        int $fpsY = $barTop + 8;
        int $fpsR = 220;
        int $fpsG = 60;
        if ($this->fps >= 30) {
            $fpsR = 60;
            $fpsG = 220;
        } else if ($this->fps >= 15) {
            $fpsR = 220;
            $fpsG = 180;
        }
        $sdl->setDrawColor($fpsR, $fpsG, 40);
        $this->drawNumber($sdl, $this->fps, $fpsX, $fpsY);
    }

    public function renderThings3D(SDL $sdl, Config $config, MapData $map, Camera $camera): void {
        if ($map->thingCount <= 0) {
            return;
        }

        int $centerX = intdiv($config->windowWidth, 2);
        int $horizonY = intdiv($config->windowHeight, 2);
        int $focal = intdiv($config->windowWidth * 3, 4);
        int $eyeZ = $camera->z + 41;
        int $maxY = $config->windowHeight - 33;

        int $forwardX = $this->walls->direction->unitX($camera->angle);
        int $forwardY = $this->walls->direction->unitY($camera->angle);
        int $rightX = -$forwardY;
        int $rightY = $forwardX;

        int $ti = 0;
        while ($ti < $map->thingCount) {
            int $thingType = $map->things[$ti]->type;
            // skip player starts
            if ($thingType >= 1 && $thingType <= 4) {
                $ti = $ti + 1;
                continue;
            }

            int $relX = $map->things[$ti]->x - $camera->x;
            int $relY = $map->things[$ti]->y - $camera->y;
            int $depth = intdiv(($relX * $forwardX) + ($relY * $forwardY), 1024);

            // behind camera or too far
            if ($depth < 16 || $depth > 2000) {
                $ti = $ti + 1;
                continue;
            }

            int $side = intdiv(($relX * $rightX) + ($relY * $rightY), 1024);
            int $screenX = $centerX + intdiv($side * $focal, $depth);

            if ($screenX < 2 || $screenX >= $config->windowWidth - 2) {
                $ti = $ti + 1;
                continue;
            }

            // thing height: 56 units for monsters, 32 for items, 48 for barrels
            int $thingHeight = 32;
            if ($thingType >= 3001 && $thingType <= 3006) {
                $thingHeight = 56;
            } else if ($thingType === 2035) {
                $thingHeight = 48;
            }

            int $thingFloor = $camera->z;
            int $top = $horizonY - intdiv(($thingFloor + $thingHeight - $eyeZ) * $focal, $depth);
            int $bottom = $horizonY - intdiv(($thingFloor - $eyeZ) * $focal, $depth);
            if ($top < 0) {
                $top = 0;
            }
            if ($bottom > $maxY) {
                $bottom = $maxY;
            }
            if ($bottom <= $top) {
                $ti = $ti + 1;
                continue;
            }

            // color by thing type
            int $r = 200;
            int $g = 180;
            int $b = 60;
            if ($thingType === 2035) {
                $r = 160; $g = 60; $b = 40;
            } else if ($thingType >= 3001 && $thingType <= 3006) {
                $r = 140; $g = 50; $b = 50;
            } else if ($thingType >= 2001 && $thingType <= 2006) {
                $r = 60; $g = 160; $b = 80;
            } else if ($thingType === 46 || $thingType === 70 || $thingType === 2028) {
                $r = 220; $g = 200; $b = 100;
            }

            // distance fog
            int $dSq = $depth * $depth;
            int $fog = 255 - intdiv($dSq * 220, $dSq + 640000);
            if ($fog < 35) {
                $fog = 35;
            }
            $r = intdiv($r * $fog, 255);
            $g = intdiv($g * $fog, 255);
            $b = intdiv($b * $fog, 255);

            // draw as a narrow column (billboard)
            int $halfWidth = intdiv(16 * $focal, $depth);
            if ($halfWidth < 1) {
                $halfWidth = 1;
            }
            if ($halfWidth > 20) {
                $halfWidth = 20;
            }
            $sdl->setDrawColor($r, $g, $b);
            int $col = $screenX - $halfWidth;
            while ($col <= $screenX + $halfWidth) {
                if ($col >= 0 && $col < $config->windowWidth) {
                    $sdl->drawLine($col, $top, $col, $bottom);
                }
                $col = $col + 1;
            }

            $ti = $ti + 1;
        }
    }

    public function drawNumber(SDL $sdl, int $value, int $x, int $y): void {
        if ($value <= 0) {
            $this->drawDigit($sdl, 0, $x, $y);
            return;
        }
        $digits = [];
        int $digitCount = 0;
        int $v = $value;
        while ($v > 0) {
            $digits[] = $v % 10;
            $v = intdiv($v, 10);
            $digitCount = $digitCount + 1;
        }
        int $dx = $x;
        int $di = $digitCount - 1;
        while ($di >= 0) {
            $this->drawDigit($sdl, $digits[$di], $dx, $y);
            $dx = $dx + 6;
            $di = $di - 1;
        }
    }

    public function drawDigit(SDL $sdl, int $digit, int $x, int $y): void {
        bool $top = $digit !== 1 && $digit !== 4;
        bool $mid = $digit !== 0 && $digit !== 1 && $digit !== 7;
        bool $bot = $digit !== 1 && $digit !== 4 && $digit !== 7;
        bool $tl = $digit === 0 || $digit === 4 || $digit === 5 || $digit === 6 || $digit === 8 || $digit === 9;
        bool $tr = $digit !== 5 && $digit !== 6;
        bool $bl = $digit === 0 || $digit === 2 || $digit === 6 || $digit === 8;
        bool $br = $digit !== 2;

        if ($top) {
            $sdl->drawLine($x, $y, $x + 3, $y);
        }
        if ($mid) {
            $sdl->drawLine($x, $y + 3, $x + 3, $y + 3);
        }
        if ($bot) {
            $sdl->drawLine($x, $y + 6, $x + 3, $y + 6);
        }
        if ($tl) {
            $sdl->drawLine($x, $y, $x, $y + 3);
        }
        if ($tr) {
            $sdl->drawLine($x + 3, $y, $x + 3, $y + 3);
        }
        if ($bl) {
            $sdl->drawLine($x, $y + 3, $x, $y + 6);
        }
        if ($br) {
            $sdl->drawLine($x + 3, $y + 3, $x + 3, $y + 6);
        }
    }
}
