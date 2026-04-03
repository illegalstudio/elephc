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

    public function __construct() {
        $this->minimap = new MinimapRenderer();
        $this->bspWalker = new BspWalker();
        $this->walls = new WallRenderer();
    }

    public function render(SDL $sdl, Config $config, MapData $map, Camera $camera, int $ticks): void {
        $order = $this->bspWalker->walk($map, $camera);
        $cameraSubSector = $this->bspWalker->findSubSectorIndex($map, $camera);
        $this->walls->render($sdl, $config, $map, $camera, $order, $cameraSubSector, $ticks);
        if ($config->renderMode !== RenderMode::World3D) {
            $this->minimap->renderInset($sdl, $config, $map, $camera, $order);
        }
        $this->renderCrosshair($sdl, $config);
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
}
