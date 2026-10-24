---
title: "Buffers"
description: "Fixed-size contiguous arrays with buffer<T> for hot-path data and game loops."
sidebar:
  order: 2
---

`buffer<T>` is a fixed-size contiguous array of POD values or packed records. Designed for game loops, renderers, and performance-critical code where hash table overhead is unacceptable.

> **Strict mode:** buffers are an elephc extension with no PHP equivalent. Compiling with [`--strict-php`](../compiling/cli-reference.md#strict-php-mode) rejects the `buffer<T>` type, `buffer_new<T>`, `buffer_len()`, `buffer_ptr()`, and `buffer_free()`.

## Why not PHP arrays?

PHP arrays are hash tables. Every access goes through hashing, probing, and entry
comparison. A `buffer<T>` access instead resolves an opaque handle, checks the handle
generation and bounds, then performs direct address arithmetic over a contiguous
payload: `payload + index * stride`. Repeated immutable integer index arithmetic can
be shared or moved out of loops by the EIR optimizer.

## Creating buffers

```php
<?php
buffer<int> $ids = buffer_new<int>(1000);
buffer<float> $speeds = buffer_new<float>(1000);
buffer<Enemy> $enemies = buffer_new<Enemy>(256);
```

Only POD scalar, pointer, packed-record, or C width element types are accepted. No union types (`buffer<int|string>`) or nullable (`buffer<?int>`).

## Element widths

A scalar element type may be one of the C width names, and the buffer then stores each
element at that width, so index data, pixels and vertex streams take only the bytes they
need:

| Element type | Stride | Reads as |
|---|---|---|
| `int`, `i64`, `u64`, `bool`, `ptr`, `ptr<T>` | 8 | `int`, `bool`, pointer |
| `i32`, `u32` | 4 | `int` |
| `i16`, `u16` | 2 | `int` |
| `i8`, `u8` | 1 | `int` |
| `float`, `f64` | 8 | `float` |
| `f32` | 4 | `float` |
| a packed class | the record's size | the record's address |

```php
<?php
buffer<u16> $indices = buffer_new<u16>(6);    // 12 bytes of payload
buffer<u8> $pixels = buffer_new<u8>(640 * 480);
buffer<f32> $positions = buffer_new<f32>(3 * 1024);

$indices[0] = 70000;   // stores the low 16 bits: 4464
$pixels[0] = -1;       // 255
$positions[0] = 0.1;   // the nearest single: 0.10000000149012
echo $indices[0] + 1;  // int arithmetic, as with buffer<int>
```

The width is a storage format: an element of any integer width reads as `int` and an `f32`
reads as `float`, a write takes the same value type a `buffer<int>` or `buffer<float>` element
takes and keeps only the declared width (signed widths sign-extend and unsigned widths
zero-extend on a read, an out-of-range integer keeps its low bits like a C conversion, a float
written to `f32` rounds to single precision). `buffer<i32>` and `buffer<int>` are distinct
types because their strides differ. Packed classes take the same width names for their fields;
see [Field widths](packed-classes.md#field-widths).

`buffer_ptr($buf)` is the address of the payload, so a width buffer reaches C at its width:
`buffer_ptr($indices)` of a `buffer<u16>` is the `const void*` index list that
`SDL_RenderGeometryRaw` takes with `size_indices` 2, and `buffer_ptr($positions)` of a
`buffer<f32>` is a `const float*`. See [Payload pointer](#payload-pointer).

## Buffer builtins

| Function | Signature | Description |
|---|---|---|
| `buffer_new<T>()` | `buffer_new<T>($length): buffer<T>` | Allocate a fixed-size buffer with `$length` elements of type `T` |
| `buffer_len()` | `buffer_len($buffer): int` | Return the logical element count stored in the buffer descriptor |
| `buffer_ptr()` | `buffer_ptr($buffer): ptr` | Return the address of the payload (`ptr<T>` for a buffer of packed records), valid until `buffer_free()` |
| `buffer_free()` | `buffer_free($buffer): void` | Release a local buffer variable and nullify it |

## Reading and writing

```php
<?php
$buf[3] = 42;          // direct store
echo $buf[3];           // direct load

$enemies[0]->x = 100;  // packed class field access
echo $enemies[0]->x;   // 100

$counts[$cell]++;      // in-place element update
echo $counts[$cell]--; // the old value, then the element is one lower
```

Buffer indices may be statically typed as `int` or `mixed`. A `mixed` index is converted to `int` at runtime before bounds checking; a statically known non-integer index is rejected at compile time.

`++` and `--` on an element of a scalar buffer (any integer or float width) read the element
once, step it by one and write it back, with the same bounds and freed-buffer checks as any
access. The update itself allocates nothing, so it suits counting and histogram loops; only the
value of a prefix expression on an integer element (`$x = ++$counts[$i]`) is a boxed `mixed`,
because it can be the promoted `float` described below. An integer element
steps as a PHP `int`: an increment past `PHP_INT_MAX` (or a decrement past `PHP_INT_MIN`)
promotes to `float` as in PHP, and the element then stores that float the way any float written
to the buffer is stored, by PHP's float-to-int conversion (`PHP_INT_MAX` plus one stores
`PHP_INT_MIN`); a narrow width then keeps the low bits. A float element adds or subtracts `1.0`,
and an `f32` element rounds the result when it is stored. The postfix form yields the element's
old value; the prefix form yields the new value before the store's width conversion, which for an
integer element past the integer range is the promoted `float`.

## Buffers in class members

Class members can name `buffer<T>` like any other type, including buffers of
packed classes: properties, method parameters and return types, promoted
constructor parameters, and generic type arguments.

```php
<?php
packed class Vec2 { public float $x; public float $y; }

final class Points {
    public buffer<Vec2> $data;
    public buffer<int> $ids;

    public function __construct(int $n) {
        $this->data = buffer_new<Vec2>($n);
        $this->ids = buffer_new<int>($n);
    }

    public function moveX(int $i, float $dx): void {
        $this->data[$i]->x = $this->data[$i]->x + $dx;
    }

    public function setId(int $i, int $id): void {
        $this->ids[$i] = $id;
    }

    public function all(): buffer<Vec2> {
        return $this->data;
    }
}

final class Store<T> {
    public function __construct(public buffer<T> $items) {}
}

$store = new Store<Vec2>(buffer_new<Vec2>(16));
```

Packed element fields and whole scalar elements can be written through a
buffer-typed property, as in `moveX()` and `setId()` above. A static property
cannot hold a buffer yet.

## Arrays of buffers

Indexed arrays and hashes can hold buffer handles. Literals, appends, index and key writes,
`array_push()`, `array_pop()`, `array_shift()`, `foreach` and variadic parameters keep each
element as the opaque handle, so nothing is copied or boxed:

```php
<?php
function total(buffer<i32> ...$streams): int {
    $sum = 0;
    foreach ($streams as $stream) {
        for ($i = 0; $i < buffer_len($stream); $i++) {
            $sum += $stream[$i];
        }
    }
    return $sum;
}

buffer<i32> $a = buffer_new<i32>(4);
buffer<i32> $b = buffer_new<i32>(2);
$a[0] = 5;
$b[1] = 7;
$streams = [$a, $b];
$streams[] = buffer_new<i32>(8);
echo total(...$streams);            // 12
$last = array_pop($streams);
echo $streams[1][1];                // 7: an element read gives the buffer back
$byName = ["a" => $a, "b" => $b];
echo $byName["b"][1];               // 7
```

A ternary, `?:`, `??` or `match` whose arms are arrays of the same buffer type keeps the
handles as words, whether the arms are literals, locals or an empty literal. An element read
gives the buffer back, so `$streams[1][1]` reads element 1 of the second buffer; a write through such a read (`$streams[1][1] = 3`) is rejected at compile time, so
assign the element to a variable first. Every element must have the same buffer type: a
`buffer<int>` beside a `buffer<i32>` widens the array to `mixed`, which cannot hold a buffer.
A missing element or key, and `array_pop()` or `array_shift()` of an empty array, read as the
invalid handle 0: `??` takes its default and `empty()` answers true for it, like a missing
element of any other array, while an element read or write, `buffer_len()` or `buffer_ptr()` on
it aborts with the use-after-free error below (`buffer_free()` of it is the same no-op as
freeing an already freed local). Freeing a buffer does not touch the arrays that hold its
handle; their elements become stale aliases. A `mixed` view of such an array (`print_r()`,
`var_dump()`, `json_encode()`, an `array<mixed>` parameter) shows each handle as an integer.

## Buffer length

```php
<?php
echo buffer_len($data);   // 512
```

## Payload pointer

`buffer_ptr($buf)` returns the address of the buffer's payload, the memory `$buf[0]` lives
in, so a buffer can be handed to C or read with the [pointer builtins](pointers.md):

```php
<?php
extern function memset(ptr $dest, i32 $byte, u64 $count): ptr;

buffer<u8> $pixels = buffer_new<u8>(640 * 480);
memset(buffer_ptr($pixels), 255, buffer_len($pixels));   // C fills the payload in place
echo $pixels[1000];                                       // 255

buffer<u16> $indices = buffer_new<u16>(6);
$indices[4] = 2;
echo ptr_read16(ptr_offset(buffer_ptr($indices), 2 * 4)); // 2, the element at its 2-byte stride

packed class Vec2 { public f32 $x; public f32 $y; }
buffer<Vec2> $quad = buffer_new<Vec2>(4);
$first = buffer_ptr($quad);                               // ptr<Vec2>, the same address as $quad[0]
$first->x = 1.5;
echo $quad[0]->x;                                         // 1.5
```

The pointer is the payload itself, not a copy: writes made through it, by PHP or by C, are
visible through `$buf[$i]`, and element `$i` sits at `buffer_ptr($buf) + $i * stride` with the
strides of the [Element widths](#element-widths) table. For a buffer of packed records the
result is a `ptr<T>`, as `$buf[0]` is; for every other element type it is the untyped `ptr`.
The address stays valid until `buffer_free()`: `buffer_ptr()` of a freed buffer, or of an alias
of one, aborts with the use-after-free error below, and a pointer taken earlier dangles once
the payload is released. Unlike `buffer_free()`, `buffer_ptr()` accepts any buffer expression,
including a property or a call result. Inside `eval()`, where a buffer is a raw allocation
rather than a registry handle, no buffer operation has that diagnostic: `buffer_free()` releases
the memory and nulls its own variable, and another copy of the handle dangles for
`buffer_ptr()` exactly as it does for `buffer_len()` and the element reads.

## Freeing buffers

```php
<?php
buffer_free($buf);   // release heap memory, nullify variable
```

Use-after-free produces: `Fatal error: use of buffer after buffer_free()`

Restrictions:

- Only accepts plain local variables
- Freeing a local nullifies that local; freeing the same local again is a no-op
- Existing aliases become stale and fail with the use-after-free fatal error, even if the descriptor slot and heap payload are later reused

## Bounds checking

Always enabled. Out-of-bounds aborts: `Fatal error: buffer index out of bounds`

## Length validation

`buffer_new<T>()` validates the requested length before allocating. A negative length, or a length
whose `length * stride` payload size does not fit in a machine word, aborts with:

```
Fatal error: buffer_new() length is negative or exceeds the maximum buffer size
```

This keeps the length recorded in the buffer header consistent with the memory the buffer actually
owns, so the bounds check above can never approve an index outside the allocation. A length that is
representable but larger than the configured heap still reports `Fatal error: heap memory exhausted`.

## Memory layout

The PHP-visible value is an opaque 64-bit handle, not a heap pointer:

```
Bits 63..32: generation (non-zero u32)
Bits 31..0:  descriptor index (1..=4096)
```

The handle resolves through a static descriptor registry. Each 48-byte descriptor contains:

```
Offset 0:   [payload pointer: 8 bytes]
Offset 8:   [length: 8 bytes]
Offset 16:  [stride: 8 bytes]
Offset 24:  [generation: 8-byte slot, low u32 used]
Offset 32:  [active marker: 8 bytes]
Offset 40:  [free-list successor: 8 bytes]
```

The payload is a separate heap allocation containing exactly `length * stride` zero-initialized bytes, where the stride is the element size: 8 for the word types, the width of a `u8`, `i16`, `i32` or `f32` element, or the padded size of a packed record. Reusing a descriptor increments its generation, so a stale alias cannot resolve to a newer buffer lifetime.

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
- At most 4096 buffers can be live simultaneously; descriptor slots are recycled after `buffer_free()`
