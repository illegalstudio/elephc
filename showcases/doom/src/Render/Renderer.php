<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\App\Config;
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

    public function render(SDL $sdl, Config $config, MapData $map, Camera $camera): void {
        $order = $this->bspWalker->walk($map, $camera);
        $this->walls->render($sdl, $config, $map, $camera, $order);
        $this->minimap->renderInset($sdl, $config, $map, $camera, $order);
    }
}
