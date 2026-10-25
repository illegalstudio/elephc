<?php

namespace Showcases\Doom\Play;

use Showcases\Doom\Data\Button;
use Showcases\Doom\Data\LightEffect;
use Showcases\Doom\Data\Linedef;
use Showcases\Doom\Data\Mobj;
use Showcases\Doom\Data\Mover;
use Showcases\Doom\Data\Sector;
use Showcases\Doom\Data\Sidedef;
use Showcases\Doom\Data\Thing;
use Showcases\Doom\Graphics\Textures;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Support\Trig;
use Showcases\Doom\Wad\Wad;

// Line and sector specials of the shareware episode, following the public DOOM
// linedef and sector type tables: doors, lifts, moving floors, stairs, switches,
// teleporters, exits, scrolling walls and light effects. Movers change sector
// heights a few units per tic; the renderer and collision read them live.
final class Specials {
    // door types
    public int $doorNormal = 0;
    public int $doorOpen = 1;
    public int $doorClose = 2;
    public int $doorCloseThenOpen = 3;
    // mover kinds
    public int $kindDoor = 1;
    public int $kindLift = 2;
    public int $kindFloor = 3;

    public MapData $map;
    public Collision $collision;
    public Things $things;
    public Trig $trig;
    public buffer<Mover> $movers;
    public int $moverCount;
    public buffer<LightEffect> $lights;
    public int $lightCount;
    public buffer<Button> $buttons;
    public buffer<int> $scrollers;
    public int $scrollerCount;
    public buffer<int> $switchPair;
    public buffer<int> $sectorLineStart;
    public buffer<int> $sectorLineCount;
    public buffer<int> $sectorLines;
    public int $seed;
    public int $exitRequest;
    // the player, mirrored by the game each tic
    public int $playerX;
    public int $playerY;
    public int $playerKeys;
    public bool $teleportPending;
    public int $teleportX;
    public int $teleportY;
    public int $teleportAngle;

    public function __construct(MapData $map, Collision $collision, Things $things, Textures $textures, Wad $wad, Trig $trig) {
        $this->map = $map;
        $this->collision = $collision;
        $this->things = $things;
        $this->trig = $trig;
        $this->movers = buffer_new<Mover>(256);
        $this->moverCount = 0;
        $this->lights = buffer_new<LightEffect>(256);
        $this->lightCount = 0;
        $this->buttons = buffer_new<Button>(16);
        $this->scrollers = buffer_new<int>(64);
        $this->scrollerCount = 0;
        $this->seed = 12345;
        $this->exitRequest = 0;
        $this->playerX = 0;
        $this->playerY = 0;
        $this->playerKeys = 0;
        $this->teleportPending = false;
        $this->teleportX = 0;
        $this->teleportY = 0;
        $this->teleportAngle = 0;
        $this->switchPair = buffer_new<int>($textures->count + 1);
        $this->buildSwitchPairs($textures, $wad);
        $this->sectorLineStart = buffer_new<int>($map->sectorCount + 1);
        $this->sectorLineCount = buffer_new<int>($map->sectorCount + 1);
        $this->sectorLines = buffer_new<int>($map->linedefCount * 2 + 1);
        $this->buildSectorLines();
        $this->spawnSectorSpecials();
        $this->spawnScrollers();
    }

    // Buffers are not reclaimed on their own: the game calls this when it leaves the map.
    public function release(): void {
        buffer<Mover> $movers = $this->movers;
        buffer_free($movers);
        buffer<LightEffect> $lights = $this->lights;
        buffer_free($lights);
        buffer<Button> $buttons = $this->buttons;
        buffer_free($buttons);
        buffer<int> $scrollers = $this->scrollers;
        buffer_free($scrollers);
        buffer<int> $switchPair = $this->switchPair;
        buffer_free($switchPair);
        buffer<int> $lineStart = $this->sectorLineStart;
        buffer_free($lineStart);
        buffer<int> $lineCount = $this->sectorLineCount;
        buffer_free($lineCount);
        buffer<int> $lines = $this->sectorLines;
        buffer_free($lines);
    }

    // SW1xxxx and SW2xxxx textures are the two faces of a switch.
    public function buildSwitchPairs(Textures $textures, Wad $wad): void {
        int $sw1 = $wad->key("SW1");
        int $sw2 = $wad->key("SW2");
        for ($t = 0; $t < $textures->count; $t++) {
            $this->switchPair[$t] = -1;
        }
        for ($t = 0; $t < $textures->count; $t++) {
            int $key = $textures->key[$t];
            int $prefix = $key & 16777215;
            if ($prefix !== $sw1) {
                continue;
            }
            int $suffix = $key - $prefix;
            int $partnerKey = $suffix | $sw2;
            int $partner = $textures->find($partnerKey);
            if ($partner >= 0) {
                $this->switchPair[$t] = $partner;
                $this->switchPair[$partner] = $t;
            }
        }
    }

    public function buildSectorLines(): void {
        buffer<Linedef> $lines = $this->map->linedefs;
        int $sectors = $this->map->sectorCount;
        for ($s = 0; $s < $sectors; $s++) {
            $this->sectorLineCount[$s] = 0;
        }
        for ($l = 0; $l < $this->map->linedefCount; $l++) {
            int $front = $lines[$l]->front_sector;
            int $back = $lines[$l]->back_sector;
            if ($front >= 0) {
                $this->sectorLineCount[$front]++;
            }
            if ($back >= 0 && $back !== $front) {
                $this->sectorLineCount[$back]++;
            }
        }
        int $total = 0;
        for ($s = 0; $s < $sectors; $s++) {
            $this->sectorLineStart[$s] = $total;
            int $grown = $total + $this->sectorLineCount[$s];
            $total = $grown;
            $this->sectorLineCount[$s] = 0;
        }
        for ($l = 0; $l < $this->map->linedefCount; $l++) {
            int $front = $lines[$l]->front_sector;
            int $back = $lines[$l]->back_sector;
            if ($front >= 0) {
                $this->addSectorLine($front, $l);
            }
            if ($back >= 0 && $back !== $front) {
                $this->addSectorLine($back, $l);
            }
        }
    }

    public function addSectorLine(int $sector, int $line): void {
        int $at = $this->sectorLineStart[$sector] + $this->sectorLineCount[$sector];
        $this->sectorLines[$at] = $line;
        $this->sectorLineCount[$sector]++;
    }

    // The sector across $line from $sector, or -1.
    public function neighbor(int $line, int $sector): int {
        int $front = $this->map->linedefs[$line]->front_sector;
        int $back = $this->map->linedefs[$line]->back_sector;
        if ($front === $sector) {
            return $back;
        }
        return $front;
    }

    public function lowestCeilingAround(int $sector): int {
        int $lowest = 32767;
        int $start = $this->sectorLineStart[$sector];
        int $end = $start + $this->sectorLineCount[$sector];
        for ($at = $start; $at < $end; $at++) {
            int $other = $this->neighbor($this->sectorLines[$at], $sector);
            if ($other >= 0 && $this->map->sectors[$other]->ceiling_height < $lowest) {
                $lowest = $this->map->sectors[$other]->ceiling_height;
            }
        }
        return $lowest;
    }

    public function highestFloorAround(int $sector): int {
        int $highest = -500;
        int $start = $this->sectorLineStart[$sector];
        int $end = $start + $this->sectorLineCount[$sector];
        for ($at = $start; $at < $end; $at++) {
            int $other = $this->neighbor($this->sectorLines[$at], $sector);
            if ($other >= 0 && $this->map->sectors[$other]->floor_height > $highest) {
                $highest = $this->map->sectors[$other]->floor_height;
            }
        }
        return $highest;
    }

    public function lowestFloorAround(int $sector): int {
        int $lowest = $this->map->sectors[$sector]->floor_height;
        int $start = $this->sectorLineStart[$sector];
        int $end = $start + $this->sectorLineCount[$sector];
        for ($at = $start; $at < $end; $at++) {
            int $other = $this->neighbor($this->sectorLines[$at], $sector);
            if ($other >= 0 && $this->map->sectors[$other]->floor_height < $lowest) {
                $lowest = $this->map->sectors[$other]->floor_height;
            }
        }
        return $lowest;
    }

    public function nextHighestFloor(int $sector): int {
        int $current = $this->map->sectors[$sector]->floor_height;
        int $next = 32767;
        int $start = $this->sectorLineStart[$sector];
        int $end = $start + $this->sectorLineCount[$sector];
        for ($at = $start; $at < $end; $at++) {
            int $other = $this->neighbor($this->sectorLines[$at], $sector);
            if ($other < 0) {
                continue;
            }
            int $height = $this->map->sectors[$other]->floor_height;
            if ($height > $current && $height < $next) {
                $next = $height;
            }
        }
        if ($next === 32767) {
            return $current;
        }
        return $next;
    }

    public function minLightAround(int $sector, int $maxLight): int {
        int $lowest = $maxLight;
        int $start = $this->sectorLineStart[$sector];
        int $end = $start + $this->sectorLineCount[$sector];
        for ($at = $start; $at < $end; $at++) {
            int $other = $this->neighbor($this->sectorLines[$at], $sector);
            if ($other >= 0 && $this->map->sectors[$other]->light_level < $lowest) {
                $lowest = $this->map->sectors[$other]->light_level;
            }
        }
        return $lowest;
    }

    public function random(): int {
        int $scaled = $this->seed * 1103515245;
        int $stepped = $scaled + 12345;
        int $next = $stepped & 2147483647;
        $this->seed = $next;
        int $shifted = $next >> 16;
        return $shifted & 255;
    }

    // ---- light effects (sector specials 1, 2, 3, 4, 8, 12, 13, 17)

    public function spawnSectorSpecials(): void {
        buffer<Sector> $sectors = $this->map->sectors;
        for ($s = 0; $s < $this->map->sectorCount; $s++) {
            int $special = $sectors[$s]->special;
            int $light = $sectors[$s]->light_level;
            if ($special === 1) {
                $this->addLight($s, 1, $this->minLightAround($s, $light), $light, 64, 7);
            } else if ($special === 2 || $special === 4) {
                $this->addStrobe($s, 15, false);
            } else if ($special === 3) {
                $this->addStrobe($s, 35, false);
            } else if ($special === 8) {
                $this->addLight($s, 8, $this->minLightAround($s, $light), $light, 0, 0);
            } else if ($special === 12) {
                $this->addStrobe($s, 35, true);
            } else if ($special === 13) {
                $this->addStrobe($s, 15, true);
            } else if ($special === 17) {
                int $flickerMin = $this->minLightAround($s, $light) + 16;
                $this->addLight($s, 17, $flickerMin, $light, 0, 0);
            }
        }
    }

    public function addLight(int $sector, int $kind, int $minLight, int $maxLight, int $darkTime, int $brightTime): void {
        if ($this->lightCount >= 256) {
            return;
        }
        int $slot = $this->lightCount;
        $this->lights[$slot]->sector = $sector;
        $this->lights[$slot]->kind = $kind;
        $this->lights[$slot]->min_light = $minLight;
        $this->lights[$slot]->max_light = $maxLight;
        $this->lights[$slot]->dark_time = $darkTime;
        $this->lights[$slot]->bright_time = $brightTime;
        $this->lights[$slot]->direction = -1;
        int $first = $this->random() & 63;
        $this->lights[$slot]->count = $first + 1;
        $this->lightCount++;
    }

    public function addStrobe(int $sector, int $darkTime, bool $inSync): void {
        int $maxLight = $this->map->sectors[$sector]->light_level;
        int $minLight = $this->minLightAround($sector, $maxLight);
        if ($minLight === $maxLight) {
            $minLight = 0;
        }
        $this->addLight($sector, 2, $minLight, $maxLight, $darkTime, 5);
        int $slot = $this->lightCount - 1;
        int $count = 1;
        if (!$inSync) {
            int $roll = $this->random() & 7;
            $count = $roll + 1;
        }
        $this->lights[$slot]->count = $count;
    }

    public function tickLights(): void {
        buffer<LightEffect> $lights = $this->lights;
        buffer<Sector> $sectors = $this->map->sectors;
        for ($i = 0; $i < $this->lightCount; $i++) {
            int $sector = $lights[$i]->sector;
            int $kind = $lights[$i]->kind;
            int $light = $sectors[$sector]->light_level;
            int $minLight = $lights[$i]->min_light;
            int $maxLight = $lights[$i]->max_light;
            if ($kind === 8) {
                // glow between the darkest neighbor and the sector's own level, 8 per tic
                if ($lights[$i]->direction < 0) {
                    int $dimmer = $light - 8;
                    if ($dimmer <= $minLight) {
                        $sectors[$sector]->light_level = $minLight;
                        $lights[$i]->direction = 1;
                    } else {
                        $sectors[$sector]->light_level = $dimmer;
                    }
                } else {
                    int $brighter = $light + 8;
                    if ($brighter >= $maxLight) {
                        $sectors[$sector]->light_level = $maxLight;
                        $lights[$i]->direction = -1;
                    } else {
                        $sectors[$sector]->light_level = $brighter;
                    }
                }
                continue;
            }
            $lights[$i]->count--;
            if ($lights[$i]->count > 0) {
                continue;
            }
            if ($kind === 17) {
                int $roll = $this->random() & 3;
                int $amount = $roll * 16;
                int $flicker = $maxLight - $amount;
                if ($flicker < $minLight) {
                    $flicker = $minLight;
                }
                $sectors[$sector]->light_level = $flicker;
                $lights[$i]->count = 4;
            } else if ($kind === 1) {
                if ($light === $maxLight) {
                    $sectors[$sector]->light_level = $minLight;
                    int $roll = $this->random() & $lights[$i]->bright_time;
                    $lights[$i]->count = $roll + 1;
                } else {
                    $sectors[$sector]->light_level = $maxLight;
                    int $roll = $this->random() & $lights[$i]->dark_time;
                    $lights[$i]->count = $roll + 1;
                }
            } else {
                if ($light === $minLight) {
                    $sectors[$sector]->light_level = $maxLight;
                    $lights[$i]->count = $lights[$i]->bright_time;
                } else {
                    $sectors[$sector]->light_level = $minLight;
                    $lights[$i]->count = $lights[$i]->dark_time;
                }
            }
        }
    }

    // ---- scrolling walls (line special 48)

    public function spawnScrollers(): void {
        buffer<Linedef> $lines = $this->map->linedefs;
        for ($l = 0; $l < $this->map->linedefCount; $l++) {
            if ($lines[$l]->special === 48 && $lines[$l]->right_side >= 0 && $this->scrollerCount < 64) {
                $this->scrollers[$this->scrollerCount] = $lines[$l]->right_side;
                $this->scrollerCount++;
            }
        }
    }

    // ---- movers

    public function newMover(int $sector, int $kind): int {
        int $slot = -1;
        for ($i = 0; $i < $this->moverCount; $i++) {
            if ($this->movers[$i]->active === 0) {
                $slot = $i;
                break;
            }
        }
        if ($slot < 0) {
            if ($this->moverCount >= 256) {
                return -1;
            }
            $slot = $this->moverCount;
            $this->moverCount++;
        }
        $this->movers[$slot]->active = 1;
        $this->movers[$slot]->sector = $sector;
        $this->movers[$slot]->kind = $kind;
        $this->movers[$slot]->type = 0;
        $this->movers[$slot]->direction = 0;
        $this->movers[$slot]->speed = 1;
        $this->movers[$slot]->top = 0;
        $this->movers[$slot]->bottom = 0;
        $this->movers[$slot]->wait = 0;
        $this->movers[$slot]->countdown = 0;
        $this->movers[$slot]->texture = -1;
        int $marker = $slot + 1;
        $this->map->sectors[$sector]->mover = $marker;
        return $slot;
    }

    public function finishMover(int $slot): void {
        int $sector = $this->movers[$slot]->sector;
        $this->movers[$slot]->active = 0;
        $this->map->sectors[$sector]->mover = 0;
    }

    public function startDoor(int $sector, int $type): void {
        int $slot = $this->newMover($sector, $this->kindDoor);
        if ($slot < 0) {
            return;
        }
        int $top = $this->lowestCeilingAround($sector) - 4;
        $this->movers[$slot]->type = $type;
        $this->movers[$slot]->speed = 2;
        $this->movers[$slot]->wait = 150;
        $this->movers[$slot]->top = $top;
        $this->movers[$slot]->direction = 1;
        if ($type === $this->doorClose) {
            $this->movers[$slot]->direction = -1;
        } else if ($type === $this->doorCloseThenOpen) {
            $this->movers[$slot]->top = $this->map->sectors[$sector]->ceiling_height;
            $this->movers[$slot]->direction = -1;
            $this->movers[$slot]->wait = 1050;
        }
    }

    // Doors, lifts and floors on every sector with the line's tag that is not already moving.
    public function tagged(int $line, int $action): bool {
        int $tag = $this->map->linedefs[$line]->tag;
        if ($tag === 0) {
            return false;
        }
        bool $started = false;
        for ($s = 0; $s < $this->map->sectorCount; $s++) {
            if ($this->map->sectors[$s]->tag !== $tag) {
                continue;
            }
            if ($action === 35) {
                $this->map->sectors[$s]->light_level = 35;
                $started = true;
                continue;
            }
            if ($this->map->sectors[$s]->mover !== 0) {
                continue;
            }
            $started = true;
            $this->startAction($line, $s, $action);
        }
        return $started;
    }

    public function startAction(int $line, int $sector, int $action): void {
        int $floor = $this->map->sectors[$sector]->floor_height;
        if ($action === 1) {
            $this->startDoor($sector, $this->doorNormal);
        } else if ($action === 2) {
            $this->startDoor($sector, $this->doorOpen);
        } else if ($action === 3) {
            $this->startDoor($sector, $this->doorClose);
        } else if ($action === 4) {
            $this->startDoor($sector, $this->doorCloseThenOpen);
        } else if ($action === 10) {
            // lift: down to the lowest neighboring floor, wait 3 seconds, back up
            int $slot = $this->newMover($sector, $this->kindLift);
            if ($slot >= 0) {
                $this->movers[$slot]->bottom = $this->lowestFloorAround($sector);
                $this->movers[$slot]->top = $floor;
                $this->movers[$slot]->speed = 4;
                $this->movers[$slot]->wait = 105;
                $this->movers[$slot]->direction = -1;
            }
        } else if ($action === 20) {
            $this->startFloor($sector, $this->lowestFloorAround($sector), 1, -1);
        } else if ($action === 21) {
            int $target = $this->highestFloorAround($sector);
            if ($target !== $floor) {
                int $raised = $target + 8;
                $target = $raised;
            }
            $this->startFloor($sector, $target, 4, -1);
        } else if ($action === 22) {
            int $target = $this->lowestCeilingAround($sector);
            if ($target > $this->map->sectors[$sector]->ceiling_height) {
                $target = $this->map->sectors[$sector]->ceiling_height;
            }
            $this->startFloor($sector, $target, 1, -1);
        } else if ($action === 23) {
            $this->startFloor($sector, $this->nextHighestFloor($sector), 1, -1);
        } else if ($action === 24) {
            // raise to the next floor and take the floor texture of the line's front sector
            int $slot = $this->startFloor($sector, $this->nextHighestFloor($sector), 1, -1);
            int $front = $this->map->linedefs[$line]->front_sector;
            if ($slot >= 0 && $front >= 0) {
                $this->movers[$slot]->texture = $this->map->sectors[$front]->floor_pic;
            }
        } else if ($action === 30) {
            $this->buildStairs($sector);
        }
    }

    public function startFloor(int $sector, int $target, int $speed, int $texture): int {
        int $slot = $this->newMover($sector, $this->kindFloor);
        if ($slot < 0) {
            return -1;
        }
        int $floor = $this->map->sectors[$sector]->floor_height;
        $this->movers[$slot]->top = $target;
        $this->movers[$slot]->speed = $speed;
        $this->movers[$slot]->texture = $texture;
        $this->movers[$slot]->direction = 1;
        if ($target < $floor) {
            $this->movers[$slot]->direction = -1;
        }
        return $slot;
    }

    // Raise the tagged sector by 8, then each following step sector of the same flat by 8 more.
    public function buildStairs(int $sector): void {
        buffer<Linedef> $lines = $this->map->linedefs;
        int $current = $sector;
        int $texture = $this->map->sectors[$sector]->floor_pic;
        int $height = $this->map->sectors[$sector]->floor_height + 8;
        $this->startFloor($current, $height, 1, -1);
        for ($guard = 0; $guard < 64; $guard++) {
            int $next = -1;
            int $start = $this->sectorLineStart[$current];
            int $end = $start + $this->sectorLineCount[$current];
            for ($at = $start; $at < $end; $at++) {
                int $line = $this->sectorLines[$at];
                if ($lines[$line]->front_sector !== $current || $lines[$line]->back_sector < 0) {
                    continue;
                }
                int $candidate = $lines[$line]->back_sector;
                if ($this->map->sectors[$candidate]->floor_pic !== $texture || $this->map->sectors[$candidate]->mover !== 0) {
                    continue;
                }
                $next = $candidate;
                break;
            }
            if ($next < 0) {
                return;
            }
            int $raised = $height + 8;
            $height = $raised;
            $current = $next;
            $this->startFloor($current, $height, 1, -1);
        }
    }

    public function tickMovers(): void {
        buffer<Mover> $movers = $this->movers;
        buffer<Sector> $sectors = $this->map->sectors;
        for ($i = 0; $i < $this->moverCount; $i++) {
            if ($movers[$i]->active === 0) {
                continue;
            }
            int $sector = $movers[$i]->sector;
            int $kind = $movers[$i]->kind;
            int $direction = $movers[$i]->direction;
            int $speed = $movers[$i]->speed;
            if ($direction === 0) {
                $movers[$i]->countdown--;
                if ($movers[$i]->countdown > 0) {
                    continue;
                }
                if ($kind === $this->kindLift) {
                    $movers[$i]->direction = 1;
                } else if ($movers[$i]->type === $this->doorCloseThenOpen) {
                    $movers[$i]->direction = 1;
                } else {
                    $movers[$i]->direction = -1;
                }
                continue;
            }

            if ($kind === $this->kindDoor) {
                int $ceiling = $sectors[$sector]->ceiling_height;
                int $floor = $sectors[$sector]->floor_height;
                if ($direction > 0) {
                    int $raised = $ceiling + $speed;
                    if ($raised >= $movers[$i]->top) {
                        $sectors[$sector]->ceiling_height = $movers[$i]->top;
                        int $type = $movers[$i]->type;
                        if ($type === $this->doorNormal) {
                            $movers[$i]->direction = 0;
                            $movers[$i]->countdown = $movers[$i]->wait;
                        } else {
                            $this->finishMover($i);
                        }
                    } else {
                        $sectors[$sector]->ceiling_height = $raised;
                    }
                } else {
                    // a closing door goes back up rather than crush the player
                    if ($this->playerInSector($sector) && $movers[$i]->type !== $this->doorClose) {
                        $movers[$i]->direction = 1;
                        continue;
                    }
                    int $lowered = $ceiling - $speed;
                    if ($lowered <= $floor) {
                        $sectors[$sector]->ceiling_height = $floor;
                        if ($movers[$i]->type === $this->doorCloseThenOpen) {
                            // shut for the wait, then back up to the ceiling it started from
                            $movers[$i]->direction = 0;
                            $movers[$i]->countdown = $movers[$i]->wait;
                        } else {
                            $this->finishMover($i);
                        }
                    } else {
                        $sectors[$sector]->ceiling_height = $lowered;
                    }
                }
                $this->settleThings($sector);
                continue;
            }

            if ($kind === $this->kindLift) {
                int $floor = $sectors[$sector]->floor_height;
                if ($direction < 0) {
                    int $lowered = $floor - $speed;
                    if ($lowered <= $movers[$i]->bottom) {
                        $sectors[$sector]->floor_height = $movers[$i]->bottom;
                        $movers[$i]->direction = 0;
                        $movers[$i]->countdown = $movers[$i]->wait;
                    } else {
                        $sectors[$sector]->floor_height = $lowered;
                    }
                } else {
                    int $raised = $floor + $speed;
                    if ($raised >= $movers[$i]->top) {
                        $sectors[$sector]->floor_height = $movers[$i]->top;
                        $this->finishMover($i);
                    } else {
                        $sectors[$sector]->floor_height = $raised;
                    }
                }
                $this->settleThings($sector);
                continue;
            }

            // floors move toward their target, then may take a new flat
            int $floor = $sectors[$sector]->floor_height;
            int $target = $movers[$i]->top;
            int $step = $speed * $direction;
            int $moved = $floor + $step;
            bool $arrived = ($direction > 0 && $moved >= $target) || ($direction < 0 && $moved <= $target);
            if (!$arrived) {
                $sectors[$sector]->floor_height = $moved;
                $this->settleThings($sector);
                continue;
            }
            $sectors[$sector]->floor_height = $target;
            $this->settleThings($sector);
            if ($movers[$i]->texture >= 0) {
                $sectors[$sector]->floor_pic = $movers[$i]->texture;
                $sectors[$sector]->special = 0;
            }
            $this->finishMover($i);
        }
    }

    // Objects ride the planes of the sector they stand in: on the floor, or hung from the ceiling.
    public function settleThings(int $sector): void {
        Things $things = $this->things;
        buffer<Mobj> $mobjs = $things->mobjs;
        buffer<int> $hanging = $things->types->hanging;
        int $floor = $this->map->sectors[$sector]->floor_height;
        int $ceiling = $this->map->sectors[$sector]->ceiling_height;
        for ($i = 0; $i < $things->count; $i++) {
            if ($mobjs[$i]->sector !== $sector) {
                continue;
            }
            if ($hanging[$mobjs[$i]->kind] !== 0) {
                int $hungFrom = $ceiling - $mobjs[$i]->height;
                $mobjs[$i]->z = $hungFrom;
            } else {
                $mobjs[$i]->z = $floor;
            }
        }
    }

    public function playerInSector(int $sector): bool {
        return $this->collision->sectorAt($this->playerX, $this->playerY) === $sector;
    }

    // ---- switches

    // Shows a switch's other face; repeatable switches flip back after a second.
    public function pressSwitch(int $line, bool $repeatable): void {
        int $side = $this->map->linedefs[$line]->right_side;
        if ($side < 0) {
            return;
        }
        buffer<Sidedef> $sides = $this->map->sidedefs;
        int $part = -1;
        int $original = -1;
        if ($sides[$side]->upper >= 0 && $this->switchPair[$sides[$side]->upper] >= 0) {
            $part = 0;
            $original = $sides[$side]->upper;
            $sides[$side]->upper = $this->switchPair[$original];
        } else if ($sides[$side]->middle >= 0 && $this->switchPair[$sides[$side]->middle] >= 0) {
            $part = 1;
            $original = $sides[$side]->middle;
            $sides[$side]->middle = $this->switchPair[$original];
        } else if ($sides[$side]->lower >= 0 && $this->switchPair[$sides[$side]->lower] >= 0) {
            $part = 2;
            $original = $sides[$side]->lower;
            $sides[$side]->lower = $this->switchPair[$original];
        }
        if ($part < 0 || !$repeatable) {
            return;
        }
        for ($b = 0; $b < 16; $b++) {
            if ($this->buttons[$b]->active === 0) {
                $this->buttons[$b]->active = 1;
                $this->buttons[$b]->sidedef = $side;
                $this->buttons[$b]->part = $part;
                $this->buttons[$b]->texture = $original;
                $this->buttons[$b]->countdown = 35;
                return;
            }
        }
    }

    public function tickButtons(): void {
        buffer<Sidedef> $sides = $this->map->sidedefs;
        for ($b = 0; $b < 16; $b++) {
            if ($this->buttons[$b]->active === 0) {
                continue;
            }
            $this->buttons[$b]->countdown--;
            if ($this->buttons[$b]->countdown > 0) {
                continue;
            }
            int $side = $this->buttons[$b]->sidedef;
            int $texture = $this->buttons[$b]->texture;
            int $part = $this->buttons[$b]->part;
            if ($part === 0) {
                $sides[$side]->upper = $texture;
            } else if ($part === 1) {
                $sides[$side]->middle = $texture;
            } else {
                $sides[$side]->lower = $texture;
            }
            $this->buttons[$b]->active = 0;
        }
    }

    // ---- triggers

    // The use key on a line's front side. Returns true when something happened.
    public function useLine(int $line): bool {
        if ($this->collision->pointSide($line, $this->playerX, $this->playerY) >= 0) {
            return false;
        }
        buffer<Linedef> $lines = $this->map->linedefs;
        int $special = $lines[$line]->special;
        if ($special === 1 || $special === 26 || $special === 27 || $special === 28) {
            return $this->manualDoor($line, $this->doorNormal, $special);
        }
        if ($special === 31 || $special === 32 || $special === 33 || $special === 34) {
            bool $opened = $this->manualDoor($line, $this->doorOpen, $special);
            if ($opened) {
                $lines[$line]->special = 0;
            }
            return $opened;
        }
        if ($special === 11 || $special === 51) {
            $this->pressSwitch($line, false);
            $this->exitRequest = 1;
            if ($special === 51) {
                $this->exitRequest = 2;
            }
            return true;
        }
        // switches: [special, action, repeatable]
        int $action = 0;
        bool $repeatable = false;
        if ($special === 7) {
            $action = 30;
        } else if ($special === 9) {
            $action = -1;
        } else if ($special === 18) {
            $action = 23;
        } else if ($special === 20) {
            $action = 24;
        } else if ($special === 21) {
            $action = 10;
        } else if ($special === 23) {
            $action = 20;
        } else if ($special === 29) {
            $action = 1;
        } else if ($special === 50) {
            $action = 3;
        } else if ($special === 103) {
            $action = 2;
        } else if ($special === 42) {
            $action = 3;
            $repeatable = true;
        } else if ($special === 46 || $special === 61) {
            $action = 2;
            $repeatable = true;
        } else if ($special === 62) {
            $action = 10;
            $repeatable = true;
        } else if ($special === 63) {
            $action = 1;
            $repeatable = true;
        } else if ($special === 70) {
            $action = 21;
            $repeatable = true;
        }
        if ($action === 0) {
            return false;
        }
        bool $started = true;
        if ($action > 0) {
            $started = $this->tagged($line, $action);
        }
        if (!$started) {
            return false;
        }
        $this->pressSwitch($line, $repeatable);
        if (!$repeatable) {
            $lines[$line]->special = 0;
        }
        return true;
    }

    public function manualDoor(int $line, int $type, int $special): bool {
        if (!$this->hasKeyFor($special)) {
            return false;
        }
        int $sector = $this->map->linedefs[$line]->back_sector;
        if ($sector < 0) {
            return false;
        }
        int $marker = $this->map->sectors[$sector]->mover;
        if ($marker !== 0) {
            int $slot = $marker - 1;
            if ($type !== $this->doorNormal || $this->movers[$slot]->kind !== $this->kindDoor) {
                return false;
            }
            // using a moving door turns it around
            if ($this->movers[$slot]->direction < 0) {
                $this->movers[$slot]->direction = 1;
            } else {
                $this->movers[$slot]->direction = -1;
            }
            return true;
        }
        $this->startDoor($sector, $type);
        return true;
    }

    // Keyed doors: 26/32 blue, 27/34 yellow, 28/33 red. Keys are bits 1, 2, 4.
    public function hasKeyFor(int $special): bool {
        int $needed = 0;
        if ($special === 26 || $special === 32) {
            $needed = 1;
        } else if ($special === 27 || $special === 34) {
            $needed = 2;
        } else if ($special === 28 || $special === 33) {
            $needed = 4;
        }
        if ($needed === 0) {
            return true;
        }
        int $owned = $this->playerKeys & $needed;
        return $owned !== 0;
    }

    // Walking over a line, from its front ($fromSide < 0) or back.
    public function crossLine(int $line, int $fromSide): void {
        buffer<Linedef> $lines = $this->map->linedefs;
        int $special = $lines[$line]->special;
        int $action = 0;
        bool $repeatable = false;
        if ($special === 2) {
            $action = 2;
        } else if ($special === 3) {
            $action = 3;
        } else if ($special === 4) {
            $action = 1;
        } else if ($special === 5) {
            $action = 22;
        } else if ($special === 8) {
            $action = 30;
        } else if ($special === 10) {
            $action = 10;
        } else if ($special === 16) {
            $action = 4;
        } else if ($special === 22) {
            $action = 24;
        } else if ($special === 35) {
            $action = 35;
        } else if ($special === 36) {
            $action = 21;
        } else if ($special === 38) {
            $action = 20;
        } else if ($special === 76) {
            $action = 4;
            $repeatable = true;
        } else if ($special === 82) {
            $action = 20;
            $repeatable = true;
        } else if ($special === 86) {
            $action = 2;
            $repeatable = true;
        } else if ($special === 88) {
            $action = 10;
            $repeatable = true;
        } else if ($special === 90) {
            $action = 1;
            $repeatable = true;
        } else if ($special === 91) {
            $action = 22;
            $repeatable = true;
        } else if ($special === 98) {
            $action = 21;
            $repeatable = true;
        } else if ($special === 39 || $special === 97) {
            if ($fromSide < 0) {
                $this->teleport($line);
            }
            if ($special === 39) {
                $lines[$line]->special = 0;
            }
            return;
        } else if ($special === 52 || $special === 124) {
            $this->exitRequest = 1;
            if ($special === 124) {
                $this->exitRequest = 2;
            }
            return;
        }
        if ($action === 0) {
            return;
        }
        $this->tagged($line, $action);
        if (!$repeatable) {
            $lines[$line]->special = 0;
        }
    }

    // Moves the player onto the teleport target (thing 14) in a sector tagged like the line.
    public function teleport(int $line): void {
        int $tag = $this->map->linedefs[$line]->tag;
        buffer<Thing> $things = $this->map->things;
        for ($i = 0; $i < $this->map->thingCount; $i++) {
            if ($things[$i]->type !== 14) {
                continue;
            }
            int $sector = $this->collision->sectorAt($things[$i]->x, $things[$i]->y);
            if ($sector < 0 || $this->map->sectors[$sector]->tag !== $tag) {
                continue;
            }
            $this->teleportPending = true;
            $this->teleportX = $things[$i]->x;
            $this->teleportY = $things[$i]->y;
            $this->teleportAngle = $this->trig->fromDegrees($things[$i]->angle);
            return;
        }
    }

    public function tick(): void {
        $this->tickMovers();
        $this->tickLights();
        $this->tickButtons();
        buffer<Sidedef> $sides = $this->map->sidedefs;
        for ($s = 0; $s < $this->scrollerCount; $s++) {
            int $side = $this->scrollers[$s];
            $sides[$side]->x_offset++;
        }
    }
}
