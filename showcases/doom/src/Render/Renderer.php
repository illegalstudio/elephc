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

    public function __construct() {
        $this->minimap = new MinimapRenderer();
        $this->bspWalker = new BspWalker();
    }

    public function render(SDL $sdl, Config $config, MapData $map, Camera $camera): void {
        $order = $this->bspWalker->walk($map, $camera);
        $this->minimap->render($sdl, $config, $map, $camera, $order);
    }
}
