---
title: "Conditional Compilation"
description: "ifdef blocks for compile-time feature flags and platform-specific code."
sidebar:
  order: 5
---

`ifdef` selectively includes or excludes code based on compile-time symbols.

## Syntax
```php
<?php
ifdef DEBUG {
    echo "debug mode\n";
} else {
    echo "release mode\n";
}
```

## How it works
- Symbols set via `--define` CLI flags
- Resolved before include resolution and type checking
- Inactive branch completely removed from AST
- Inactive branches can reference files that don't exist

## Use cases
```php
<?php
ifdef USE_SDL {
    extern "SDL2" {
        function SDL_Init(int $flags): int;
    }
    SDL_Init(0x20);
}

ifdef DEBUG {
    if ($hp < 0) {
        echo "BUG: negative HP!\n";
        exit(1);
    }
}
```

## CLI usage
```bash
elephc --define DEBUG app.php
elephc --define DEBUG --define USE_SDL app.php
```

## Nesting
```php
<?php
ifdef PLATFORM_MAC {
    ifdef USE_METAL {
        echo "Metal renderer\n";
    } else {
        echo "OpenGL renderer\n";
    }
}
```

## Constraints
- Symbols are simple names (no expressions, no `ifndef`, no `#if`)
- Symbols come only from `--define` flags
- `ifdef` is not PHP syntax

## CLI flags reference

| Flag | Description |
|---|---|
| `--heap-size=BYTES` | Set heap buffer size (default 8MB, min 64KB) |
| `--gc-stats` | Print GC allocation/free statistics at exit |
| `--heap-debug` | Enable runtime heap verification (slow) |
| `--emit-asm` | Write the generated assembly and skip assemble/link |
| `--check` | Run the front-end pipeline without producing assembly or a binary |
| `--timings` | Print per-phase compiler timings to stderr |
| `--source-map` | Write a sidecar `.map` file next to generated assembly |
| `--define SYMBOL` | Define compile-time symbol for `ifdef` |
