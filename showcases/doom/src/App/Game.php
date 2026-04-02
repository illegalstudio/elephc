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
        int $moveDx = 0;
        int $moveDy = 0;

        if ($this->input->moveForward($keys)) {
            $moveDx = $moveDx + $this->forwardStepX($moveStep);
            $moveDy = $moveDy + $this->forwardStepY($moveStep);
        }
        if ($this->input->moveBackward($keys)) {
            $moveDx = $moveDx - $this->forwardStepX($moveStep);
            $moveDy = $moveDy - $this->forwardStepY($moveStep);
        }
        if ($this->input->moveLeft($keys)) {
            $moveDx = $moveDx + $this->strafeLeftStepX($moveStep);
            $moveDy = $moveDy + $this->strafeLeftStepY($moveStep);
        }
        if ($this->input->moveRight($keys)) {
            $moveDx = $moveDx - $this->strafeLeftStepX($moveStep);
            $moveDy = $moveDy - $this->strafeLeftStepY($moveStep);
        }
        if ($this->input->turnLeft($keys)) {
            $this->camera->rotateBy(-$turnStep);
        }
        if ($this->input->turnRight($keys)) {
            $this->camera->rotateBy($turnStep);
        }

        if ($moveDx !== 0 || $moveDy !== 0) {
            $this->tryMoveCamera($moveDx, $moveDy);
        }
    }

    public function tryMoveCamera(int $dx, int $dy): void {
        int $targetX = $this->camera->x + $dx;
        int $targetY = $this->camera->y + $dy;

        if ($this->isWalkablePosition($targetX, $targetY)) {
            $this->camera->setSpawn($targetX, $targetY, $this->camera->angle);
            return;
        }

        if ($dx !== 0 && $this->isWalkablePosition($this->camera->x + $dx, $this->camera->y)) {
            $this->camera->moveBy($dx, 0);
        }
        if ($dy !== 0 && $this->isWalkablePosition($this->camera->x, $this->camera->y + $dy)) {
            $this->camera->moveBy(0, $dy);
        }
    }

    public function isWalkablePosition(int $x, int $y): bool {
        int $margin = 24;
        if ($x < $this->map->minX - $margin || $x > $this->map->maxX + $margin) {
            return false;
        }
        if ($y < $this->map->minY - $margin || $y > $this->map->maxY + $margin) {
            return false;
        }

        int $i = 0;
        int $radiusSquared = 28 * 28;
        while ($i < $this->map->linedefCount) {
            if ($this->isSolidLinedef($i)) {
                int $startIndex = $this->map->linedefs[$i]->start_vertex;
                int $endIndex = $this->map->linedefs[$i]->end_vertex;
                if (
                    $startIndex >= 0
                    && $endIndex >= 0
                    && $startIndex < $this->map->vertexCount
                    && $endIndex < $this->map->vertexCount
                ) {
                    int $distanceSquared = $this->distanceToSegmentSquared(
                        $x,
                        $y,
                        $this->map->vertexes[$startIndex]->x,
                        $this->map->vertexes[$startIndex]->y,
                        $this->map->vertexes[$endIndex]->x,
                        $this->map->vertexes[$endIndex]->y
                    );
                    if ($distanceSquared < $radiusSquared) {
                        return false;
                    }
                }
            }
            $i += 1;
        }

        return true;
    }

    public function isSolidLinedef(int $index): bool {
        return $this->map->linedefs[$index]->left_sidedef < 0
            || $this->map->linedefs[$index]->right_sidedef < 0;
    }

    public function distanceToSegmentSquared(
        int $px,
        int $py,
        int $ax,
        int $ay,
        int $bx,
        int $by
    ): int {
        int $dx = $bx - $ax;
        int $dy = $by - $ay;
        int $lengthSquared = ($dx * $dx) + ($dy * $dy);

        if ($lengthSquared <= 0) {
            return (($px - $ax) * ($px - $ax)) + (($py - $ay) * ($py - $ay));
        }

        int $numerator = (($px - $ax) * $dx) + (($py - $ay) * $dy);
        int $closestX = $ax;
        int $closestY = $ay;

        if ($numerator >= $lengthSquared) {
            $closestX = $bx;
            $closestY = $by;
        } else if ($numerator > 0) {
            $closestX = $ax + intdiv($dx * $numerator, $lengthSquared);
            $closestY = $ay + intdiv($dy * $numerator, $lengthSquared);
        }

        return (($px - $closestX) * ($px - $closestX))
            + (($py - $closestY) * ($py - $closestY));
    }

    public function forwardStepX(int $step): int {
        return intdiv($this->directionUnitX() * $step, 1024);
    }

    public function forwardStepY(int $step): int {
        return intdiv($this->directionUnitY() * $step, 1024);
    }

    public function strafeLeftStepX(int $step): int {
        return intdiv($this->directionUnitY() * $step, 1024);
    }

    public function strafeLeftStepY(int $step): int {
        return intdiv((-1 * $this->directionUnitX()) * $step, 1024);
    }

    public function directionBucket16(): int {
        int $angle = $this->camera->angle + 11;
        if ($angle >= 360) {
            $angle = $angle - 360;
        }

        return intdiv($angle * 16, 360);
    }

    public function directionUnitX(): int {
        int $bucket = $this->directionBucket16();

        if ($bucket === 1) {
            return 392;
        }
        if ($bucket === 2) {
            return 724;
        }
        if ($bucket === 3) {
            return 946;
        }
        if ($bucket === 4) {
            return 1024;
        }
        if ($bucket === 5) {
            return 946;
        }
        if ($bucket === 6) {
            return 724;
        }
        if ($bucket === 7) {
            return 392;
        }
        if ($bucket === 9) {
            return -392;
        }
        if ($bucket === 10) {
            return -724;
        }
        if ($bucket === 11) {
            return -946;
        }
        if ($bucket === 12) {
            return -1024;
        }
        if ($bucket === 13) {
            return -946;
        }
        if ($bucket === 14) {
            return -724;
        }
        if ($bucket === 15) {
            return -392;
        }

        return 0;
    }

    public function directionUnitY(): int {
        int $bucket = $this->directionBucket16();

        if ($bucket === 0) {
            return -1024;
        }
        if ($bucket === 1) {
            return -946;
        }
        if ($bucket === 2) {
            return -724;
        }
        if ($bucket === 3) {
            return -392;
        }
        if ($bucket === 5) {
            return 392;
        }
        if ($bucket === 6) {
            return 724;
        }
        if ($bucket === 7) {
            return 946;
        }
        if ($bucket === 8) {
            return 1024;
        }
        if ($bucket === 9) {
            return 946;
        }
        if ($bucket === 10) {
            return 724;
        }
        if ($bucket === 11) {
            return 392;
        }
        if ($bucket === 13) {
            return -392;
        }
        if ($bucket === 14) {
            return -724;
        }
        if ($bucket === 15) {
            return -946;
        }

        return 0;
    }
}
