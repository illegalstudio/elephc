---
title: "Pointers"
description: "Low-level memory access with ptr(), ptr_get(), ptr_set(), raw byte/string helpers, pointer arithmetic, and typed casting."
sidebar:
  order: 1
---

Pointers provide low-level memory access. A pointer is a 64-bit memory address stored in a single register.

> **Strict mode:** pointers are an elephc extension with no PHP equivalent. Compiling with [`--strict-php`](../compiling/cli-reference.md#strict-php-mode) rejects the `ptr` type, `ptr_cast<T>`, and every `ptr_*` builtin.

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

`ptr_cast<T>()` is compile-time only. When `T` is a packed class, fields are read and
written through the typed pointer (`$typed->field`, including nested records); see
[Packed Classes](packed-classes.md#typed-pointers). When `T` is an extern class (a C
struct), each field is read and written at its natural C offset with its declared width;
see [Extern classes](extern.md#extern-classes-c-structs).

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
ptr_write16($p, 0x1234);     // write 2 bytes, little-endian
echo ptr_read16($p);          // read 2 bytes, zero-extended
ptr_write32($p, 12345);      // write 4 bytes
echo ptr_read32($p);          // read 4 bytes
ptr_write_f32($p, 1.5);      // write an IEEE 754 single (4 bytes)
echo ptr_read_f32($p);        // read it back as a PHP float
ptr_write_f64($p, 0.1);      // write an IEEE 754 double (8 bytes)
echo ptr_read_f64($p);        // 0.1
```

`ptr_write_f32()` rounds the PHP float to single precision, so
`ptr_read_f32()` returns `0.10000000149011612` for a stored `0.1`; the 64-bit
pair keeps the value exactly. Both accept an `int` value and convert it.

## Null safety

Pointer dereference helpers abort on null pointer dereference. Use `ptr_is_null($p)` to check.

A nullable pointer type (`?ptr`, `?ptr<T>` or `ptr<T>|null`) is stored as the bare address,
and PHP `null` is address 0. Returning `null`, assigning it, or a `= null` property default
writes a null pointer, and `=== null`, `!== null`, `is_null()`, `isset()` and `??` test the
address against 0:

```php
<?php
packed class Vec2 { public int $x; public int $y; }

function find(buffer<Vec2> $items, int $x): ?ptr<Vec2> {
    for ($i = 0; $i < buffer_len($items); $i++) {
        if ($items[$i]->x === $x) {
            return $items[$i];
        }
    }
    return null;
}

$items = buffer_new<Vec2>(4);
$hit = find($items, 0);
if ($hit !== null) {
    echo $hit->y;             // field access through the found record
}
echo gettype(find($items, 9)); // "NULL"
```

Because the two are the same value, a `ptr_null()` stored in a `?ptr<T>` reads as `null`:
`echo`, `(string)`, concatenation, interpolation and `sprintf("%s")` give the empty string,
`gettype()` answers `NULL`, and boxing it into a `mixed` value stores PHP `null`. Builtins
that take `mixed` (`gettype`, `is_int`, `json_encode`, `var_dump`, `serialize`) and the
`(int)`/`(float)` casts see a live `?ptr<T>` as its integer address, so `gettype()` answers
`integer` for it. A
non-nullable `ptr` keeps printing `0x0`. Narrow a `?ptr<T>` with `!== null` before passing
it to an `extern` function that takes `ptr<T>`. Object introspection (`var_dump`, `print_r`,
`var_export`, `json_encode`, `serialize`) shows a pointer property as its integer address,
so a null `?ptr<T>` property appears there as `0`.

Instance and static properties can be typed `ptr`, `ptr<T>` or `?ptr<T>`. A non-nullable
`ptr` or `ptr<T>` property may still take a `= null` default, which starts it as the null
pointer; without a default, a typed static pointer property is uninitialized until its first
assignment, like any typed static property:

```php
<?php
packed class Vec2 { public int $x; public int $y; }

final class Cursor {
    public static ptr<Vec2> $at = null;
}

$items = buffer_new<Vec2>(2);
if (ptr_is_null(Cursor::$at)) {
    Cursor::$at = $items[1];      // the record address
}
Cursor::$at->x = 7;               // writes through to $items[1]
echo $items[1]->x;                // 7
```

A `?ptr<T> &$p` (or `ptr<T> &$p`) by-reference parameter writes the pointer word back
through the caller's variable, so that variable must hold a pointer word everywhere in its
scope: a pointer value (`$p = $buffer[0];`), a declared `?ptr<T> $p = null;`, or a pointer
parameter. A variable that holds only `null`, or `null` and a pointer at different points, is
rejected at compile time (declare it `?ptr<T> $p = null;` instead), and so is an array
element. A `?ptr<T>` variable that a `=== null` guard narrowed can still be passed, and reads
as `?ptr<T>` again after the call. A plain `ptr<T>` variable stays a plain pointer after the
call: if the callee stored `null`, it is address 0, not PHP `null`.

## Arrays of pointers

Indexed arrays and hashes can hold pointers: `ptr`, `ptr<T>`, packed record addresses such
as `$buffer[$i]`, and `null`. Each element is the raw pointer word, a `null` element is the
null pointer, and the array keeps pointer storage instead of boxing its elements. That holds
for literals (including spreads such as `[$p, ...$more]` and `["a" => $p, ...$rest]`),
appends, index and key writes, and `array_push()`:

```php
<?php
packed class Vec2 { public int $x; public int $y; }

$points = buffer_new<Vec2>(3);
$picked = [$points[0], $points[2]];   // record addresses
$picked[] = null;                     // stays an array of pointers
foreach ($picked as $p) {
    if ($p !== null) {
        $p->x += 1;                   // writes through to the buffer
    }
}
$byName = ["origin" => $points[0]];
echo $byName["origin"]->x;            // 1
```

Reading an element gives a `?ptr<T>`: a `null` element and a missing key both read as
`null`, so check `!== null` before reaching a field. `array_pop()` and `array_shift()`
return a `?ptr<T>` the same way. Introspection (`var_dump`, `print_r`, `var_export`,
`json_encode`, `serialize`), a `mixed` or `array<mixed>` view of the array, and builtins
that read its elements at run time (`in_array()`, `array_sum()`, `vsprintf()`) see a `null`
element as PHP `null` and a live one as its integer address. An array that reaches a pointer-array
parameter from `call_user_func_array()`, a callable or `eval` (where its elements are ordinary
PHP values) is converted on entry: `null` becomes the null pointer and an integer its address.

Some array builtins (`array_merge()`, `array_slice()`, `array_map()`, `sort()` and similar) do
not accept arrays of pointers yet and report an unsupported-feature error at compile time.

## Raw string copies

```php
<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(64);

$written = ptr_write_string($buf, "HTTP/1.1 200 OK\r\n");
$line = ptr_read_string($buf, $written);

echo $line;
free($buf);
```

`ptr_read_string($p, $len)` copies exactly `$len` bytes into a new PHP string. It does not stop at `\0`, so binary payloads are preserved. `$len < 0` is a fatal error.

`ptr_write_string($p, $str)` copies the raw bytes of `$str` to `$p` without a trailing null terminator and returns the number of bytes written.

## Echo, string conversion and comparison

```php
<?php
echo $p;                      // prints hex: 0x16f502348
echo ptr_null();              // prints: 0x0
echo "p = " . $p;             // "p = 0x16f502348"
echo "p = $p";                // same text through interpolation
$s = (string)$p;              // "0x16f502348", also strval($p) and sprintf("%s", $p)
echo $p === $q ? "same" : "different";
echo gettype($p);             // "pointer"
```

A pointer converts to a string as `0x` followed by its address in lowercase
hexadecimal without leading zeros (`0x0` for a null pointer), on every target.
`echo`, `print`, concatenation, `.=`, interpolation in double-quoted strings and
heredocs, `(string)` casts, `strval()` and `%s` in `sprintf()`/`printf()` (with a
literal or a runtime format string) all produce that same text, for `ptr` and
`ptr<T>` values alike, including pointers returned by `extern` functions. To
hand the text to another string builtin, pass `(string)$p`.

A pointer is not a string, so it is still rejected where a `string` is required
by declaration: a `string` parameter or return type, `strlen()`, and loose
comparison with `==` (use `===`; a `?ptr<T>` compares with `==` like the PHP value it
stands for, `null` or its integer address). The conversion applies to values the compiler
knows to be pointers: a pointer stored in a `mixed` or union-typed value, for
example `$v = $flag ? $p : "none"`, is held as its integer address there and
prints in decimal (a null `ptr` is the integer `0` there, not PHP `null`; a null
`?ptr<T>` is PHP `null`). `settype($p, "string")` needs that same mixed storage, so it
does not compile yet for a pointer variable; assign `(string)$p` to another
variable instead. Code running inside `eval()` also sees pointers as plain
integers, so `echo ptr_null()` there prints `0`. A `?ptr<T>` that `eval()` reaches through
a compiled class (a method result or parameter, an instance or static property, a
constructor parameter) keeps its nullable meaning: address 0 is PHP `null` there, and a
`null` passed from `eval()` becomes address 0. A method with a `?ptr<T> &$p` by-reference
parameter cannot be called from `eval()` yet.

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
| `ptr_read16($p)` | `ptr_read16($p): int` | Read 2 little-endian bytes, zero-extended |
| `ptr_write16($p, $v)` | `ptr_write16($p, $v): void` | Write the low 16 bits as 2 little-endian bytes |
| `ptr_read32($p)` | `ptr_read32($p): int` | Read 4 bytes |
| `ptr_write32($p, $v)` | `ptr_write32($p, $v): void` | Write 4 bytes |
| `ptr_read_f32($p)` | `ptr_read_f32($p): float` | Read a 4-byte IEEE 754 single, widened to a PHP float |
| `ptr_write_f32($p, $v)` | `ptr_write_f32($p, $v): void` | Round a float (or int) to a single and write 4 bytes |
| `ptr_read_f64($p)` | `ptr_read_f64($p): float` | Read an 8-byte IEEE 754 double |
| `ptr_write_f64($p, $v)` | `ptr_write_f64($p, $v): void` | Write a float (or int) as an 8-byte double |
| `ptr_read_string($p, $len)` | `ptr_read_string($p, $len): string` | Copy exactly `$len` bytes into a new PHP string |
| `ptr_write_string($p, $str)` | `ptr_write_string($p, $str): int` | Copy string bytes into raw memory and return bytes written |

Notes:

- `ptr()` only accepts variables. `ptr(1 + 2)` is a compile-time error.
- `ptr_set()` writes a single 8-byte word (int, bool, null, or pointer); floats go through `ptr_write_f64()` or `ptr_write_f32()`.
- `ptr_cast<T>()` accepts builtin types, classes, packed classes, and extern classes.
- Use `===` and `!==` for comparison. `==`/`!=` is rejected.
