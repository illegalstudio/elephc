<?php

namespace Showcases\Doom\App;

final class Application {
    public Config $config;
    public Game $game;

    public function __construct(Config $config) {
        $this->config = $config;
        $this->game = new Game($config);
    }

    public function run(): void {
        $this->game->run();
    }
}
