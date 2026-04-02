<?php

namespace Showcases\Doom\App;

use Showcases\Doom\Player\Camera;
use Showcases\Doom\SDL\Input;
use Showcases\Doom\SDL\SDL;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Map\MapLoader;
use Showcases\Doom\Wad\WadFile;
use Showcases\Doom\Wad\WadLoader;

class Game {
    public $config;
    public $sdl;
    public $input;
    public $camera;
    public $state;
    public $wadLoader;
    public $mapLoader;
    public $wad;
    public $map;

    public function __construct(Config $config) {
        $this->config = $config;
        $this->sdl = new SDL();
        $this->input = new Input();
        $this->camera = new Camera();
        $this->state = GameState::Booting;
        $this->wadLoader = new WadLoader();
        $this->mapLoader = new MapLoader();
        $this->wad = new WadFile("", "", 0, 0);
        $this->map = new MapData("", -1);
    }

    public function run() {
        $this->bootWad();

        if (!$this->sdl->boot($this->config)) {
            echo "SDL boot failed: " . $this->sdl->lastError() . "\n";
            return;
        }

        $this->state = GameState::Running;
        echo "DOOM showcase SDL shell running\n";
        echo "ESC quits early\n";

        $start = $this->sdl->ticks();
        while ($this->state === GameState::Running) {
            $this->sdl->pumpEvents();
            $keys = $this->sdl->keyboardState();
            if ($this->input->shouldQuit($keys)) {
                $this->state = GameState::Stopped;
            }

            if ($this->sdl->ticks() - $start >= $this->config->bootDurationMs) {
                $this->state = GameState::Stopped;
            }

            $this->sdl->clear(
                $this->config->backgroundR,
                $this->config->backgroundG,
                $this->config->backgroundB
            );
            $this->sdl->present();
            $this->sdl->delay($this->config->targetFrameMs);
        }

        $this->sdl->shutdown();
    }

    public function bootWad(): void {
        string $wadPath = $this->resolveWadPath();
        if ($wadPath === "") {
            echo "No WAD file at " . $this->config->wadPath . "\n";
            return;
        }

        $this->wad = $this->wadLoader->load($wadPath);
        if (!$this->wad->isValid()) {
            echo "Failed to load WAD: " . $wadPath . "\n";
            return;
        }

        echo "Loaded WAD: ";
        echo $this->wad->kind;
        echo " | lumps: ";
        echo $this->wad->entryCount;
        echo " | directory: ";
        echo $this->wad->directoryOffset;
        echo "\n";

        if ($this->wad->firstEntryName !== "") {
            echo "First lump: ";
            echo $this->wad->firstEntryName;
            echo " @ ";
            echo $this->wad->firstEntryOffset;
            echo " (";
            echo $this->wad->firstEntrySize;
            echo " bytes)\n";
        }

        $this->map = $this->mapLoader->load($this->wad, $this->config->startupMap);
        if ($this->map->isValid()) {
            echo $this->map->summary() . "\n";
        } else {
            echo "Map " . $this->config->startupMap . " not found or incomplete\n";
        }
    }

    public function resolveWadPath(): string {
        if (file_exists($this->config->wadPath)) {
            return $this->config->wadPath;
        }

        $repoRelative = "showcases/doom/" . $this->config->wadPath;
        if (file_exists($repoRelative)) {
            return $repoRelative;
        }

        return "";
    }
}
