---
title: "FFI"
description: "PHP's ext/ffi surface: C declarations, CData/CType values, dynamic calls, memory helpers, and preloaded scopes."
sidebar:
  order: 14.5
---

Elephc implements PHP's `FFI` extension as a Rust bridge (`elephc-ffi`) over a
bundled `libffi`. The bridge is linked only by programs that name the `FFI`
surface, so ordinary binaries carry neither the C declaration parser nor
libffi. `get_declared_classes()` and opaque `eval()` do not link it on their
own: like the PDO, curl, and XML preludes, the FFI classes appear in the symbol
table only when the program actually uses the bridge. Use `--with-ffi` to force
the bridge into a build, including one that reaches `FFI` only through a
computed probe or enumeration, a runtime-provided eval string, or a runtime
include.

```php
<?php
$libc = FFI::cdef("
    int puts(const char *s);
    size_t strlen(const char *s);
", "libc.so.6");

$libc->puts("Hello from C");
var_dump($libc->strlen("elephc"));
```

## Declarations

`FFI::cdef($code, $library = null)` parses a C declaration string at run time.
The parser supports typedefs, structs, unions, enums, bit-fields, arrays,
pointers, function pointers, variadic functions, attributes such as `packed`
and `aligned`, and the common `FFI_LIB` / `FFI_SCOPE` preprocessor directives.
`FFI::load($path)` reads a header from disk, and `FFI::scope($name)` returns a
scope registered by preloading.

On Apple targets a `libc.so.6`/`libm.so.6` library name resolves through the
process image, because those SONAMEs exist only on Linux.

## Values

| API | Description |
|---|---|
| `FFI::new($type, $owned = true, $persistent = false): FFI\CData` | Allocate a C value. A type name or `FFI\CType` selects the layout. |
| `FFI::free($ptr): void` | Free a C allocation and zero a pointer slot. |
| `FFI::cast($type, $ptr): FFI\CData` | Cast a CData value to another C type. |
| `FFI::type($type): FFI\CType` | Resolve a type name in the current context. |
| `FFI::arrayType($type, $dimensions): FFI\CType` | Build an array type. |
| `FFI::typeof($ptr): FFI\CType` | Return a value's C type. |
| `FFI::sizeof($ptr): int` | `sizeof` a C type or value. |
| `FFI::alignof($ptr): int` | `alignof` a C type or value. |
| `FFI::memcpy($to, $from, $size): void` | Copy bytes from a CData value or PHP string. |
| `FFI::memcmp($a, $b, $size): int` | Compare bytes. |
| `FFI::memset($ptr, $value, $size): void` | Fill bytes. |
| `FFI::string($ptr, ?$size = null): string` | Read a NUL-terminated or sized C string. |
| `FFI::isNull($ptr): bool` | Test a null C pointer. |

`FFI\CData` supports `$cdata[$index]`, `count($cdata)`, and `foreach` with the
same observable behavior as php-src's internal handlers. The compiler uses
`ArrayAccess`, `Countable`, and `Iterator` internally for dispatch, but
`class_implements()`, `instanceof`, and reflection report the empty interface
set php-src exposes for CData. Any method call (direct, dynamic, or
non-null nullsafe) raises the catchable
`Error: Object of type FFI\CData does not support method calls` at run time,
before its arguments are evaluated.

C struct and union fields are read and written as properties; nested records and
pointer fields return CData views, while scalar fields return PHP scalars. A
scalar CData's `cdata` property is the PHP scalar payload, not a wrapper.

Only pointer/array CData supports `+` and `-`. Arithmetic on a scalar CData
raises php-src's `TypeError: Unsupported operand types: FFI\CData + int` (and
the equivalent message for the other operators), while pointer plus/minus an
integer performs element arithmetic and pointer difference returns an `int`.
Comparisons follow php-src's internal handler: two C pointers order by address,
a null pointer raises `NULL pointer dereference`, and every other combination
(including scalar CData or an array) raises
`Comparison of incompatible C types`. Truthiness is always true for a CData.

`(int)`/`(float)` casts warn `Object of class FFI\CData could not be converted
to int` (or `to float`) and yield `1`, `(bool)` is true, and `(string)` reads the scalar
payload. Casting a pointer or aggregate to string raises
`Object of class FFI\CData could not be converted to string`. CData is not
`Traversable`: `is_iterable()` is false and `iterator_to_array()` raises its
`Traversable|array` `TypeError`, while `$cdata[$index]`, `count()` and
`foreach` keep working through the internal handlers.

`foreach` over a scalar C array yields PHP ints, floats, strings, bools, or
nulls; aggregate elements stay CData views. A by-reference loop
(`foreach ($array as &$value)`) yields writable CData views instead, so
`$value->cdata += 5` updates the underlying element.

String assignment follows php-src: a one-byte string is accepted for `char`,
any string assigned to a C array (including `char[N]`) or pointer raises
`Incompatible types when assigning to type '...' from PHP 'string'`, and numeric
fields convert through PHP's numeric-string prefix rules. `is_callable()` is
true only when the CData's declared type is a function pointer, including when
the value is boxed in a `mixed` slot; scalar and aggregate CData are not
callable, and a NULL pointer field reads as `null`.

```php
<?php
$point = FFI::new("struct { int x; int y; }");
$point->x = 10;
$point->y = 20;
var_dump($point->x + $point->y); // int(30)

$array = FFI::new("int[3]");
foreach ($array as $index => $value) {
    $array[$index] = $index * 10;
}
```

`FFI\CType` exposes the reflection surface (`getName()`, `getKind()`,
`getSize()`, `getAlignment()`, `getAttributes()`, struct/array/pointer/function
introspection, ABI selectors, and the `TYPE_*` / `ATTR_*` constants).

## Calls and globals

Declared C functions and global variables are available as properties of the
`FFI` context. Calls marshal integers, floats, strings, CData values, null, and
variadic arguments through libffi. A function pointer stored in CData is
callable directly:

```php
<?php
$libc = FFI::cdef("int printf(const char *format, ...);", "libc.so.6");
$printf = $libc->printf;
$printf("value=%d\n", 42);
```

PHP closures assigned to a C function pointer become real libffi closures, so
the C side can call them back: assign through a global (`$ffi->callback = fn`),
a struct field, an array element (`FFI::addr($array)[0] = fn`), or a scalar
function-pointer CData (`$ptr->cdata = fn`). Arguments arrive as PHP scalars,
`const char *` parameters arrive as strings, pointers arrive as `FFI\CData`
views, and aggregates arrive as owned `FFI\CData` copies that remain valid
after the C callback returns; the return value is converted to the declared C
return type. Fixed `int`, `float`, `bool`, `char`, pointer, `void`, and
struct/array-by-value signatures are supported, while variadic function pointers
reject PHP closures with php-src's
`Variadic function closures are not supported`. A callable may also be passed
directly to a C function-pointer parameter (for example a `qsort` comparator):
the call builder prepares the same temporary callback before invoking C.
Writing a non-callable value into a function-pointer slot throws php-src's
`Attempt to assign an invalid callback` exception instead of executing an
arbitrary address. `long double` values can be laid out and sized, but calls and
callbacks that marshal them are rejected rather than silently treating them as
`double`.

A throw from a callback never crosses the C boundary: the program reports
php-src's `Warning: Uncaught ...` trace and then the fatal
`Throwing from FFI callbacks is not allowed`. The `zend_write` global is
available as a replaceable output hook, so an FFI program can wrap `echo` the
same way ext/ffi does on PHP itself.

## Preloading

`ffi.preload` headers are registered as scopes before the program starts;
`FFI::load()` at run time returns an `FFI` context and also registers an
`FFI_SCOPE` when the header declares one. `opcache.preload` files are executed
before the program body.

## Compatibility notes

- Struct and array callback returns by value are supported through libffi.
  Variadic function pointers reject PHP closures with php-src's
  `Variadic function closures are not supported`.
- `FFI::new($type, $owned, $persistent)` marks the allocation persistent: the
  CData destructor does not release it, matching php-src's allocator lifetime.
  An explicit `FFI::free()` still releases it.
- PHP 8.5 and later emit the static `FFI::new()` deprecation; earlier profiles
  accept the spelling silently, matching their runtime.
- `--ini ffi.enable=0` disables every guarded FFI API entry point with php-src's
  `FFI API is restricted by "ffi.enable" configuration directive`. The default
  `preload` value allows CLI binaries and rejects `--web` builds, while `1`/`On`
  allows both.
- Windows calling-convention tests are not part of the supported target matrix;
  the parser accepts the spelling but the ABI selector is ignored.
