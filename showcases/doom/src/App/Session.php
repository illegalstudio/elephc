<?php

namespace Showcases\Doom\App;

use Showcases\Doom\Bsp\BspWalker;
use Showcases\Doom\Graphics\Flats;
use Showcases\Doom\Graphics\Palette;
use Showcases\Doom\Graphics\Sprites;
use Showcases\Doom\Graphics\Textures;
use Showcases\Doom\Map\MapData;
use Showcases\Doom\Map\MapLoader;
use Showcases\Doom\Play\Collision;
use Showcases\Doom\Play\Player;
use Showcases\Doom\Play\Specials;
use Showcases\Doom\Play\Things;
use Showcases\Doom\Render\Renderer;
use Showcases\Doom\SDL\Input;
use Showcases\Doom\Support\Trig;
use Showcases\Doom\Wad\Wad;

// One game: the WAD's pictures, the current map with its objects and specials,
// the marine, and the renderer that draws them into an 8-bit frame. Nothing here
// touches SDL, so the tests drive it directly; Game adds the window and the clock.
final class Session {
    public Config $config;
    public Trig $trig;
    public Wad $wad;
    public Palette $palette;
    public Textures $textures;
    public Flats $flats;
    public Sprites $sprites;
    public MapLoader $mapLoader;
    public MapData $map;
    public Things $things;
    public BspWalker $locator;
    public Collision $collision;
    public Specials $specials;
    public Player $player;
    // created by boot() once the WAD's pictures are loaded
    public Renderer $renderer;
    public string $mapName;
    public int $levelTime;

    public function __construct(Config $config) {
        $this->config = $config;
        $this->trig = new Trig();
        $this->wad = new Wad();
        $this->palette = new Palette();
        $this->textures = new Textures();
        $this->flats = new Flats();
        $this->sprites = new Sprites();
        $this->mapLoader = new MapLoader();
        // an empty map until the first loadMap() replaces (and releases) it
        $this->map = new MapData("", -1);
        $this->things = new Things();
        $this->locator = new BspWalker();
        $this->collision = new Collision($this->map, $this->locator, $this->things);
        $this->specials = new Specials($this->map, $this->collision, $this->things, $this->textures, $this->wad, $this->trig);
        $this->player = new Player();
        $this->mapName = "";
        $this->levelTime = 0;
    }

    // Loads the WAD and its pictures, builds the renderer and enters the startup map.
    public function boot(): bool {
        string $wadPath = $this->resolveWadPath();
        if ($wadPath === "") {
            $this->say("No WAD file at " . $this->config->wadPath);
            return false;
        }
        if (!$this->wad->load($wadPath)) {
            $this->say("Failed to load WAD: " . $wadPath);
            return false;
        }
        $this->say("Loaded WAD: " . $this->wad->kind . " | lumps: " . $this->wad->lumpCount);
        if (!$this->palette->load($this->wad) || !$this->textures->load($this->wad) || !$this->flats->load($this->wad) || !$this->sprites->load($this->wad)) {
            $this->say("The WAD lacks PLAYPAL, textures, flats or sprites");
            return false;
        }
        $this->say("Graphics: " . $this->textures->count . " textures, " . $this->flats->count . " flats, " . $this->sprites->count . " sprite frames");
        $this->renderer = new Renderer($this->wad, $this->palette, $this->textures, $this->flats, $this->sprites, $this->trig, $this->config->renderWidth, $this->config->renderHeight);
        return $this->loadMap($this->config->startupMap);
    }

    // Switches to map $name. Buffers are not reclaimed on their own, so the previous
    // map, its objects, collision data and specials are released here, once the
    // renderer and the new objects no longer read them.
    public function loadMap(string $name): bool {
        MapData $map = $this->mapLoader->load($this->wad, $this->textures, $this->flats, $name);
        if (!$map->isValid()) {
            $map->release();
            $this->say("Map " . $name . " not found or incomplete");
            return false;
        }
        MapData $previous = $this->map;
        $this->map = $map;
        $this->mapName = $name;
        $this->levelTime = 0;
        $this->renderer->useMap($map);
        $this->things->spawn($map, $this->sprites, $this->wad, $this->trig, $this->locator);
        $this->collision->release();
        $this->collision = new Collision($map, $this->locator, $this->things);
        $this->specials->release();
        $this->specials = new Specials($map, $this->collision, $this->things, $this->textures, $this->wad, $this->trig);
        $previous->release();
        int $angle = $this->trig->fromDegrees($map->playerStartAngle);
        int $floor = $this->floorAt($map->playerStartX, $map->playerStartY);
        $this->player->spawn($map->playerStartX, $map->playerStartY, $angle, $floor);
        $this->player->keys = 0;
        $this->renderer->hud->say($name);
        $this->say($map->summary() . " | " . $this->things->count . " objects");
        return true;
    }

    // One game tic: the marine, the specials, pickups, then death, teleports and exits.
    public function runTic(Input $input, ptr $keys): void {
        $this->specials->playerX = $this->player->unitX();
        $this->specials->playerY = $this->player->unitY();
        $this->specials->playerKeys = $this->player->keys;
        $this->player->tick($input, $keys, $this->collision, $this->specials, $this->trig, $this->levelTime);
        $this->specials->playerX = $this->player->unitX();
        $this->specials->playerY = $this->player->unitY();
        $this->specials->tick();
        if ($this->things->touch($this->player) > 0) {
            $this->renderer->hud->say($this->player->message);
        }
        $this->renderer->hud->tick($this->levelTime, $this->specials->random());
        if ($this->player->health <= 0) {
            // dead: start the level over with a fresh marine
            $this->player->reset();
            $this->loadMap($this->mapName);
            $this->renderer->hud->say("You died");
            return;
        }
        if ($this->specials->teleportPending) {
            $this->specials->teleportPending = false;
            int $x = $this->specials->teleportX;
            int $y = $this->specials->teleportY;
            $this->player->teleport($x, $y, $this->specials->teleportAngle, $this->floorAt($x, $y));
        }
        $this->levelTime++;
        if ($this->specials->exitRequest !== 0) {
            $this->loadMap($this->nextMap($this->specials->exitRequest === 2));
        }
    }

    // Draws the current view (or the automap) and the status bar into the 8-bit frame.
    public function render(string $fps): void {
        $this->renderer->render($this->map, $this->things, $this->player, $this->levelTime, $fps);
    }

    // E1M8 ends the episode, the secret exit leads to E1M9, and E1M9 returns to E1M4.
    public function nextMap(bool $secret): string {
        string $episode = substr($this->mapName, 1, 1);
        int $mission = (int)substr($this->mapName, 3, 1);
        int $next = $mission + 1;
        if ($secret) {
            $next = 9;
        } else if ($mission === 9) {
            $next = 4;
            if ($episode === "2") {
                $next = 6;
            } else if ($episode === "3") {
                $next = 7;
            }
        } else if ($mission === 8) {
            $next = 1;
        }
        string $name = "E" . $episode . "M" . $next;
        if ($this->wad->find($name) < 0) {
            return "E1M1";
        }
        return $name;
    }

    public function say(string $line): void {
        if ($this->config->verbose) {
            echo $line . "\n";
        }
    }

    public function floorAt(int $x, int $y): int {
        int $sector = $this->collision->sectorAt($x, $y);
        if ($sector < 0) {
            return 0;
        }
        return $this->map->sectors[$sector]->floor_height;
    }

    public function resolveWadPath(): string {
        if (file_exists($this->config->wadPath)) {
            return $this->config->wadPath;
        }
        string $repoRelative = "showcases/doom/" . $this->config->wadPath;
        if (file_exists($repoRelative)) {
            return $repoRelative;
        }
        return "";
    }
}
