<?php

namespace Showcases\Doom\App;

use Showcases\Doom\Player\Camera;
use Showcases\Doom\Render\Renderer;
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
    public $renderer;
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
        $this->renderer = new Renderer();
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

            if ($this->map->isValid()) {
                $this->updateCamera($keys);
            }

            if (
                $this->config->bootDurationMs > 0
                && $this->sdl->ticks() - $start >= $this->config->bootDurationMs
            ) {
                $this->state = GameState::Stopped;
            }

            $this->sdl->clear(
                $this->config->backgroundR,
                $this->config->backgroundG,
                $this->config->backgroundB
            );
            if ($this->map->isValid()) {
                $this->renderer->render($this->sdl, $this->config, $this->map, $this->camera);
            }
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
            if ($this->map->hasPlayerStart()) {
                $this->camera->setSpawn(
                    $this->map->playerStartX,
                    $this->map->playerStartY,
                    $this->map->playerStartAngle
                );
            }
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

    public function updateCamera(ptr $keys): void {
        int $moveStep = 24;
        int $turnStep = 4;

        if ($this->input->moveForward($keys)) {
            $this->camera->moveBy(0, -$moveStep);
        }
        if ($this->input->moveBackward($keys)) {
            $this->camera->moveBy(0, $moveStep);
        }
        if ($this->input->moveLeft($keys)) {
            $this->camera->moveBy(-$moveStep, 0);
        }
        if ($this->input->moveRight($keys)) {
            $this->camera->moveBy($moveStep, 0);
        }
        if ($this->input->turnLeft($keys)) {
            $this->camera->rotateBy(-$turnStep);
        }
        if ($this->input->turnRight($keys)) {
            $this->camera->rotateBy($turnStep);
        }
    }
}
