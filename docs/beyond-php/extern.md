---
title: "FFI & Extern"
description: "Foreign Function Interface: calling C libraries, extern functions, globals, classes, and callbacks."
sidebar:
  order: 4
---

FFI lets elephc programs call C library functions directly, with automatic type marshalling.

## Declaring extern functions
```php
<?php
extern function abs(int $n): int;
extern function getpid(): int;

// With explicit library
extern "curl" function curl_easy_init(): ptr;

// Block syntax
extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
}
```

## Supported C types

| elephc type | C equivalent | Register |
|---|---|---|
| `int` | `int64_t` / `long` | x0-x7 |
| `float` | `double` | d0-d7 |
| `bool` | `int` (0/1) | x0-x7 |
| `string` | `char*` (auto null-terminated) | x0-x7 |
| `ptr` | `void*` | x0-x7 |
| `ptr<T>` | `T*` | x0-x7 |
| `void` | void (return only) | — |
| `callable` | function pointer | x0-x7 |

## String conversion
- **Calling C**: elephc creates temporary null-terminated copy, frees after call
- **C returns string**: elephc scans for `\0`, copies to owned storage

## Callbacks
```php
<?php
extern function signal(int $sig, callable $handler): ptr;

function on_signal($sig) {
    echo "caught signal " . $sig . "\n";
}

signal(15, "on_signal");
```
Callbacks must use C-compatible types only. No strings, arrays, variadic, defaults, or pass-by-reference.

## Extern globals
```php
<?php
extern global ptr $environ;
```
Uses GOT-relative addressing. String globals auto-converted.

## Extern classes (C structs)
```php
<?php
extern class Point {
    public int $x;
    public int $y;
}

$p = ptr_cast<Point>(malloc(16));
$p->x = 10;
echo $p->x;   // 10
```
Flat sequential layout, no class_id/vtable. 8-byte alignment.

## CLI linker flags

| Flag | Description |
|---|---|
| `--link LIB` / `-lLIB` | Link additional library |
| `--link-path DIR` / `-LDIR` | Add library search path |
| `--framework NAME` | Link macOS framework |

Libraries in `extern "lib" {}` blocks are linked automatically.
