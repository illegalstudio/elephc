<?php

namespace Showcases\Doom\App;

class Application {
    public $config;
    public $game;

    public function __construct(Config $config) {
        $this->config = $config;
        $this->game = new Game($config);
    }

    public function run() {
        $this->game->run();
    }
}
