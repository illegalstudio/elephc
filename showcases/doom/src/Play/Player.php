<?php

namespace Showcases\Doom\Play;

use Showcases\Doom\SDL\Input;
use Showcases\Doom\Support\Trig;

// The player as DOOM moves it, one tic at a time: input adds thrust to momentum,
// momentum moves the player through the collision checks, friction slows it, and
// the view height eases back after steps while bobbing with speed.
// Position, height and momentum are 16.16 fixed point.
final class Player {
    public int $x;
    public int $y;
    public int $z;
    public int $momX;
    public int $momY;
    public int $momZ;
    public int $angle;
    public int $turnHeld;
    public int $viewHeight;
    public int $deltaViewHeight;
    public int $viewZ;
    public int $floorZ;
    public int $ceilingZ;
    public bool $useHeld;
    public int $keys;
    public int $health;
    public int $armor;
    public int $armorType;
    public int $bullets;
    public int $shells;
    public int $weapons;
    public int $damageCount;
    public int $bonusCount;
    public string $message;

    public function __construct() {
        $this->x = 0;
        $this->y = 0;
        $this->z = 0;
        $this->momX = 0;
        $this->momY = 0;
        $this->momZ = 0;
        $this->angle = 0;
        $this->turnHeld = 0;
        $this->viewHeight = 2686976;
        $this->deltaViewHeight = 0;
        $this->viewZ = 0;
        $this->floorZ = 0;
        $this->ceilingZ = 0;
        $this->useHeld = false;
        $this->keys = 0;
        $this->message = "";
        $this->reset();
    }

    // A fresh marine: 100% health, a pistol with 50 bullets.
    public function reset(): void {
        $this->keys = 0;
        $this->health = 100;
        $this->armor = 0;
        $this->armorType = 0;
        $this->bullets = 50;
        $this->shells = 0;
        $this->weapons = 6;
        $this->damageCount = 0;
        $this->bonusCount = 0;
    }

    // Palette to show: red while hurt, gold right after a pickup.
    public function palette(): int {
        if ($this->damageCount > 0) {
            int $rounded = $this->damageCount + 7;
            int $red = $rounded >> 3;
            if ($red > 8) {
                return 8;
            }
            return $red;
        }
        if ($this->bonusCount > 0) {
            int $rounded = $this->bonusCount + 7;
            int $gold = $rounded >> 3;
            if ($gold > 4) {
                $gold = 4;
            }
            int $palette = 8 + $gold;
            return $palette;
        }
        return 0;
    }

    // Damage with armor soaking a third (green) or half (blue) of it.
    public function damage(int $amount): void {
        int $absorbed = 0;
        if ($this->armorType === 1) {
            $absorbed = intdiv($amount, 3);
        } else if ($this->armorType === 2) {
            $absorbed = intdiv($amount, 2);
        }
        if ($absorbed > $this->armor) {
            $absorbed = $this->armor;
        }
        int $armorLeft = $this->armor - $absorbed;
        $this->armor = $armorLeft;
        if ($this->armor === 0) {
            $this->armorType = 0;
        }
        int $taken = $amount - $absorbed;
        int $healthLeft = $this->health - $taken;
        $this->health = $healthLeft;
        int $hurt = $this->damageCount + $taken;
        $this->damageCount = $hurt;
        if ($this->damageCount > 100) {
            $this->damageCount = 100;
        }
    }

    // Applies an item; false when the player has no use for it yet.
    public function pickup(int $type): bool {
        if ($type === 2011 || $type === 2012) {
            if ($this->health >= 100) {
                return false;
            }
            int $amount = 10;
            $this->message = "Stimpack";
            if ($type === 2012) {
                $amount = 25;
                $this->message = "Medikit";
            }
            $this->health = $this->capped($this->health, $amount, 100);
        } else if ($type === 2014) {
            $this->health = $this->capped($this->health, 1, 200);
            $this->message = "Health bonus";
        } else if ($type === 2013) {
            $this->health = $this->capped($this->health, 100, 200);
            $this->message = "Soulsphere";
        } else if ($type === 2015) {
            $this->armor = $this->capped($this->armor, 1, 200);
            if ($this->armorType === 0) {
                $this->armorType = 1;
            }
            $this->message = "Armor bonus";
        } else if ($type === 2018 || $type === 2019) {
            int $level = 100;
            int $kind = 1;
            $this->message = "Green armor";
            if ($type === 2019) {
                $level = 200;
                $kind = 2;
                $this->message = "Blue armor";
            }
            if ($this->armor >= $level) {
                return false;
            }
            $this->armor = $level;
            $this->armorType = $kind;
        } else if ($type === 2007 || $type === 2048 || $type === 2002) {
            if ($this->bullets >= 200) {
                return false;
            }
            int $amount = 10;
            $this->message = "Clip";
            if ($type === 2048) {
                $amount = 50;
                $this->message = "Box of bullets";
            } else if ($type === 2002) {
                $amount = 20;
                $this->message = "Chaingun";
                int $owned = $this->weapons | 16;
                $this->weapons = $owned;
            }
            $this->bullets = $this->capped($this->bullets, $amount, 200);
        } else if ($type === 2008 || $type === 2049 || $type === 2001) {
            int $amount = 4;
            $this->message = "Shells";
            if ($type === 2049) {
                $amount = 20;
                $this->message = "Box of shells";
            } else if ($type === 2001) {
                $amount = 8;
                $this->message = "Shotgun";
                int $owned = $this->weapons | 8;
                $this->weapons = $owned;
            }
            if ($this->shells >= 50 && $type !== 2001) {
                return false;
            }
            $this->shells = $this->capped($this->shells, $amount, 50);
        } else if ($type === 5 || $type === 40) {
            int $owned = $this->keys | 1;
            $this->keys = $owned;
            $this->message = "Blue key";
        } else if ($type === 6 || $type === 39) {
            int $owned = $this->keys | 2;
            $this->keys = $owned;
            $this->message = "Yellow key";
        } else if ($type === 13 || $type === 38) {
            int $owned = $this->keys | 4;
            $this->keys = $owned;
            $this->message = "Red key";
        } else if ($type === 8) {
            $this->bullets = $this->capped($this->bullets, 10, 200);
            $this->shells = $this->capped($this->shells, 4, 50);
            $this->message = "Backpack";
        } else {
            return false;
        }
        $this->bonusCount = 6;
        return true;
    }

    public function capped(int $value, int $amount, int $limit): int {
        int $sum = $value + $amount;
        if ($sum > $limit) {
            return $limit;
        }
        return $sum;
    }

    public function spawn(int $x, int $y, int $angle, int $floor): void {
        $this->x = $x << 16;
        $this->y = $y << 16;
        $this->z = $floor << 16;
        $this->momX = 0;
        $this->momY = 0;
        $this->momZ = 0;
        $this->angle = $angle;
        $this->viewHeight = 2686976;
        $this->deltaViewHeight = 0;
        $this->viewZ = $this->z + $this->viewHeight;
    }

    public function unitX(): int {
        return $this->x >> 16;
    }

    public function unitY(): int {
        return $this->y >> 16;
    }

    public function unitZ(): int {
        return $this->z >> 16;
    }

    public function tick(Input $input, ptr $keys, Collision $collision, Specials $specials, Trig $trig, int $levelTime): void {
        bool $run = $input->run($keys) !== 0;
        bool $left = $input->turnLeft($keys) !== 0;
        bool $right = $input->turnRight($keys) !== 0;

        // turning: slow for the first 6 tics, then 80 (walking) or 160 (running) fine angles per tic
        if ($left || $right) {
            $this->turnHeld++;
        } else {
            $this->turnHeld = 0;
        }
        int $turnSpeed = 80;
        if ($run) {
            $turnSpeed = 160;
        }
        if ($this->turnHeld < 6) {
            $turnSpeed = 40;
        }
        if ($left) {
            int $turned = $this->angle + $turnSpeed;
            $this->angle = $turned & 8191;
        }
        if ($right) {
            int $turned = $this->angle - $turnSpeed;
            $this->angle = $turned & 8191;
        }

        // thrust: DOOM's forward move 25 (50 running) and side move 24 (40), times 2048
        int $forward = 0;
        int $side = 0;
        int $forwardStep = 25;
        int $sideStep = 24;
        if ($run) {
            $forwardStep = 50;
            $sideStep = 40;
        }
        if ($input->moveForward($keys) !== 0) {
            $forward = $forwardStep;
        }
        if ($input->moveBackward($keys) !== 0) {
            $forward = -$forwardStep;
        }
        if ($input->moveRight($keys) !== 0) {
            $side = $sideStep;
        }
        if ($input->moveLeft($keys) !== 0) {
            $side = -$sideStep;
        }
        if ($forward !== 0) {
            int $forwardThrust = $forward * 2048;
            $this->thrust($trig, $this->angle, $forwardThrust);
        }
        if ($side !== 0) {
            int $sideThrust = $side * 2048;
            int $rightAngle = $this->angle - 2048;
            $this->thrust($trig, $rightAngle, $sideThrust);
        }

        $this->moveXY($collision, $specials, $forward !== 0 || $side !== 0);
        $this->moveZ($collision);
        $this->calcHeight($trig, $levelTime);
        if ($this->damageCount > 0) {
            $this->damageCount--;
        }
        if ($this->bonusCount > 0) {
            $this->bonusCount--;
        }

        // damaging floors (nukage 5, harder slime 10 or 20) hurt every 32 tics while standing on them
        int $sector = $collision->sectorAt($this->unitX(), $this->unitY());
        int $floorFixed = $this->floorZ << 16;
        int $everyThirtyTwo = $levelTime & 31;
        if ($sector >= 0 && $this->z === $floorFixed && $everyThirtyTwo === 0) {
            int $special = $collision->map->sectors[$sector]->special;
            if ($special === 7) {
                $this->damage(5);
            } else if ($special === 5) {
                $this->damage(10);
            } else if ($special === 4 || $special === 11 || $special === 16) {
                $this->damage(20);
            }
        }

        // use: once per key press, on the nearest special line within 64 units ahead
        bool $use = $input->use($keys) !== 0;
        if ($use && !$this->useHeld) {
            int $line = $collision->useTarget($this->unitX(), $this->unitY(), $trig->cos($this->angle), $trig->sin($this->angle));
            if ($line >= 0) {
                $specials->useLine($line);
            }
        }
        $this->useHeld = $use;
    }

    public function thrust(Trig $trig, int $angle, int $move): void {
        int $alongX = $move * $trig->cos($angle);
        int $alongY = $move * $trig->sin($angle);
        int $pushX = $alongX >> 16;
        int $pushY = $alongY >> 16;
        int $momX = $this->momX + $pushX;
        int $momY = $this->momY + $pushY;
        $this->momX = $momX;
        $this->momY = $momY;
    }

    public function moveXY(Collision $collision, Specials $specials, bool $pushing): void {
        // at most 30 units per tic, moved in halves when faster than 15
        int $limit = 1966080;
        if ($this->momX > $limit) {
            $this->momX = $limit;
        } else if ($this->momX < -$limit) {
            $this->momX = -$limit;
        }
        if ($this->momY > $limit) {
            $this->momY = $limit;
        } else if ($this->momY < -$limit) {
            $this->momY = -$limit;
        }
        int $remainingX = $this->momX;
        int $remainingY = $this->momY;
        int $half = 983040;
        for ($pass = 0; $pass < 4; $pass++) {
            if ($remainingX === 0 && $remainingY === 0) {
                break;
            }
            int $stepX = $remainingX;
            int $stepY = $remainingY;
            if ($remainingX > $half || $remainingX < -$half || $remainingY > $half || $remainingY < -$half) {
                int $halfX = $remainingX >> 1;
                int $halfY = $remainingY >> 1;
                $stepX = $halfX;
                $stepY = $halfY;
            }
            int $leftX = $remainingX - $stepX;
            int $leftY = $remainingY - $stepY;
            $remainingX = $leftX;
            $remainingY = $leftY;
            int $targetX = $this->x + $stepX;
            int $targetY = $this->y + $stepY;
            if ($this->tryMove($collision, $specials, $targetX, $targetY)) {
                continue;
            }
            // slide along whichever axis is still free
            if ($stepX !== 0 && $this->tryMove($collision, $specials, $targetX, $this->y)) {
                $this->momY = 0;
                $remainingY = 0;
                continue;
            }
            if ($stepY !== 0 && $this->tryMove($collision, $specials, $this->x, $targetY)) {
                $this->momX = 0;
                $remainingX = 0;
                continue;
            }
            $this->momX = 0;
            $this->momY = 0;
            break;
        }

        // friction, and a full stop below 1/16 unit per tic when no key is held
        int $stop = 4096;
        bool $slow = $this->momX > -$stop && $this->momX < $stop && $this->momY > -$stop && $this->momY < $stop;
        if ($slow && !$pushing) {
            $this->momX = 0;
            $this->momY = 0;
            return;
        }
        int $frictionX = $this->momX * 59392;
        int $frictionY = $this->momY * 59392;
        int $slowedX = $frictionX >> 16;
        int $slowedY = $frictionY >> 16;
        $this->momX = $slowedX;
        $this->momY = $slowedY;
    }

    public function tryMove(Collision $collision, Specials $specials, int $targetX, int $targetY): bool {
        int $oldX = $this->unitX();
        int $oldY = $this->unitY();
        int $newX = $targetX >> 16;
        int $newY = $targetY >> 16;
        if (!$collision->checkPosition($newX, $newY, $this->unitZ())) {
            return false;
        }
        $this->x = $targetX;
        $this->y = $targetY;
        $this->floorZ = $collision->floorZ;
        $this->ceilingZ = $collision->ceilingZ;
        // walk-over specials fire when the player's center changes side of the line
        for ($t = 0; $t < $collision->touchedCount; $t++) {
            int $line = $collision->touched[$t];
            int $before = $collision->pointSide($line, $oldX, $oldY);
            int $after = $collision->pointSide($line, $newX, $newY);
            if ($before !== $after && $before !== 0) {
                $specials->crossLine($line, $before);
            }
        }
        return true;
    }

    // Floors follow moving sectors; walking off a ledge falls with DOOM's gravity of 1 unit/tic².
    public function moveZ(Collision $collision): void {
        $collision->checkPosition($this->unitX(), $this->unitY(), $this->unitZ());
        int $floorFixed = $collision->floorZ << 16;
        int $ceilingFixed = $collision->ceilingZ << 16;
        $this->floorZ = $collision->floorZ;
        $this->ceilingZ = $collision->ceilingZ;
        if ($this->z < $floorFixed) {
            // stepped up: lift the feet now and ease the eyes up over the next tics
            int $rise = $floorFixed - $this->z;
            int $lowered = $this->viewHeight - $rise;
            $this->viewHeight = $lowered;
            int $missing = 2686976 - $this->viewHeight;
            int $ease = $missing >> 3;
            $this->deltaViewHeight = $ease;
            $this->z = $floorFixed;
            $this->momZ = 0;
        } else if ($this->z > $floorFixed) {
            int $faster = $this->momZ - 65536;
            $this->momZ = $faster;
            int $fallen = $this->z + $this->momZ;
            if ($fallen <= $floorFixed) {
                // a hard landing squats the view a little
                if ($this->momZ < -524288) {
                    int $squat = $this->momZ >> 3;
                    $this->deltaViewHeight = $squat;
                }
                $this->z = $floorFixed;
                $this->momZ = 0;
            } else {
                $this->z = $fallen;
            }
        }
        int $headroom = $ceilingFixed - 3670016;
        if ($this->z > $headroom && $headroom >= $floorFixed) {
            $this->z = $headroom;
        }
    }

    public function calcHeight(Trig $trig, int $levelTime): void {
        int $speedX = $this->momX * $this->momX;
        int $speedY = $this->momY * $this->momY;
        int $squareX = $speedX >> 16;
        int $squareY = $speedY >> 16;
        int $squares = $squareX + $squareY;
        int $bob = $squares >> 2;
        if ($bob > 1048576) {
            $bob = 1048576;
        }
        int $phase = $levelTime * 409;
        int $halfBob = $bob >> 1;
        int $wave = $halfBob * $trig->sin($phase);
        int $bobOffset = $wave >> 16;

        int $eased = $this->viewHeight + $this->deltaViewHeight;
        $this->viewHeight = $eased;
        if ($this->viewHeight > 2686976) {
            $this->viewHeight = 2686976;
            $this->deltaViewHeight = 0;
        }
        if ($this->viewHeight < 1343488) {
            $this->viewHeight = 1343488;
            if ($this->deltaViewHeight <= 0) {
                $this->deltaViewHeight = 1;
            }
        }
        if ($this->deltaViewHeight !== 0) {
            int $accelerated = $this->deltaViewHeight + 16384;
            $this->deltaViewHeight = $accelerated;
            if ($this->deltaViewHeight === 0) {
                $this->deltaViewHeight = 1;
            }
        }
        int $eye = $this->z + $this->viewHeight;
        int $viewZ = $eye + $bobOffset;
        int $ceilingFixed = $this->ceilingZ << 16;
        int $limit = $ceilingFixed - 262144;
        if ($viewZ > $limit) {
            $viewZ = $limit;
        }
        $this->viewZ = $viewZ;
    }

    public function teleport(int $x, int $y, int $angle, int $floor): void {
        $this->spawn($x, $y, $angle, $floor);
    }
}
