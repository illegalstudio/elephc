<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\App\Config;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Player\Camera;
use Showcases\Doom\SDL\SDL;

class Renderer {
    public $minimap;

    public function __construct() {
        $this->minimap = new MinimapRenderer();
    }

    public function render(SDL $sdl, Config $config, MapData $map, Camera $camera): void {
        $this->minimap->render($sdl, $config, $map, $camera);
    }
}
