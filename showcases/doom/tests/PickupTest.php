<?php

namespace Showcases\Doom\Tests;

require_once __DIR__ . '/../src/engine.php';
require_once __DIR__ . '/Fixture/TestWad.php';
require_once __DIR__ . '/Fixture/SessionCase.php';

use Showcases\Doom\App\Session;
use Showcases\Doom\Tests\Fixture\SessionCase;
use Showcases\Doom\Tests\Fixture\TestWad;

// The blue key card lies at (128, 64) in the west room, whose floor is at 0.
final class PickupTest extends SessionCase {
    public function testTakesAnItemAtTheFeet(): void {
        $session = $this->standOnTheKey(0);
        $this->assertSameInt(1, $session->things->touch($session->player));
        $this->assertSameInt(1, $session->player->keys);
        $this->assertSameInt(0, $session->things->mobjs[TestWad::KEY_OBJECT]->active);
    }

    public function testTakesAnItemWithinTheBody(): void {
        $session = $this->standOnTheKey(40);
        $this->assertSameInt(1, $session->things->touch($session->player));
    }

    public function testLeavesAnItemOnALedgeOverhead(): void {
        $session = $this->standOnTheKey(100);
        $this->assertSameInt(0, $session->things->touch($session->player));
        $this->assertSameInt(0, $session->player->keys);
        $this->assertSameInt(1, $session->things->mobjs[TestWad::KEY_OBJECT]->active);
    }

    public function testLeavesAnItemFarBelowTheFeet(): void {
        $session = $this->standOnTheKey(-20);
        $this->assertSameInt(0, $session->things->touch($session->player));
    }

    // The player on the key's spot with the key raised (or sunk) to $height.
    public function standOnTheKey(int $height): Session {
        $session = $this->boot();
        $session->player->spawn(128, 64, 0, 0);
        $session->things->mobjs[TestWad::KEY_OBJECT]->z = $height;
        return $session;
    }
}
