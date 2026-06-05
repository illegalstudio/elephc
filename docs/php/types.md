---
title: "Types"
description: "Data types supported by elephc: int, float, string, bool, array, null, mixed, iterable, resource, callable, object, enum, and extension types."
sidebar:
  order: 1
---

## Data Types


| Type             | Supported        | Notes                                                                                                                  |
| ---------------- | ---------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `int`            | Yes              | 64-bit signed integer. Literals: decimal `42` / `1_000_000`, hexadecimal `0xFF` / `0xFF_FF`, legacy octal `0755` / `0_755`, explicit octal `0o755` / `0O755` (PHP 8.1+), binary `0b1010` / `0B1010` (PHP 5.4+). Numeric separators `_` allowed between digits in any base (PHP 7.4+). |
| `string`         | Yes              | Pointer + length pair, double and single quoted                                                                        |
| `null`           | Yes              | Sentinel value, coerces to `0`/`""` in operations                                                                      |
| `bool`           | Yes              | `true`/`false` as distinct type. `echo false` prints nothing, `echo true` prints `1`. Coerces to 0/1 in arithmetic.    |
| `float`          | Yes              | 64-bit double-precision. Literals: `3.14`, `.5`, `1.5e3`, `1.0e-5`, `1_000.5`, `1e1_0`. Constants: `INF`, `NAN`.       |
| `array`          | Yes              | Indexed (`[1, 2, 3]`) and associative (`["key" => "value"]`). Heterogeneous indexed and associative payloads widen to boxed `mixed`. Arrays use copy-on-write semantics. |
| `mixed`          | Yes              | Supported in type hints and typed locals. Runtime values are boxed with a per-value tag, including heterogeneous array payloads and union storage. |
| `iterable`       | Yes              | PHP pseudo-type for `array \| Traversable`. Supports indexed arrays, associative arrays, `Iterator`, and `IteratorAggregate`; runtime operations (`foreach`, `echo`, `gettype()`, `var_dump()`, `===`, casts, `is_iterable()`) dispatch on heap-kind, value-type, or interface metadata as needed. |
| `resource`       | Inferred only    | File handles and standard streams are modeled separately from integers. `fopen()` returns `resource\|false`, and `STDIN`, `STDOUT`, and `STDERR` are stream resources. PHP does not allow `resource` as a type declaration, so elephc does not accept `resource` annotations. |
| `callable`       | Yes              | Closures, arrow functions, first-class callables, and FFI callback parameters.                                         |
| `object`         | Yes              | Class instances. Heap-allocated, fixed-layout. `new ClassName(...)`                                                    |
| `enum`           | Yes              | Pure and backed enums. Cases are singletons. Backed enums support `->value`, `::from()`, `::tryFrom()`, `::cases()`.   |
| `int|string`     | Yes              | Union type — variable accepts any of the listed types. Lowered to Mixed at runtime.                                    |
| `?int`           | Yes              | Nullable shorthand — sugar for `int|null`.                                                                             |
| `void`           | Return only      | Valid as a function, method, closure, arrow, or extern return type. Internally, `null` is represented as `Void`.        |
| `never`          | Return only      | Marks a function, method, closure, or interface method that **never returns** — it must always `throw`, call `exit()`/`die()`, or loop forever. Returning is rejected at type-check time. |
| `ptr` / `ptr<T>` | elephc extension | Raw 64-bit pointer, optionally carrying a checked compile-time pointee tag. See [Pointers](../beyond-php/pointers.md). |
| `buffer<T>`      | elephc extension | Fixed-size contiguous storage for POD scalars, pointers, or packed records. See [Buffers](../beyond-php/buffers.md).   |
| `packed class`   | elephc extension | Flat POD record type with compile-time field offsets. See [Packed Classes](../beyond-php/packed-classes.md).           |

Integer-form numeric literals keep the `int` type only while they fit in PHP's signed 64-bit range. Larger decimal, hexadecimal, octal, or binary literals are promoted to `float`, matching PHP on 64-bit builds.

### Never

`never` marks a function, method, closure, or interface method that **must not return normally**. The function body is expected to either `throw`, call `exit()`/`die()`, or loop forever.

```php
<?php
function panic(string $msg): never {
    throw new RuntimeException($msg);
}

class Failer {
    public function fail(): never {
        throw new \Exception("boom");
    }

    public static function bail(int $code): never {
        exit($code);
    }
}

interface Aborts {
    public function abort(): never;
}
```

Rules:

- valid as a return type for functions, closures, instance methods, static methods, and interface methods
- matched case-insensitively like PHP's built-in type names (`never`, `Never`, and `NEVER` are equivalent)
- must be used as a standalone return type; `?never`, `never|null`, and `int|never` are rejected
- not valid as a parameter, property, or typed local
- declaring `: never` and then writing `return $value;` (or even bare `return;`) is rejected at type-check time
- `: never` is the **bottom type** in the type system: it is a subtype of every other type, so a child method may override a parent that returns `void`/`int`/etc. with `: never`
- the reverse is rejected: a parent or interface method declared as `: never` requires the child/implementation to declare a compatible return type
- if execution falls through a `: never` function or method body, elephc emits a runtime fatal error instead of returning to the caller

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
    public static int $count = 0;
}
```

Rules:

- instance and static properties can use declared property types
- property defaults and assignments must be compatible with the declared type
- constructor assignments through untyped parameters are checked once call sites refine the parameter type
- nullable and union property storage is boxed using the same mixed runtime shape as typed locals
- static property redeclarations across inheritance follow PHP-style rules: non-private inherited properties keep invariant declared types, cannot reduce visibility, and cannot override `final` properties
- private inherited static properties can be redeclared as independent subclass slots
- untyped inherited static properties cannot be redeclared with a type, and typed inherited static properties cannot be redeclared without one
- direct element writes to static array properties, such as `ClassName::$items[] = $value` or `ClassName::$items[0] = $value`, require the property to be an `array`
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

Cast names and aliases are case-insensitive, matching PHP. For example,
`(INT)`, `(Integer)`, and `(integer)` are equivalent.

Aliases: `(integer)`, `(double)`, `(real)`, `(boolean)`.

### Type functions


| Function        | Signature                    | Description                    |
| --------------- | ---------------------------- | ------------------------------ |
| `is_null()`     | `is_null($val): bool`        | Returns true if null           |
| `is_float()`    | `is_float($val): bool`       | Returns true if float          |
| `is_int()`      | `is_int($val): bool`         | Returns true if integer        |
| `is_string()`   | `is_string($val): bool`      | Returns true if string         |
| `is_numeric()`  | `is_numeric($val): bool`     | Returns true if int or float   |
| `is_bool()`     | `is_bool($val): bool`        | Returns true if bool           |
| `is_iterable()` | `is_iterable($val): bool`    | Returns true if array or Traversable-compatible iterable |
| `is_callable()` | `is_callable($val): bool`    | Returns true for closures, first-class callables, strings case-insensitively naming known builtins, user functions, or public static methods (`"Class::method"`), `[$obj, "method"]` arrays with public methods, `[ClassName::class, "method"]` static method arrays, and objects with public `__invoke()`. |
| `is_resource()` | `is_resource($val): bool`    | Returns true if value is an open resource handle |
| `is_nan()`      | `is_nan($val): bool`         | Returns true if NAN            |
| `is_finite()`   | `is_finite($val): bool`      | Returns true if not INF/NAN    |
| `is_infinite()` | `is_infinite($val): bool`    | Returns true if INF or -INF    |
| `boolval()`     | `boolval($val): bool`        | Convert to bool                |
| `floatval()`    | `floatval($val): float`      | Convert to float               |
| `intval()`      | `intval($val): int`          | Converts to integer            |
| `gettype()`     | `gettype($val): string`      | Returns type name              |
| `empty()`       | `empty($val): bool`          | Returns true if value is falsy |
| `unset()`       | `unset($var, ...$vars): void` | Sets one or more variables to null |
| `settype()`     | `settype($var, $type): bool` | Changes variable type in place |


### Known incompatibilities with PHP

- `$argv[0]` returns the compiled binary path, not the `.php` file path.
- Integer `+`, `-`, and `*` overflow promotes to `double` for both constant-folded and non-folded runtime scalar arithmetic.
- Scalar loose comparison (`==`, `!=`) follows PHP-style bool truthiness, null-vs-empty-string, numeric-string, and non-numeric string byte-comparison rules for constant-folded literals and non-folded runtime scalar operands.
- `??=` is checked against typed assignment storage for variables, object properties, static properties, and non-append array elements. For concrete local variable types, the fallback must keep the same type or be a literal `null`.
- Plain array numeric casts (`(int)$array`, `(float)$array`) follow elephc's existing array cast semantics (return the element count rather than PHP's `0`/`1`). Direct `iterable` numeric casts use PHP's empty/non-empty `0`/`1` semantics.
- `__destruct` runs when an object's refcount reaches zero (scope exit, reassignment, `unset`, program end), matching PHP's timing, but **object resurrection is not supported**: re-storing `$this` so the object would outlive the destructor does not keep it alive — the object is still freed once `__destruct` returns.
### Filesystem functions not implemented

These standard PHP filesystem functions are intentionally absent from elephc because they have no meaningful semantics in a compiled native binary:

- `move_uploaded_file()`, `is_uploaded_file()` — both rely on the PHP-FPM/SAPI request lifecycle (the `$_FILES` superglobal and a per-request "uploaded files" registry). A standalone compiled binary has no such request scope.
- `realpath_cache_get()`, `realpath_cache_size()` — expose a per-request realpath cache that elephc does not maintain. `clearstatcache()` is provided as a no-op for source-level compatibility.
- `fgetss()` — deprecated in PHP 7.3 and removed in PHP 8.0. New code should use `strip_tags()` on the result of `fgets()`.

### Compiler diagnostics

elephc reports errors with source spans. Example:

```text
error[3:5]: Undefined variable: $name
error[8:1]: Function 'foo' declared return type string but returns int
```

The compiler also emits non-fatal warnings (unused variables, unreachable code).

### Runtime diagnostics

Runtime warnings flow through a suppressible diagnostics channel. The `@` operator hides those warnings for its operand only, while fatal runtime errors and compile-time diagnostics remain visible. Current suppressible warnings include `fopen()` / `file_get_contents()` open failures and duplicate `define()` calls.
