---
title: "Packed Classes"
description: "Flat POD record types with compile-time field offsets for hot-path storage."
sidebar:
  order: 3
---

A `packed class` is a flat record with compile-time field offsets. Designed for hot-path data that needs predictable layouts and zero hash overhead.

> **Strict mode:** `packed class` is an elephc extension with no PHP equivalent. Compiling with [`--strict-php`](../compiling/cli-reference.md#strict-php-mode) rejects it.

## Declaration
```php
<?php
packed class Enemy {
    int $x;
    int $y;
    int $hp;
    int $state;
}
```

## Constraints
- Fields must be `int`, `float`, `bool`, `ptr`, a C width (`i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f32`, `f64`; see [Field widths](#field-widths)), or another `packed class`
- No union or nullable types
- No strings, arrays, objects, or mixed values
- No methods, constructors, inheritance, traits, or interfaces
- Only `public` visibility (optional and redundant)
- No default values
- Layout is the natural C layout: each `int`, `float`, `bool`, or `ptr` field takes 8 bytes, a width-declared field takes its own size at the next multiple of that size, a nested packed field takes the full size of its record at the record's alignment, and the record is padded to its widest field. A class of 8-byte fields is therefore sequential: in `Body` below `$pos` is at offset 0, `$vel` at 16 and `$id` at 32 (40 bytes in total)

## Usage with buffers
```php
<?php
packed class Vertex {
    float $x;
    float $y;
    float $u;
    float $v;
    int $color;
}

buffer<Vertex> $verts = buffer_new<Vertex>(1024);
$verts[0]->x = 10.0;
echo $verts[0]->x;  // 10
```

A packed class name is a type wherever a type is accepted: a `buffer<Vertex>`
or `ptr<Vertex>` can be a class property, a method parameter or return type, a
promoted constructor parameter, or a generic type argument. See
[buffers in class members](buffers.md#buffers-in-class-members).

## Values known only at run time

A field takes a value of its own type (a `bool` field also takes `false`). Some values only
have a type at run time: integer arithmetic, which can overflow to `float`, float arithmetic
over such a result (`1.5 * ($i + 1)` with a loop counter), a `mixed` parameter or a
`json_decode()` result. A scalar field accepts them and checks the value as it is stored:

- an `int` field (every integer width) takes an int;
- a `float` or `f32` field takes a float, or an int converted to the nearest float;
- a `bool` field takes a bool.

Any other value throws a catchable `TypeError` and the field keeps its previous value; it is
never coerced the way a PHP cast would:

```php
<?php
packed class Vec2 { public float $x; public float $y; }
function load(mixed $v) { return $v; }

$pts = buffer_new<Vec2>(3);
for ($i = 0; $i < 3; $i++) {
    $pts[$i]->x = 1.5 * ($i + 1);   // 1.5, 3, 4.5
}
$pts[0]->y = load(2);               // the int 2 is stored as 2.0
try {
    $pts[0]->y = load("2.5");
} catch (TypeError $e) {
    echo $e->getMessage();          // Packed field Vec2::$y must be of type float, string given
}
```

A `ptr` field takes only a pointer-typed value: a pointer and an integer look the same once
they are `mixed`, so `mixed` is a compile error there.

## Field widths

A field declared with one of the C width names keeps that width in memory, so a packed class
can mirror a C struct byte for byte and a buffer of records carries only the bytes the data
needs:

| Field type | C type | Size and alignment | Reads as |
|---|---|---|---|
| `int` / `i64`, `u64` | `int64_t`, `uint64_t` | 8 | `int` |
| `i32`, `u32` | `int32_t`, `uint32_t` | 4 | `int` |
| `i16`, `u16` | `int16_t`, `uint16_t` | 2 | `int` |
| `i8`, `u8` | `int8_t`, `uint8_t` | 1 | `int` |
| `float` / `f64` | `double` | 8 | `float` |
| `f32` | `float` | 4 | `float` |
| `bool` | | 8 | `bool` |
| `ptr`, `ptr<T>` | `void*`, `T*` | 8 | pointer |

The widths are a storage format, not new types: a field of any integer width reads as `int`
and an `f32` reads as `float`, and `i32 + int` is plain integer arithmetic. A write keeps the
declared width with the rules [extern class fields](extern.md#extern-classes-c-structs) have:
signed widths sign-extend and unsigned widths zero-extend on a read, an out-of-range integer
keeps its low bits like a C conversion (`300` in a `u8` is `44`, `-1` in a `u32` is
`4294967295`), and a float written to an `f32` field rounds to single precision. Integer
arithmetic results and compound assignments (`$v->count = $i * 2`, `$v->flags++`) narrow the
same way. A packed `bool` keeps its 8-byte slot; declare a one-byte C flag as `u8`.

```php
<?php
// SDL2's SDL_Vertex: SDL_FPoint position, SDL_Color color, SDL_FPoint tex_coord.
packed class Vertex {
    public f32 $x;
    public f32 $y;
    public u8 $r;
    public u8 $g;
    public u8 $b;
    public u8 $a;
    public f32 $u;
    public f32 $v;
}

$verts = buffer_new<Vertex>(4);   // 20 bytes per element, the C sizeof
$verts[0]->x = 0.1;               // stored as the nearest single: 0.10000000149012
$verts[0]->g = 300;               // stored as 44
echo ptr_sizeof("Vertex");        // 20
```

Fields are laid out like a C compiler does on every supported target: each field starts at
the next multiple of its own size, a nested record at its own alignment, and the record is
padded to its widest field (`Vertex` above is 20 bytes; a class of one `u8` is 1 byte). Declare
the fields in the header's order with the header's widths and a `ptr<Vertex>` cast over
memory a C library owns (`ptr_cast<Vertex>(...)`, see [Typed pointers](#typed-pointers))
reads and writes the C struct in place. Scalar buffers take the same width names:
`buffer<u16>` index data, `buffer<u8>` pixels, `buffer<f32>` vertex streams; see
[Buffers](buffers.md#element-widths).

The width names are not reserved words: in a packed field type or inside `buffer<...>` they
name the width, and everywhere else they resolve like class names. A program that declares
or imports a class under one of these names (`class u8`, `use Lib\Byte as i32`) shadows the
width wherever that class is visible, exactly as PHP resolves the name. A misspelled width
(`I32`, `uint8_t`, `double`) is reported with the accepted spellings.

## Nested records

A field whose type is another packed class embeds that record in place. Chained access
resolves the inner field from the summed compile-time offsets, for reads, writes, and
compound assignments, at any depth:

```php
<?php
packed class Vec2 { public float $x; public float $y; }
packed class Body { public Vec2 $pos; public Vec2 $vel; public int $id; }

$bodies = buffer_new<Body>(2);
$bodies[0]->pos->x = 1.25;
$bodies[0]->vel->y += 0.5;
$bodies[1]->id++;
echo $bodies[0]->pos->x;   // 1.25
echo ptr_sizeof("Body");   // 40
```

The embedded class may be declared before or after the class that embeds it. A packed class
cannot embed itself, directly or through other packed classes, because the record would have
no finite size.

Reading a nested record (`$bodies[0]->pos`) yields the record's address, like reading a
buffer element: a local holding it writes through to the buffer. A local that also holds
`null` at some point (`$hit = null;` before a loop that stores `$hit = $bodies[$i];`) keeps
that address as one pointer word with `null` as address 0, so it compares with `null` and
reaches the fields like a `?ptr<T>` (see [Pointers](pointers.md#null-safety)). Assigning a
whole record to a nested field copies its bytes (including the odd tail of a record of
width-declared fields), and the source must be a record of the same packed class: a buffer
element, another nested field, or a `ptr<T>` typed pointer. Like C struct assignment, the copy
assumes the source and the destination do not partially overlap.

```php
<?php
$bodies[1]->pos = $bodies[0]->pos;   // copies 16 bytes
$bodies[1]->pos = 5;                 // compile error: Body::pos takes a Vec2 record
```

A whole buffer element is still assigned field by field (`$bodies[0] = ...` is rejected).

## Typed pointers

A `ptr<T>` whose target is a packed class reaches the same fields, which maps a packed
record onto memory a C library owns:

```php
<?php
extern function calloc(u64 $count, u64 $size): ptr;

$body = ptr_cast<Body>(calloc(1, ptr_sizeof("Body")));
$body->pos->y = 4.5;
$body->id = 7;
```

Field access through a null `ptr<T>` aborts with `Fatal error: null pointer dereference`,
like the other pointer dereference helpers.

## Best practices
- Use packed classes for structured hot-path data instead of associative arrays
- Prefer SoA (parallel buffers) for large datasets where you iterate one field at a time
- Use AoS (buffer of packed class) when you always access all fields together
