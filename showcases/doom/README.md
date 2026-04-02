# DOOM Showcase

This showcase is the starting scaffold for a DOOM WAD renderer built in elephc.

The goal is not to structure it like a C demo with a few giant procedural files. The goal is to keep the project as close as possible to a real PHP codebase:

- namespaced source files under `src/`
- regular classes for orchestration and application state
- enums for modes and lifecycle state
- `packed class` and `buffer<T>` reserved for the hot path and binary/map data

## Current status

This directory currently contains only the architectural scaffold:

- folder structure
- bootstrap file with explicit `require_once`
- empty or near-empty PHP classes / enums / packed data records
- a tiny `main.php` entry point

No WAD parsing, SDL setup, BSP traversal, or rendering logic has been implemented yet.

## Why this structure

The project is intentionally split by responsibility instead of by low-level implementation step:

- `App/` holds application lifecycle and top-level runtime state
- `SDL/` wraps SDL declarations and input-facing helpers
- `IO/` holds binary-reading helpers
- `Wad/` owns WAD file and lump concepts
- `Map/` owns level-level data structures and loaders
- `Bsp/` owns traversal and visibility helpers
- `Render/` owns rendering passes
- `Player/` owns camera and movement state
- `Data/` contains compact hot-path records as `packed class`
- `Support/` contains utility helpers

This should let the final showcase read like a PHP project first, and like a low-level renderer second.

## Directory layout

```text
showcases/doom/
  main.php
  README.md
  .gitignore
  src/
    bootstrap.php
    App/
      Application.php
      Config.php
      Game.php
      GameState.php
      RenderMode.php
    SDL/
      SDL.php
      Input.php
    IO/
      BinaryReader.php
    Wad/
      WadEntry.php
      WadFile.php
      WadLoader.php
    Map/
      MapData.php
      MapLoader.php
    Bsp/
      BspWalker.php
    Render/
      Renderer.php
      WallRenderer.php
      MinimapRenderer.php
    Player/
      Camera.php
    Data/
      Vertex.php
      Linedef.php
      Sector.php
      Node.php
      Thing.php
    Support/
      Debug.php
```

## Architectural intent

### High-level PHP layer

These files should remain class-oriented and namespace-oriented:

- `App/Application.php`
- `App/Game.php`
- `Map/MapLoader.php`
- `Render/Renderer.php`
- `Player/Camera.php`

These are the files where orchestration should live.

### Hot-path / compact-data layer

These files are the right place for compiler-specific data primitives:

- `Data/*.php` as `packed class`
- future `buffer<T>` storage for vertices, segs, nodes, sectors, clip buffers, and frame data

That keeps the low-level details isolated instead of leaking through the whole application.

## Build

Right now the scaffold can be compiled like any other showcase:

```bash
cargo run -- showcases/doom/main.php
./showcases/doom/main
```

Expected output:

```text
DOOM showcase scaffold
```

## Future implementation notes

When the real implementation starts, prefer these rules:

- keep parsing, traversal, rendering, and SDL concerns in separate components
- avoid free-floating global state unless a compiler limitation truly forces it
- if shared hot-path storage is needed, hide it behind a named component instead of scattered globals
- keep the public-facing architecture object-oriented even when the innermost loops use `packed class` and `buffer<T>`
