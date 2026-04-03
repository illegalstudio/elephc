---
title: "Packed Classes"
description: "Flat POD record types with compile-time field offsets for hot-path storage."
sidebar:
  order: 3
---

A `packed class` is a flat record with compile-time field offsets. Designed for hot-path data that needs predictable layouts and zero hash overhead.

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
- Fields must be `int`, `float`, `bool`, `ptr`, or another `packed class`
- No union or nullable types
- No strings, arrays, objects, or mixed values
- No methods, constructors, inheritance, traits, or interfaces
- Only `public` visibility (optional and redundant)
- No default values
- Layout is sequential: field 0 at offset 0, field 1 at offset 8, etc.

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

## Best practices
- Use packed classes for structured hot-path data instead of associative arrays
- Prefer SoA (parallel buffers) for large datasets where you iterate one field at a time
- Use AoS (buffer of packed class) when you always access all fields together
