---
title: "Buffers"
description: "Fixed-size contiguous arrays with buffer<T> for hot-path data and game loops."
sidebar:
  order: 2
---

`buffer<T>` is a fixed-size contiguous array of POD values or packed records. Designed for game loops, renderers, and performance-critical code where hash table overhead is unacceptable.

## Why not PHP arrays?

PHP arrays are hash tables. Every access goes through hashing, linear probing, entry comparison. `buffer<T>` compiles to a single `ldr` instruction: `base + 16 + index * stride`.

## Creating buffers

```php
<?php
buffer<int> $ids = buffer_new<int>(1000);
buffer<float> $speeds = buffer_new<float>(1000);
buffer<Enemy> $enemies = buffer_new<Enemy>(256);
```

Only POD scalar, pointer, or packed-record element types are accepted. No union types (`buffer<int|string>`) or nullable (`buffer<?int>`).

## Buffer builtins

| Function | Signature | Description |
|---|---|---|
| `buffer_new<T>()` | `buffer_new<T>($length): buffer<T>` | Allocate a fixed-size buffer with `$length` elements of type `T` |
| `buffer_len()` | `buffer_len($buffer): int` | Return the logical element count stored in the buffer header |
| `buffer_free()` | `buffer_free($buffer): void` | Release a local buffer variable and nullify it |

## Reading and writing

```php
<?php
$buf[3] = 42;          // direct store
echo $buf[3];           // direct load

$enemies[0]->x = 100;  // packed class field access
echo $enemies[0]->x;   // 100
```

## Buffer length

```php
<?php
echo buffer_len($data);   // 512
```

## Freeing buffers

```php
<?php
buffer_free($buf);   // release heap memory, nullify variable
```

Use-after-free produces: `Fatal error: use of buffer after buffer_free()`

Restrictions:
- Only accepts plain local variables
- Aliases after free are undefined behavior
- Double-free is undefined behavior

## Bounds checking

Always enabled. Out-of-bounds aborts: `Fatal error: buffer index out of bounds`

## Memory layout

```
Offset 0:   [length: 8 bytes]
Offset 8:   [stride: 8 bytes]
Offset 16:  [element 0]
Offset 16 + stride: [element 1]
...
```

## SoA vs AoS patterns

**Structure of Arrays (SoA)** — better cache locality for single-field iteration:

```php
<?php
buffer<float> $x = buffer_new<float>(1000);
buffer<float> $y = buffer_new<float>(1000);
for ($i = 0; $i < 1000; $i++) {
    $x[$i] = $x[$i] + $speed * $dt;
}
```

**Array of Structures (AoS)** — better when accessing all fields together:

```php
<?php
packed class Particle { float $x; float $y; float $vx; float $vy; }
buffer<Particle> $particles = buffer_new<Particle>(10000);
for ($i = 0; $i < buffer_len($particles); $i++) {
    $particles[$i]->x = $particles[$i]->x + $particles[$i]->vx;
}
```

## Limitations

- Fixed size — no push, pop, or dynamic resize
- No automatic cleanup — use `buffer_free()` explicitly
- No conversion to/from PHP arrays
- No copy-on-write semantics
- No `foreach` iteration
- No mixed element types
- Payload is zero-initialized by `buffer_new`
