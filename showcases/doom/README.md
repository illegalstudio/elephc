<p align="center">
  <img src="https://raw.githubusercontent.com/illegalstudio/elephc/main/assets/logo-mark.png" alt="elephc logo" width="130">
</p>

<h1 align="center">DOOM</h1>

<p align="center">
  <em>Rendered in PHP. Compiled to a native binary.</em>
</p>

<p align="center">
  <a href="https://github.com/getelephc/doom/stargazers"><img src="https://img.shields.io/github/stars/getelephc/doom?style=flat-square&logo=github&logoColor=white&label=stars&color=B91C1C" alt="Stars"></a>
  <a href="https://github.com/illegalstudio/elephc"><img src="https://img.shields.io/badge/built%20with-elephc-B91C1C?style=flat-square" alt="Built with elephc"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/getelephc/doom?style=flat-square&color=B91C1C" alt="License: MIT"></a>
  <a href="https://x.com/nahime0"><img src="https://img.shields.io/badge/Follow-%40nahime0-B91C1C?style=flat-square&logo=x&logoColor=white" alt="Follow @nahime0 on X"></a>
</p>

<p align="center">
  <strong>Pure PHP &middot; 320x200 at 35 tics per second &middot; about 2 ms per frame &middot; only SDL2 at run time</strong>
</p>

<p align="center">
  The shareware episode of DOOM, played from the original <code>DOOM1.WAD</code>, written entirely in PHP and compiled to a standalone native binary by <a href="https://github.com/illegalstudio/elephc">elephc</a>. No interpreter, no VM, no runtime dependencies beyond SDL2.
</p>

<p align="center">
  <a href="https://elephc.dev"><strong>elephc Official Website</strong></a>
</p>

---

<p align="center">
  <img src="https://raw.githubusercontent.com/getelephc/static/main/assets/doom/0.27.6/doom-readme-fluid.gif" alt="DOOM played by the PHP engine compiled with elephc" width="512">
</p>

<p align="center">
  <a href="https://raw.githubusercontent.com/getelephc/static/main/assets/doom/0.27.6/doom-walkthrough.mp4">Download the walkthrough (MP4)</a>
</p>

The engine is written from the public descriptions of the WAD format and of how DOOM draws and moves (the Unofficial DOOM Specs and the DoomWiki), not translated from DOOM's source code.

> This showcase is developed in [illegalstudio/elephc](https://github.com/illegalstudio/elephc/tree/main/showcases/doom). [getelephc/doom](https://github.com/getelephc/doom) is a read-only mirror published from there, so send changes to elephc.

## What it does

- Reads the whole WAD into one byte buffer: lump directory, PLAYPAL palettes, COLORMAP light tables, PNAMES and TEXTURE1/TEXTURE2 wall textures composed from their patches, flats, sprites and status bar pictures
- Draws DOOM's own 320x200 8-bit frame and shows it at 4:3 in a 960x720 window
- Renders the BSP front to back one screen column at a time, with DOOM-style vertical clipping: every pixel of the view is written once
- Wall textures with sidedef offsets and upper/lower unpegging, floors and ceilings textured by per-pixel floor casting, the sky where the ceiling is `F_SKY1`
- See-through middle textures on two-sided lines (grates), painted with the sprites from far to near so each covers what lies behind it
- DOOM's light model: sector light bands, fake contrast on axis-aligned walls, darkening with distance through COLORMAP
- Animated flats and walls (nukage, water, lava, dripping slime), scrolling walls, blinking, strobing, glowing and flickering sector lights
- Map objects as sprites with eight rotations and mirroring, clipped by the walls in front of them, for skill 3 ("Hurt me plenty")
- DOOM's movement at 35 tics per second: thrust, friction and momentum, running, slow-start turning, view bob, smooth steps, gravity
- Collision against the BLOCKMAP: walls, steps over 24 units, openings under 56 units, solid objects
- Doors (manual, keyed, tagged, timed, close-then-open), lifts, rising and lowering floors, stairs, switches that change texture, walk-over triggers, teleporters; objects ride the floors and ceilings that move under them
- Pickups (health, armor, ammo, keys) within the marine's reach, damaging floors, the red and gold palette flashes
- The status bar with the WAD's own digits, weapon panel, keys and the marine's face
- The automap (Tab), drawing only the lines you have seen
- Exits load the next map (E1M1 to E1M8, the secret exit to E1M9)
- Frame rate and render time per frame on screen, in the window title and on stdout

## elephc features used

- `buffer<u8>` for the WAD file, the 8-bit frame and the composed textures; `buffer<u32>` for the ARGB frame handed to `SDL_UpdateTexture` through `buffer_ptr()`
- `packed class` records in `buffer<T>` for map geometry, objects, movers and light effects
- `extern "SDL2" { ... }` for the window, the texture and the keyboard; `file_get_contents()` and `ptr_write_string()` copy the WAD into its byte buffer
- `buffer_free()` when a map is left: buffers are manually managed, so each map's records, objects and specials are released on the next map change
- `elephc test` for the showcase's own test suite
- Typed properties and locals everywhere, so the hot code never touches boxed `mixed` values

## Build

Requires [elephc](https://github.com/illegalstudio/elephc#install), SDL2 and a local copy of `DOOM1.WAD` (shareware). From this directory:

```bash
# compile
elephc -l SDL2 -L /opt/homebrew/lib --heap-size=134217728 main.php

# run
./main
```

In the elephc repository, build from its root with `cargo run -- -l SDL2 -L /opt/homebrew/lib --heap-size=134217728 showcases/doom/main.php` and run `./showcases/doom/main`.

Download the shareware WAD: search for "DOOM1.WAD shareware" or extract it from the [original shareware release](https://doomwiki.org/wiki/DOOM1.WAD). Place it in this directory.

## Tests

```bash
elephc test tests
```

In the elephc repository, run `cargo run -- test showcases/doom/tests` from its root.

The suite needs neither SDL nor `DOOM1.WAD`: it writes a small WAD of its own (two rooms joined by a grate, a few objects, light tables that keep every color) and checks map changes, timed doors, objects on moving floors and ceilings, pickup reach and rendered frames read back from the 8-bit screen. elephc's CI runs it on every pull request, and the mirror's CI runs it against the elephc nightly on every push.

## Controls

| Key | Action |
|-----|--------|
| W / Up | Move forward |
| S / Down | Move backward |
| A | Strafe left |
| D | Strafe right |
| Left / Right arrow | Turn |
| Shift | Run |
| Space / E | Use (doors, switches) |
| Tab | Automap |
| V | Toggle vsync (off shows the uncapped frame rate) |
| ESC | Quit |

## Architecture

```
main.php                 Entry point
src/
  bootstrap.php          Module loader: the engine plus SDL and the window
  engine.php             Module loader for everything but SDL (what the tests load)
  App/
    Application.php      Top-level shell
    Config.php           Window and frame size, WAD path, tic rate, vsync
    Session.php          WAD, current map, objects, specials, player and renderer; map changes and game tics
    Game.php             Window, game loop at 35 tics per second, stats
    GameState.php        Enum: Booting, Running, Stopped
  SDL/
    extern.php           SDL2 FFI declarations
    SDL.php              Window, renderer, streaming texture, vsync
    Input.php            Keyboard state
  Wad/
    Wad.php              Whole-file byte buffer, lump directory, little-endian reads
  Graphics/
    Palette.php          PLAYPAL palettes and COLORMAP
    Textures.php         Wall textures composed from patches
    Flats.php            Floor and ceiling flats
    Sprites.php          Sprite frames and rotations
    Animations.php       Cycling textures and flats
  Map/
    MapData.php          One map's records and BLOCKMAP
    MapLoader.php        Lump parsing and resolution of textures, flats and sectors
  Bsp/
    BspWalker.php        Front-to-back traversal and point-in-subsector lookup
  Render/
    Renderer.php         One frame: 3D view or automap, status bar, overlay
    BspRenderer.php      Clipped column renderer for walls, flats, sky and see-through middle textures
    SpriteRenderer.php   Scaled, rotated and clipped sprites
    LightTables.php      Light bands and distance fading
    Screen.php           The 8-bit frame, patches, lines, palette conversion
    Hud.php              Status bar, face, messages, FPS text
    Automap.php          The map view
  Play/
    Player.php           Movement physics, use, pickups, damage
    Collision.php        BLOCKMAP collision and use traces
    Specials.php         Doors, lifts, floors, stairs, switches, teleporters, lights
    Things.php           Spawned objects and pickups
    ThingTypes.php       Sprite, size and behavior per thing type
  Data/                  packed classes: map records, Mobj, Mover, LightEffect, Button
  Support/
    Trig.php             Fine-angle sine and cosine tables
tests/
  Fixture/TestWad.php    The WAD the tests write: two rooms, a grate, sprites, identity light tables
  Fixture/SessionCase.php  Boots a Session on that WAD
  *Test.php              Map changes, specials, pickups, rendering
```

## Performance notes

A frame takes about 2 ms on a desktop x86_64 CPU, mostly spent writing floor, wall and sprite pixels. The per-frame code keeps to the shapes elephc compiles to plain machine integers; loading code is written freely:

- Loops count with `$i++` from a local, against a bound held in an `int` local or written as `$n - 1`; `$i += 1`, `$i = $i + 1`, `<=` and `$n + 1` bounds keep the counter boxed, so inclusive ranges use an `int $end = $last + 1;` bound, and a loop starting at `$start + 1` declares that start as an `int` local first.
- Integer arithmetic is written one operation per typed declaration (`int $p = $a * $b;`), and locals are reassigned only from other locals, constants or call results. A nested operation, an arithmetic call argument or an arithmetic reassignment may overflow to float, so elephc boxes it.
- Hot calls pass only ints and buffers: an object argument costs about 20 ns per call, ten times an int-only call.

## Known limitations

- No weapons, shooting or monster AI: monsters stand still
- No sound, menus or intermission screens
- Gun-triggered lines open when used; the donut special does nothing
- Stairs rise four times faster than in the original
