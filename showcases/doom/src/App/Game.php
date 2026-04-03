<?php

namespace Showcases\Doom\App;

use Showcases\Doom\Player\Camera;
use Showcases\Doom\Render\Renderer;
use Showcases\Doom\SDL\Input;
use Showcases\Doom\SDL\SDL;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Map\MapLoader;
use Showcases\Doom\Support\Direction;
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
    public $mapToggleHeld;
    public $direction;

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
        $this->mapToggleHeld = 0;
        $this->direction = new Direction();
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

            $this->handleMapToggle($keys);

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
                $this->renderer->render($this->sdl, $this->config, $this->map, $this->camera, $this->sdl->ticks());
            }
            $this->sdl->present();
            $this->sdl->delay($this->config->targetFrameMs);
        }

        $this->sdl->shutdown();
    }

    public function bootWad(): void {
        $wadPath = $this->resolveWadPath();
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

    public function handleMapToggle(ptr $keys): void {
        $pressed = $this->input->toggleMap($keys);
        if ($pressed !== 0) {
            if ($this->mapToggleHeld === 0) {
                if ($this->config->renderMode === RenderMode::Split) {
                    $this->config->renderMode = RenderMode::World3D;
                } else {
                    $this->config->renderMode = RenderMode::Split;
                }
            }
            $this->mapToggleHeld = 1;
            return;
        }

        $this->mapToggleHeld = 0;
    }

    public function updateCamera(ptr $keys): void {
        $moveStep = 24;
        $turnStep = 5;
        $moveDx = 0;
        $moveDy = 0;

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
        $targetX = $this->camera->x + $dx;
        $targetY = $this->camera->y + $dy;

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
            bool $blocking = false;
            if ($this->isSolidLinedef($i)) {
                if ($this->isBlockingSideOfSolidLinedef($i, $x, $y)) {
                    $blocking = true;
                }
            } else if ($this->isBlockingTwoSidedLinedef($i)) {
                $blocking = true;
            }

            if ($blocking) {
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
        if ($this->map->linedefs[$index]->left_sidedef < 0
            || $this->map->linedefs[$index]->right_sidedef < 0) {
            return true;
        }
        return false;
    }

    public function isBlockingTwoSidedLinedef(int $index): bool {
        int $rightSide = $this->map->linedefs[$index]->right_sidedef;
        int $leftSide = $this->map->linedefs[$index]->left_sidedef;
        if ($rightSide < 0 || $leftSide < 0) {
            return false;
        }
        if ($rightSide >= $this->map->sidedefCount || $leftSide >= $this->map->sidedefCount) {
            return false;
        }

        int $frontIdx = $this->map->sidedefs[$rightSide]->sector_index;
        int $backIdx = $this->map->sidedefs[$leftSide]->sector_index;
        if ($frontIdx < 0 || $backIdx < 0 || $frontIdx >= $this->map->sectorCount || $backIdx >= $this->map->sectorCount) {
            return false;
        }

        int $frontFloor = $this->map->sectors[$frontIdx]->floor_height;
        int $backFloor = $this->map->sectors[$backIdx]->floor_height;
        int $frontCeiling = $this->map->sectors[$frontIdx]->ceiling_height;
        int $backCeiling = $this->map->sectors[$backIdx]->ceiling_height;

        // closed door: ceiling <= floor on either side
        if ($backCeiling <= $backFloor) {
            return true;
        }
        if ($frontCeiling <= $frontFloor) {
            return true;
        }

        // step too high (> 24 units)
        int $stepUp = $backFloor - $frontFloor;
        if ($stepUp < 0) {
            $stepUp = -$stepUp;
        }
        if ($stepUp > 24) {
            return true;
        }

        // ceiling too low to pass (< 56 units clearance)
        int $lowestCeiling = $frontCeiling;
        if ($backCeiling < $lowestCeiling) {
            $lowestCeiling = $backCeiling;
        }
        int $highestFloor = $frontFloor;
        if ($backFloor > $highestFloor) {
            $highestFloor = $backFloor;
        }
        if ($lowestCeiling - $highestFloor < 56) {
            return true;
        }

        return false;
    }

    public function isBlockingSideOfSolidLinedef(int $index, int $x, int $y): bool {
        int $startIndex = $this->map->linedefs[$index]->start_vertex;
        int $endIndex = $this->map->linedefs[$index]->end_vertex;
        if (
            $startIndex < 0
            || $endIndex < 0
            || $startIndex >= $this->map->vertexCount
            || $endIndex >= $this->map->vertexCount
        ) {
            return false;
        }

        int $ax = $this->map->vertexes[$startIndex]->x;
        int $ay = $this->map->vertexes[$startIndex]->y;
        int $bx = $this->map->vertexes[$endIndex]->x;
        int $by = $this->map->vertexes[$endIndex]->y;
        int $edgeX = $bx - $ax;
        int $edgeY = $by - $ay;
        int $toPointX = $x - $ax;
        int $toPointY = $y - $ay;
        int $cross = ($edgeX * $toPointY) - ($edgeY * $toPointX);

        return $cross < 0;
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
        return intdiv($this->direction->unitX($this->camera->angle) * $step, 1024);
    }

    public function forwardStepY(int $step): int {
        return intdiv($this->direction->unitY($this->camera->angle) * $step, 1024);
    }

    public function strafeLeftStepX(int $step): int {
        return intdiv($this->direction->unitY($this->camera->angle) * $step, 1024);
    }

    public function strafeLeftStepY(int $step): int {
        return intdiv((-1 * $this->direction->unitX($this->camera->angle)) * $step, 1024);
    }
}
