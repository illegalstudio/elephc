# DOOM E1M1 — rendered in PHP, compiled to native ARM64

This showcase loads original DOOM WAD files and renders levels in real-time, entirely in PHP compiled to a standalone native binary by [elephc](https://github.com/illegalstudio/elephc).

No interpreter. No VM. No runtime dependencies beyond SDL2.

## What it does

- Parses the original `DOOM1.WAD` binary format (header, directory, lumps)
- Loads all E1M1 geometry: vertices, linedefs, sidedefs, sectors, segs, subsectors, BSP nodes
- Traverses the BSP tree front-to-back for visibility ordering
- Renders walls with per-column perspective projection and exponential distance fog
- Per-sector colors from the WAD PLAYPAL palette
- Per-sector floor/ceiling flat rendering with depth shading
- Animated sector light effects (flicker, strobe, oscillation)
- Scrolling panoramic sky
- Camera movement with WASD + arrow keys
- Step climbing (up to 24 units)
- Collision detection against walls, height steps, and low ceilings
- Doors pre-opened at load time
- Minimap overlay with player dot and thing markers
- HUD bar with compass, height indicator, and live FPS counter
- Closed door detection with distinct metallic color

## elephc features used

- `packed class` for all WAD data structures (Vertex, Linedef, Sidedef, Sector, Seg, SubSector, Node, Thing)
- `buffer<T>` for contiguous typed storage of map geometry
- `extern "SDL2" { ... }` FFI block for SDL2 function declarations
- `ptr`, `ptr_null()`, `ptr_is_null()`, `ptr_offset()`, `ptr_read8()` for SDL keyboard state
- Namespaces and `require_once` for modular project structure
- Regular classes for orchestration (Application, Game, Camera, Renderer)
- Enums for state machines (GameState, RenderMode)

## Build

Requires SDL2 and a local copy of `DOOM1.WAD` (shareware).

```bash
# compile
cargo run -- -l SDL2 -L /opt/homebrew/lib --heap-size=67108864 showcases/doom/main.php

# run
./showcases/doom/main
```

Download the shareware WAD: search for "DOOM1.WAD shareware" or extract it from the [original shareware release](https://doomwiki.org/wiki/DOOM1.WAD). Place it in this directory.

## Controls

| Key | Action |
|-----|--------|
| W / Up | Move forward |
| S / Down | Move backward |
| A | Strafe left |
| D | Strafe right |
| Left arrow | Turn left |
| Right arrow | Turn right |
| Tab | Toggle minimap |
| ESC | Quit |

## Architecture

```
main.php                 Entry point
src/
  bootstrap.php          Module loader
  App/
    Application.php      Top-level shell
    Config.php           Window size, WAD path, timing
    Game.php             Game loop, input, camera, collisions
    GameState.php         Enum: Booting, Running, Stopped
    RenderMode.php        Enum: Map2D, World3D, Split
  SDL/
    extern.php           SDL2 FFI declarations
    SDL.php              SDL wrapper (init, window, renderer)
    Input.php            Keyboard state helpers
  IO/
    BinaryReader.php     Little-endian binary parsing
  Wad/
    WadEntry.php         Lump metadata
    WadFile.php          WAD file metadata
    WadLoader.php        WAD header + directory parser
  Map/
    MapData.php          All parsed level data
    MapLoader.php        Lump parser, palette loader, door opener
  Bsp/
    BspWalker.php        BSP tree front-to-back traversal
  Render/
    Renderer.php         Orchestrator, HUD, thing rendering, FPS counter
    WallRenderer.php     BSP wall column renderer with fog and palette
    MinimapRenderer.php  2D map overlay
    Projection.php       Perspective math helpers
  Player/
    Camera.php           Position, height, angle
  Data/
    Vertex.php           packed class (x, y)
    Linedef.php          packed class (7 fields)
    Sidedef.php          packed class (3 fields)
    Sector.php           packed class (5 fields)
    Seg.php              packed class (6 fields)
    SubSector.php        packed class (2 fields)
    Node.php             packed class (14 fields)
    Thing.php            packed class (5 fields)
  Support/
    Direction.php        Trig helpers for camera movement
```

## Known limitations

- No wall textures (solid colors from WAD palette)
- No vertical clipping (far walls can bleed through portal steps) — requires `solidsegs` implementation
- Doors cannot animate at runtime (compiler limitation: #62/#65)
- Things rendered as colored columns, not sprites
- No sound
