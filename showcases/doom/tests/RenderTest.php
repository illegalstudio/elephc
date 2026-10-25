<?php

namespace Showcases\Doom\Tests;

require_once __DIR__ . '/../src/engine.php';
require_once __DIR__ . '/Fixture/TestWad.php';
require_once __DIR__ . '/Fixture/SessionCase.php';

use Showcases\Doom\App\Session;
use Showcases\Doom\Tests\Fixture\SessionCase;
use Showcases\Doom\Tests\Fixture\TestWad;

// Frames of the test map, read back from the 8-bit screen (the test WAD's light
// tables keep every color). The view is 320x168 with the horizon on row 84.
final class RenderTest extends SessionCase {
    public function testDrawsWallFloorAndCeiling(): void {
        $session = $this->boot();
        // facing the east wall from 212 units away, clear of every object
        $session->player->spawn(300, 40, 0, 0);
        $session->render("");
        $this->assertSameInt(TestWad::CEILING, $this->pixel($session, 160, 2));
        $this->assertSameInt(TestWad::WALL, $this->pixel($session, 160, 84));
        $this->assertSameInt(TestWad::FLOOR, $this->pixel($session, 160, 165));
    }

    // From the start, the grate is 192 units ahead and fills columns 53..267; the east
    // room's walls lie behind it. Through its holes the walls show, between them the
    // grate does.
    public function testTheGrateIsSeeThrough(): void {
        $session = $this->boot();
        $session->render("");
        $this->assertGreaterThan(20, $this->count($session, TestWad::GRATE, 60, 140, 84, 84));
        $this->assertGreaterThan(20, $this->count($session, TestWad::WALL, 60, 140, 84, 84));
    }

    // The health bonus stands behind the grate: the grate is drawn over it, and it
    // shows through the holes.
    public function testTheGrateCoversAnObjectBehindIt(): void {
        $session = $this->boot();
        $session->render("");
        $this->assertGreaterThan(10, $this->count($session, TestWad::GRATE, 150, 170, 76, 100));
        $this->assertGreaterThan(10, $this->count($session, TestWad::BONUS, 150, 170, 76, 100));
        $this->assertSameInt(0, $this->count($session, TestWad::WALL, 150, 170, 76, 100));
    }

    // Moved in front of the grate, the health bonus hides it.
    public function testAnObjectInFrontCoversTheGrate(): void {
        $session = $this->boot();
        $session->things->mobjs[TestWad::BONUS_OBJECT]->x = 192;
        $session->things->mobjs[TestWad::BONUS_OBJECT]->sector = 0;
        $session->render("");
        $this->assertSameInt(0, $this->count($session, TestWad::GRATE, 150, 170, 76, 100));
        $this->assertSameInt(21 * 25, $this->count($session, TestWad::BONUS, 150, 170, 76, 100));
    }

    public function pixel(Session $session, int $x, int $y): int {
        int $row = $y * 320;
        int $at = $row + $x;
        return $session->renderer->screen->pixels[$at];
    }

    // Pixels of $color in columns $x1..$x2 and rows $y1..$y2.
    public function count(Session $session, int $color, int $x1, int $x2, int $y1, int $y2): int {
        int $found = 0;
        for ($y = $y1; $y <= $y2; $y++) {
            for ($x = $x1; $x <= $x2; $x++) {
                if ($this->pixel($session, $x, $y) === $color) {
                    $found++;
                }
            }
        }
        return $found;
    }
}
