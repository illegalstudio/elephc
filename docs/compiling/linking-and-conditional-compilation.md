---
title: "Linking, heap, and conditional compilation"
description: "Linking native libraries and frameworks for FFI, sizing the runtime heap, and defining compile-time symbols for ifdef branches."
sidebar:
  order: 7
---

These flags control how the binary is linked, how much heap the program gets, and
which compile-time branches are taken.

## Linking native libraries

When a program calls into C libraries through [extern/FFI](../beyond-php/extern.md),
those libraries must be linked into the binary.

### `--link` / `-l`

Links an extra native library. Accepts the spaced form, the short flag, and the
attached form; repeat it for multiple libraries.

```bash
elephc app.php --link sqlite3
elephc app.php -l sqlite3
elephc app.php -lsqlite3
```

### `--link-path` / `-L`

Adds a directory to the library search path. Repeatable.

```bash
elephc app.php -l sqlite3 -L /opt/homebrew/lib
elephc app.php --link-path /usr/local/lib
```

### `--framework`

Links a macOS framework. Repeatable.

```bash
elephc app.php --framework Cocoa --framework Metal
```

`extern "libname" { ... }` blocks in source add their own `-l` flags
automatically; the flags above are for libraries not already named in the source.
See [FFI & Extern](../beyond-php/extern.md).

## Heap size

The compiled program uses a fixed-size runtime heap, **8 MB** by default. Programs
that allocate a lot of arrays, strings, or objects may need more.

### `--heap-size`

Sets the heap size in bytes. The minimum is `65536` (64 KB).

```bash
elephc --heap-size=16777216 heavy.php   # 16 MB
```

If a program exhausts its heap it aborts with a fatal "heap memory exhausted"
error; raising `--heap-size` is the fix. See [Memory Model](../internals/memory-model.md).

## Conditional compilation

elephc supports compile-time feature branches with `ifdef`. Symbols are defined
on the command line and the branches are resolved before optimization and code
generation, so unused branches are never compiled.

### `--define` / `--define=`

Defines a compile-time symbol. Repeatable.

```bash
elephc --define DEBUG app.php
elephc --define=DEBUG --define=METAL app.php
```

```php
ifdef (DEBUG) {
    echo "debug build\n";
}
```

See [Conditional Compilation](../beyond-php/ifdef.md) for the full `ifdef` syntax
and semantics.
