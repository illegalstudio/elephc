<?php

namespace Showcases\Doom\Tests;

require_once __DIR__ . '/../src/engine.php';
require_once __DIR__ . '/Fixture/TestWad.php';
require_once __DIR__ . '/Fixture/SessionCase.php';

use Showcases\Doom\Tests\Fixture\SessionCase;
use Showcases\Doom\Tests\Fixture\TestWad;

final class SpecialsTest extends SessionCase {
    // Walk-over special 16: the tagged east room's ceiling comes down, stays shut for
    // 30 seconds (1050 tics) and goes back up to where it started.
    public function testCloseThenOpenDoorReopensAfterItsWait(): void {
        $session = $this->boot();
        $session->map->linedefs[0]->special = 16;
        $session->map->linedefs[0]->tag = 1;
        $session->specials->playerX = 64;
        $session->specials->playerY = 128;
        $session->specials->crossLine(0, 0);

        int $lowest = 128;
        int $shutAtTic = -1;
        for ($tic = 0; $tic < 1300; $tic++) {
            $session->specials->tick();
            int $ceiling = $session->map->sectors[1]->ceiling_height;
            if ($ceiling < $lowest) {
                $lowest = $ceiling;
            }
            if ($tic === 600) {
                $shutAtTic = $ceiling;
            }
        }
        $this->assertSameInt(0, $lowest, "the door closes to the floor");
        $this->assertSameInt(0, $shutAtTic, "the door stays shut during its wait");
        $this->assertSameInt(128, $session->map->sectors[1]->ceiling_height, "the door reopens to its old ceiling");
        $this->assertSameInt(0, $session->map->sectors[1]->mover, "the door is done");
    }

    public function testHangingObjectsFollowAMovingCeiling(): void {
        $session = $this->boot();
        $session->specials->playerX = 64;
        $session->specials->playerY = 128;
        $session->specials->startAction(TestWad::GRATE_LINE, 1, 3);
        for ($tic = 0; $tic < 20; $tic++) {
            $session->specials->tick();
        }
        int $ceiling = $session->map->sectors[1]->ceiling_height;
        $this->assertLessThan(128, $ceiling);
        int $hungFrom = $ceiling - 52;
        $this->assertSameInt($hungFrom, $session->things->mobjs[TestWad::BODY_OBJECT]->z);
    }

    public function testObjectsRideAMovingFloor(): void {
        $session = $this->boot();
        $session->map->sectors[0]->floor_height = 64;
        // raise the east room's floor to the next higher neighboring floor
        $session->specials->startAction(TestWad::GRATE_LINE, 1, 23);
        for ($tic = 0; $tic < 10; $tic++) {
            $session->specials->tick();
        }
        int $rising = $session->map->sectors[1]->floor_height;
        $this->assertGreaterThan(0, $rising);
        $this->assertSameInt($rising, $session->things->mobjs[TestWad::BONUS_OBJECT]->z);
        for ($tic = 0; $tic < 100; $tic++) {
            $session->specials->tick();
        }
        $this->assertSameInt(64, $session->map->sectors[1]->floor_height);
        $this->assertSameInt(64, $session->things->mobjs[TestWad::BONUS_OBJECT]->z);
    }
}
