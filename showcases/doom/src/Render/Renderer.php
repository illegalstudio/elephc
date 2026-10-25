<?php

namespace Showcases\Doom\Render;

use Showcases\Doom\Bsp\BspWalker;
use Showcases\Doom\Graphics\Flats;
use Showcases\Doom\Graphics\Palette;
use Showcases\Doom\Graphics\Sprites;
use Showcases\Doom\Graphics\Textures;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Play\Player;
use Showcases\Doom\Play\Things;
use Showcases\Doom\Support\Trig;
use Showcases\Doom\Wad\Wad;

// One frame: animate pictures, then either the 3D view (BSP walk front to back, map
// objects and see-through middle textures over it, far to near) or the automap,
// then the status bar and the text overlay.
final class Renderer {
    public Screen $screen;
    public Wad $wad;
    public Palette $palette;
    public Textures $textures;
    public Flats $flats;
    public Trig $trig;
    public LightTables $lights;
    public BspWalker $bsp;
    public BspRenderer $world;
    public SpriteRenderer $spriteRenderer;
    public Hud $hud;
    public Automap $automap;
    public bool $showAutomap;
    public int $viewHeight;

    public function __construct(Wad $wad, Palette $palette, Textures $textures, Flats $flats, Sprites $sprites, Trig $trig, int $width, int $height) {
        $this->wad = $wad;
        $this->palette = $palette;
        $this->textures = $textures;
        $this->flats = $flats;
        $this->trig = $trig;
        $this->screen = new Screen($width, $height);
        // the status bar takes the bottom 32 rows, as in DOOM
        $this->viewHeight = $height - 32;
        $this->lights = new LightTables();
        $this->world = new BspRenderer($this->screen, $this->viewHeight, $wad, $palette, $textures, $flats, $this->lights);
        $this->bsp = new BspWalker();
        $this->spriteRenderer = new SpriteRenderer($this->world, $sprites, $wad);
        $this->hud = new Hud($wad, $this->screen, $this->viewHeight);
        $this->automap = new Automap($this->screen, $this->viewHeight);
        $this->showAutomap = false;
    }

    public function useMap(MapData $map): void {
        $this->world->useMap($map);
    }

    public function render(MapData $map, Things $things, Player $player, int $tic, string $fps): void {
        $this->textures->animate($tic);
        $this->flats->animate($tic);
        if ($this->showAutomap) {
            $this->automap->draw($map, $player, $this->trig);
            $this->hud->drawStatusBar($player);
            $this->hud->drawOverlay($fps);
            return;
        }
        int $viewX = $player->unitX();
        int $viewY = $player->unitY();
        int $viewZ = $player->viewZ >> 16;
        int $viewAngle = $player->angle;
        int $count = $this->bsp->walk($map, $viewX, $viewY);
        $this->world->beginFrame($viewX, $viewY, $viewZ, $viewAngle, $this->trig->cos($viewAngle), $this->trig->sin($viewAngle));
        buffer<int> $order = $this->bsp->order;
        for ($o = 0; $o < $count; $o++) {
            if ($this->world->openColumns <= 0) {
                break;
            }
            $this->world->renderSubSector($order[$o]);
        }
        $this->world->finishFrame();
        $this->spriteRenderer->render($things, $tic);
        $this->world->drawMaskedSegs();
        $this->hud->drawStatusBar($player);
        $this->hud->drawOverlay($fps);
    }
}
