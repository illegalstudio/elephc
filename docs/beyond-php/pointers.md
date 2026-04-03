---
title: "Pointers"
description: "Low-level memory access with ptr(), ptr_get(), ptr_set(), pointer arithmetic, and typed casting."
sidebar:
  order: 1
---

Pointers provide low-level memory access. A pointer is a 64-bit memory address stored in a single register.

## Creating pointers

```php
<?php
$x = 42;
$p = ptr($x);        // take the address of a stack variable
$null = ptr_null();   // create a null pointer (0x0)
```

## Reading and writing

```php
<?php
$x = 10;
$p = ptr($x);
echo ptr_get($p);     // 10
ptr_set($p, 99);
echo $x;              // 99
```

## Pointer arithmetic

```php
<?php
$p = ptr($x);
$q = ptr_offset($p, 8);   // advance by 8 bytes
```

`ptr_offset` adds a **byte** offset.

## Typed pointers and casting

```php
<?php
$typed = ptr_cast<MyClass>($p);   // change type tag, same address
```

`ptr_cast<T>()` is compile-time only.

## Querying sizes

```php
<?php
echo ptr_sizeof("int");      // 8
echo ptr_sizeof("string");   // 16
echo ptr_sizeof("Point");    // computed from class
```

## Sub-word access

```php
<?php
ptr_write8($p, 0x41);        // write 1 byte
echo ptr_read8($p);           // read 1 byte
ptr_write32($p, 12345);      // write 4 bytes
echo ptr_read32($p);          // read 4 bytes
```

## Null safety

`ptr_get` and `ptr_set` abort on null pointer dereference. Use `ptr_is_null($p)` to check.

## Echo and comparison

```php
<?php
echo $p;                      // prints hex: 0x16f502348
echo ptr_null();              // prints: 0x0
echo $p === $q ? "same" : "different";
echo gettype($p);             // "pointer"
```

## Function reference

| Function | Signature | Description |
|---|---|---|
| `ptr($var)` | `ptr($var): pointer` | Take address of a variable |
| `ptr_null()` | `ptr_null(): pointer` | Create a null pointer |
| `ptr_is_null($p)` | `ptr_is_null($p): bool` | Check if pointer is null |
| `ptr_get($p)` | `ptr_get($p): int` | Read 8 bytes at address |
| `ptr_set($p, $val)` | `ptr_set($p, $val): void` | Write 8 bytes at address |
| `ptr_offset($p, $n)` | `ptr_offset($p, $n): pointer` | Add byte offset |
| `ptr_cast<T>($p)` | `ptr_cast<T>($p): pointer` | Change type tag |
| `ptr_sizeof("type")` | `ptr_sizeof($t): int` | Get byte size of a type |
| `ptr_read8($p)` | `ptr_read8($p): int` | Read 1 byte |
| `ptr_write8($p, $v)` | `ptr_write8($p, $v): void` | Write 1 byte |
| `ptr_read32($p)` | `ptr_read32($p): int` | Read 4 bytes |
| `ptr_write32($p, $v)` | `ptr_write32($p, $v): void` | Write 4 bytes |

:::note
- `ptr()` only accepts variables. `ptr(1 + 2)` is a compile-time error.
- `ptr_set()` writes a single 8-byte word (int, bool, null, or pointer).
- `ptr_cast<T>()` accepts builtin types, classes, packed classes, and extern classes.
- Use `===` and `!==` for comparison. `==`/`!=` is rejected.
:::
