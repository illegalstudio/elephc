<?php

namespace Showcases\Doom\Tests;

require_once __DIR__ . '/../src/engine.php';
require_once __DIR__ . '/Fixture/TestWad.php';
require_once __DIR__ . '/Fixture/SessionCase.php';

use Showcases\Doom\SDL\Input;
use Showcases\Doom\Tests\Fixture\SessionCase;

final class SessionTest extends SessionCase {
    public function testBootsIntoTheStartupMap(): void {
        $session = $this->boot();
        $this->assertSameString("E1M1", $session->mapName);
        $this->assertSameInt(2, $session->map->sectorCount);
        $this->assertSameInt(3, $session->things->count);
        $this->assertSameInt(64, $session->player->unitX());
        $this->assertSameInt(128, $session->player->unitY());
    }

    // Each map change allocates a few dozen buffers and at most 4096 can be live, so
    // without releasing the previous map's buffers this dies after about 130 loads.
    public function testMapChangesReleaseThePreviousMap(): void {
        $session = $this->boot();
        int $loaded = 0;
        for ($i = 0; $i < 400; $i++) {
            string $name = "E1M1";
            if ($i % 2 === 0) {
                $name = "E1M2";
            }
            if ($session->loadMap($name)) {
                $loaded++;
            }
        }
        $this->assertSameInt(400, $loaded);
        $this->assertSameString("E1M1", $session->mapName);
        $this->assertSameInt(3, $session->things->count);
    }

    public function testAMissingMapKeepsTheCurrentOne(): void {
        $session = $this->boot();
        for ($i = 0; $i < 400; $i++) {
            $this->assertFalse($session->loadMap("E9M9"));
        }
        $this->assertSameString("E1M1", $session->mapName);
    }

    public function testTheExitLoadsTheNextMap(): void {
        $session = $this->boot();
        buffer<u8> $keys = buffer_new<u8>(512);
        $session->specials->exitRequest = 1;
        $session->runTic(new Input(), buffer_ptr($keys));
        $this->assertSameString("E1M2", $session->mapName);
        $this->assertSameInt(0, $session->specials->exitRequest);
        buffer_free($keys);
    }

    public function testDyingRestartsTheMap(): void {
        $session = $this->boot();
        buffer<u8> $keys = buffer_new<u8>(512);
        $session->player->health = 0;
        $session->runTic(new Input(), buffer_ptr($keys));
        $this->assertSameString("E1M1", $session->mapName);
        $this->assertSameInt(100, $session->player->health);
        $this->assertSameInt(64, $session->player->unitX());
        buffer_free($keys);
    }
}
