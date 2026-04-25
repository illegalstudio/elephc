---
title: "Types"
description: "Data types supported by elephc: int, float, string, bool, array, null, mixed, callable, object, enum, and extension types."
sidebar:
  order: 1
---

## Data Types

| Type | Supported | Notes |
|---|---|---|
| `int` | Yes | 64-bit signed integer |
| `string` | Yes | Pointer + length pair, double and single quoted |
| `null` | Yes | Sentinel value, coerces to `0`/`""` in operations |
| `bool` | Yes | `true`/`false` as distinct type. `echo false` prints nothing, `echo true` prints `1`. Coerces to 0/1 in arithmetic. |
| `float` | Yes | 64-bit double-precision. Literals: `3.14`, `.5`, `1.5e3`, `1.0e-5`. Constants: `INF`, `NAN`. |
| `array` | Yes | Indexed (`[1, 2, 3]`) and associative (`["key" => "value"]`). Arrays use copy-on-write semantics. |
| `mixed` | Yes | Supported in type hints and typed locals. Runtime values are boxed with a per-value tag. |
| `callable` | Yes | Closures, arrow functions, first-class callables, and FFI callback parameters. |
| `object` | Yes | Class instances. Heap-allocated, fixed-layout. `new ClassName(...)` |
| `enum` | Yes | Pure and backed enums. Cases are singletons. Backed enums support `->value`, `::from()`, `::tryFrom()`, `::cases()`. |
| `int\|string` | Yes | Union type — variable accepts any of the listed types. Lowered to Mixed at runtime. |
| `?int` | Yes | Nullable shorthand — sugar for `int\|null`. |
| `void` | Return only | Valid as a function, method, closure, or extern return type. Internally, `null` is represented as `Void`. |
| `ptr` / `ptr<T>` | elephc extension | Raw 64-bit pointer, optionally carrying a checked compile-time pointee tag. See [Pointers](../beyond-php/pointers.md). |
| `buffer<T>` | elephc extension | Fixed-size contiguous storage for POD scalars, pointers, or packed records. See [Buffers](../beyond-php/buffers.md). |
| `packed class` | elephc extension | Flat POD record type with compile-time field offsets. See [Packed Classes](../beyond-php/packed-classes.md). |
| `resource` | No | File handles are modeled as integer file descriptors (`int`). |

### Typed local declarations

```php
<?php
int|string $value = 1;
?int $maybe = null;
```

Rules:
- union types are supported in typed local declarations, for example `int|string`
- nullable shorthand `?T` is supported as sugar for `T|null`
- at runtime these values are lowered to the compiler's boxed tagged representation
- `?T|U` is not accepted; write `T|U|null` explicitly instead

### Property type declarations

```php
<?php
class User {
    public int $id;
    public string $name = "Ada";
    public ?string $email = null;
}
```

Rules:
- property defaults and assignments must be compatible with the declared type
- constructor assignments through untyped parameters are checked once call sites refine the parameter type
- nullable and union property storage is boxed using the same mixed runtime shape as typed locals
- `void` and `callable` are not valid property types

### Null behavior

```php
<?php
$x = null;
echo $x;              // prints nothing
echo is_null($x);     // prints 1
echo $x + 5;          // prints 5 (null → 0)
echo $x . "hello";    // prints "hello" (null → "")
$x = 42;              // reassignment from null works
```

### Type Casting

```php
$i = (int)3.7;       // 3
$f = (float)42;      // 42.0
$s = (string)42;     // "42"
$b = (bool)0;        // false
$a = (array)42;      // [42]
```

Aliases: `(integer)`, `(double)`, `(real)`, `(boolean)`.

### Type functions

| Function | Signature | Description |
|---|---|---|
| `is_null()` | `is_null($val): bool` | Returns true if null |
| `is_float()` | `is_float($val): bool` | Returns true if float |
| `is_int()` | `is_int($val): bool` | Returns true if integer |
| `is_string()` | `is_string($val): bool` | Returns true if string |
| `is_numeric()` | `is_numeric($val): bool` | Returns true if int or float |
| `is_bool()` | `is_bool($val): bool` | Returns true if bool |
| `is_nan()` | `is_nan($val): bool` | Returns true if NAN |
| `is_finite()` | `is_finite($val): bool` | Returns true if not INF/NAN |
| `is_infinite()` | `is_infinite($val): bool` | Returns true if INF or -INF |
| `boolval()` | `boolval($val): bool` | Convert to bool |
| `floatval()` | `floatval($val): float` | Convert to float |
| `intval()` | `intval($val): int` | Converts to integer |
| `gettype()` | `gettype($val): string` | Returns type name |
| `empty()` | `empty($val): bool` | Returns true if value is falsy |
| `unset()` | `unset($var): void` | Sets variable to null |
| `settype()` | `settype($var, $type): bool` | Changes variable type in place |

### Known incompatibilities with PHP

- `$argv[0]` returns the compiled binary path, not the `.php` file path.
- `strpos()` returns `-1` when not found, not `false`.
- `array_search()` returns `-1` when an indexed array search misses and `""` when an associative array search misses, not `false`.
- `define()` registers a compile-time constant and its return value is not modeled.
- Integer overflow wraps instead of promoting to float.
- Loose comparison (`==`) between different types coerces both sides to integer.
- elephc does not model PHP's uninitialized typed-property state; property slots without explicit defaults start from the compiler's existing zero/null-like object-slot initialization until assigned.

### Compiler diagnostics

elephc reports errors with source spans. Example:
```text
error[3:5]: Undefined variable: $name
error[8:1]: Function 'foo' declared return type string but returns int
```

The compiler also emits non-fatal warnings (unused variables, unreachable code).
