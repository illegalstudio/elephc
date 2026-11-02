---
title: "FFI & Extern"
description: "Foreign Function Interface: calling C libraries, extern functions, globals, classes, and callbacks."
sidebar:
  order: 4
---

FFI lets elephc programs call C library functions directly, with automatic type marshalling.

> **Strict mode:** `extern` declarations are an elephc extension with no PHP equivalent. Compiling with [`--strict-php`](../compiling/cli-reference.md#strict-php-mode) rejects them.

## Declaring extern functions
```php
<?php
extern function abs(i32 $n): i32;
extern function getpid(): i32;

// With explicit library
extern "curl" function curl_easy_init(): ptr;

// Block syntax
extern "SDL2" {
    function SDL_Init(u32 $flags): i32;
    function SDL_Quit(): void;
}
```

## Supported C types

| elephc type | C equivalent | Passed in |
|---|---|---|
| `int` (alias `i64`) | `int64_t` / `long` / `ssize_t` | integer argument register |
| `u64` | `uint64_t` / `size_t` | integer argument register |
| `i32` / `u32` | `int32_t` / `uint32_t` (C `int` / `unsigned`) | integer argument register |
| `i16` / `u16` | `int16_t` / `uint16_t` (`short`) | integer argument register |
| `i8` / `u8` | `int8_t` / `uint8_t` (`signed char` / `unsigned char`) | integer argument register |
| `float` (alias `f64`) | `double` | floating-point argument register |
| `f32` | `float` | floating-point argument register |
| `bool` | `bool` (`_Bool`, one byte) | integer argument register |
| `string` | `char*` (auto null-terminated) | integer argument register |
| `?string` | `char*` that may be `NULL` (parameters and returns only) | integer argument register |
| `ptr` | `void*` | integer argument register |
| `ptr<T>` | `T*` | integer argument register |
| `ptr<ptr<T>>` | `T**` | integer argument register |
| `void` | void (return only) | — |
| `callable` | function pointer | integer argument register |

Argument registers follow the selected target's C ABI: AArch64 uses
`x0`-`x7` for integers/pointers and `d0`-`d7` for doubles, while Linux
x86_64 uses the System V registers (`rdi`, `rsi`, `rdx`, `rcx`, `r8`, `r9`
for integers/pointers and `xmm0`-`xmm7` for doubles).

## Integer widths

On the PHP side every C integer is an `int`: `$t = SDL_GetTicks()` is a plain
64-bit PHP integer whatever width the declaration names, and it can be passed
to any extern parameter declared `int`, `u32`, `i64`, and so on. The width only
describes the C side, and it matters because both supported ABIs leave the bits
above a narrow value unspecified: a function returning a C `int` may leave
garbage in the upper 32 bits of the return register, and a callee compiled by
clang expects the caller to extend arguments narrower than 32 bits.

Declare the width the C header uses:

| C type | elephc extern type |
|---|---|
| `int`, `int32_t` | `i32` |
| `unsigned`, `uint32_t`, `Uint32` | `u32` |
| `long`, `int64_t`, `ssize_t`, `intptr_t`, `off_t` | `int` (or `i64`) |
| `size_t`, `uint64_t`, `Uint64`, `uintptr_t` | `u64` |
| `short`, `int16_t` | `i16` |
| `unsigned short`, `uint16_t`, `Uint16` | `u16` |
| `signed char`, `int8_t` | `i8` |
| `unsigned char`, `uint8_t`, `Uint8` | `u8` |
| `bool`, `_Bool` | `bool` |
| `float` | `f32` |
| `double` | `float` (or `f64`) |

Plain `char` is signed on linux-x86_64 and on the Apple targets but unsigned on
linux-aarch64, so declare it `i8` or `u8` from the header's intent, not from
the keyword.

What elephc does at the call boundary:

- **Returns** are widened from the declared width before PHP sees them: `i32`
  sign-extends, `u32` zero-extends, `i16`/`u16` and `i8`/`u8` do the same for
  their widths, `int`/`i64`/`u64` pass through, and `bool` keeps only its low
  byte. A `u64` above `PHP_INT_MAX` reads as a negative `int`, exactly like
  reinterpreting the bits.
- **Arguments** of 32 or 64 bits are passed as they are; the callee reads only
  the bits it declared. Arguments of 8 or 16 bits are reduced modulo 2^width
  and extended to 32 bits by the caller, as C would convert them: `300` passed
  to a `u8` parameter arrives as `44`.
- Nothing is checked at runtime: like C, an out-of-range value truncates
  silently.

Declaring a C `int` function as `int` is a mistake the compiler cannot detect:
the call works whenever the callee happens to leave the upper bits clean and
reads garbage otherwise. Explicit widths are accepted on `extern function`
parameters and returns and on [`extern class` fields](#extern-classes-c-structs);
`extern global` declarations and callback signatures still use `int`.

## Float widths

`float` is a C `double`. Declare a C `float` parameter or return as `f32`: the
PHP value is still a 64-bit `float`, and elephc rounds it to single precision
before the call (IEEE round-to-nearest, values outside the single range become
infinity) and widens a returned single back to a double. The C side only ever
reads the low 32 bits of its floating-point register, so a C `float` function
declared as `float` passes and reads garbage.

```php
<?php
extern function sqrtf(f32 $x): f32;
var_dump(sqrtf(2.0));   // float(1.4142135381698608): the single-precision root, widened
```

Like the integer widths, `f32` is accepted on `extern function` parameters and
returns and on `extern class` fields, so a C struct that holds `float` fields
(`SDL_FRect`, `SDL_Vertex`) is declared with `f32` fields and read and written
by name; see [Extern classes](#extern-classes-c-structs). Packed classes and
`buffer<T>` elements take the same width names for data elephc owns: a
`buffer<Vertex>` of `f32` fields hands its elements to C as `ptr<Vertex>`, and
`buffer_ptr()` hands a `buffer<u16>` index list or a `buffer<f32>` stream to C
at its width; see [Packed classes](packed-classes.md#field-widths) and
[Buffers](buffers.md#payload-pointer). Raw memory without a
declared struct uses `ptr_read_f32()` and `ptr_write_f32()`; see
[Pointers](pointers.md).

`callable` in an `extern function` signature is a special FFI-only path. The
call site can provide either a string-literal elephc function name (resolved
case-insensitively like PHP function names) or a callable descriptor. Descriptor
callbacks are bound into generated C-ABI trampoline symbols, including values
with closure captures, object receivers, late-static-binding context, or
branch-selected descriptor state. The trampoline receives the C callback arguments, reloads the retained
descriptor from global storage, invokes the descriptor's runtime invoker, and
casts the boxed result back to the C-compatible callback return type.

FFI callback descriptors are limited to fixed scalar/pointer signatures:
`int`, `float`, `bool`, `ptr`, and `void` return values, with parameters drawn
from `int`, `float`, `bool`, and `ptr`. `string`, arrays, objects, variadics,
defaults, and by-reference callback parameters are rejected because ownership
and temporary lifetime across a C callback boundary are not modeled safely yet.
For C APIs without a userdata/context parameter, each generated trampoline owns
one mutable descriptor slot for that callsite; registering a new descriptor at
the same callsite replaces the previous slot owner. Two C registrations made
through one callsite (a helper or a loop calling `signal()` for two signals)
therefore share the function pointer and both run the last descriptor bound
there; give each long-lived registration its own callsite.

## String conversion
- **Calling C**: elephc creates temporary null-terminated copy, frees after call
- **C returns string**: elephc scans for `\0`, copies to owned storage
- **`NULL`**: a `string` never passes `NULL` and reads a returned `NULL` as `""`; declare
  `?string` where the C side allows `NULL` (see [Nullable strings](#nullable-strings))

Extern calls support the declared parameter names:

```php
<?php
extern function strcmp(string $left, string $right): i32;

$left = ["a"];
echo strcmp(...$left, right: "b");
```

Argument expressions are evaluated in PHP source order, then elephc loads the
resulting values into the target C ABI registers. This matters when positional,
named, or spread arguments have side effects.

Declared extern functions can also be selected by dynamic PHP string callbacks
such as `call_user_func($cb, ...)` and `call_user_func_array($cb, $args)`.
The runtime descriptor uses an extern invocation shape and a generated PHP-ABI
wrapper; the wrapper performs the C ABI call after PHP argument normalization.

## Nullable strings

Many C APIs take a `const char*` that may be `NULL` (`SDL_CreateRenderer()`
lets SDL pick the driver when the name is `NULL`) or return `NULL` instead of a
string (`getenv()` for an unset variable). Declare those parameters and returns
as `?string`:

```php
<?php
extern "SDL3" {
    function SDL_CreateRenderer(ptr $window, ?string $name): ptr;
    function SDL_GetError(): ?string;
}
extern function getenv(string $name): ?string;

$renderer = SDL_CreateRenderer($window, null);       // NULL: SDL picks the driver
$software = SDL_CreateRenderer($window, "software"); // a temporary "software\0" copy
var_dump(getenv("SURELY_UNSET_VARIABLE"));           // NULL
```

On the PHP side the type is `?string` (`string|null`):

- **Arguments**: PHP `null` passes a `NULL` pointer. A string passes the same
  temporary null-terminated copy a `string` parameter gets, freed after the
  call. A `?string` value, such as a `?string` variable or
  `$flag ? "a" : null`, is checked at runtime and passes whichever it holds.
  A `mixed` value holding an int, float or bool passes its PHP string form;
  one holding an array or an object is a fatal error, so convert it first
  (`(string) $value`), because elephc does not run `__toString()` or an error
  handler in the middle of building the C arguments.
- **Returns**: a `NULL` return reads as PHP `null`; any other pointer is copied
  up to its terminator into an owned string, exactly like a `string` return.

A plain `string` keeps its conversions: it never passes `NULL`, and a `NULL`
`char*` returned through it reads as `""`. Use `?string` whenever the C header
says the pointer may be `NULL`. `?` applies to `string` only, since `ptr`
already accepts `null` as the address 0, and `?string` is accepted on
`extern function` parameters and returns but not on `extern class` fields or
`extern global` declarations.

## Callbacks
```php
<?php
extern function signal(i32 $sig, callable $handler): ptr;

function on_signal($sig) {
    echo "caught signal " . $sig . "\n";
}

signal(15, "on_signal");
```

Descriptor-backed callbacks can carry state:

```php
<?php
extern function signal(i32 $sig, callable $handler): ptr;

$delta = 3;
$handler = function (int $sig) use ($delta): void {
    echo $sig + $delta;
};

signal(15, $handler);
```
Callbacks must use C-compatible types only. No strings, arrays, variadic, defaults, or pass-by-reference.

Descriptor-backed callbacks are accepted when no environment is needed:

```php
<?php
extern function signal(i32 $sig, callable $handler): ptr;

function on_signal(int $sig): void {
    echo $sig;
}

$handler = on_signal(...);
signal(15, $handler);
```

## Extern globals
```php
<?php
extern global ptr $environ;
```
Uses GOT-relative addressing. String globals auto-converted.

## Extern classes (C structs)

An `extern class` declares a C struct. It has no class id and no vtable: a
`ptr<Class>` (from `ptr_cast<Class>()` or an extern function declared to return
`ptr<Class>`) is the struct's address, and `$p->field` reads and writes the
field in place.

```php
<?php
extern class SDL_Rect { public i32 $x; public i32 $y; public i32 $w; public i32 $h; }
extern class SDL_Color { public u8 $r; public u8 $g; public u8 $b; public u8 $a; }
extern class SDL_FRect { public f32 $x; public f32 $y; public f32 $w; public f32 $h; }
extern class SDL_KeyboardEvent {
    public u32 $type;
    public u32 $reserved;
    public u64 $timestamp;
    public u32 $windowID;
    public u32 $which;
    public u32 $scancode;
    public u32 $key;
    public u16 $mod;
    public u16 $raw;
    public bool $down;
    public bool $repeat;
}

extern function malloc(u64 $size): ptr;

$rect = ptr_cast<SDL_Rect>(malloc(ptr_sizeof("SDL_Rect")));
$rect->x = 10;
$rect->w = 640;
echo $rect->w;                          // 640
echo ptr_sizeof("SDL_KeyboardEvent");   // 40, the C sizeof
```

Fields take the same types as extern function parameters:

| Field type | C type | Size and alignment |
|---|---|---|
| `int` / `i64`, `u64` | `int64_t`, `uint64_t` (`long`, `size_t`) | 8 |
| `i32`, `u32` | `int32_t`, `uint32_t` (`int`, `unsigned`) | 4 |
| `i16`, `u16` | `int16_t`, `uint16_t` | 2 |
| `i8`, `u8` | `int8_t`, `uint8_t` | 1 |
| `float` / `f64` | `double` | 8 |
| `f32` | `float` | 4 |
| `bool` | `bool` (`_Bool`) | 1 |
| `ptr`, `ptr<T>` | `void*`, `T*` | 8 |
| `string` | `char*` | 8 |

`void`, `callable` and `?string` fields are rejected.

**Layout.** Fields are laid out like a C compiler does on every supported
target: each field starts at the next multiple of its own size, and the struct
size is rounded up to the largest field alignment. Declare the fields in the
header's order with the header's types and the offsets and `ptr_sizeof()`
match the C `sizeof`; there is no packing attribute. A `bool` field is one
byte, like C `_Bool` (earlier versions gave it an 8-byte slot, which matched no
C struct). Structs whose fields are all `int`, `float`, `ptr` or `string` keep
the plain 8-byte-per-field layout.

**Reads and writes.** A field reads as its PHP type: every integer width is an
`int` (signed widths sign-extend, unsigned widths zero-extend, a `u64` above
`PHP_INT_MAX` reads as the same bits), both float widths are a `float` (`f32`
widens exactly), and a `bool` reads any nonzero byte as `true`. A write stores
only the field's width, so an out-of-range integer keeps its low bits like a C
conversion (`300` in a `u8` is `44`), a float written to an `f32` field rounds
to single precision, and an `int` written to a float field converts. A value
whose type is only known at run time (integer arithmetic such as `$r->w * 2`
or `$r->x += 1`, float arithmetic over a loop counter such as
`$f->w = 1.5 * ($i + 1)`, a `mixed` parameter) is accepted into an integer,
float or `bool` field and checked as it is stored, with the rules
[packed class fields](packed-classes.md#values-known-only-at-run-time) have:
an integer field takes an int, a float field a float or an int, a `bool` field
a bool, and any other value raises a `TypeError` instead of reaching C memory.
Statically wrong types (a string into an `f32` field, an int into a `bool`
field) are compile errors.

**Pointer fields** chain: a `ptr<Node>` field reads as a typed pointer, so
`$node->next->value` follows it. Inside a namespace, the class a `ptr<T>`
names in an extern class field, extern function signature or extern global
resolves like any other class name (`ptr<Node>` in `namespace App` is
`App\Node`, and `use` aliases apply). **`string` fields** are `char*`: a read copies
the bytes up to the terminator into a PHP string (`NULL` reads as `""`), and a
write stores a fresh null-terminated copy that the struct keeps for the rest
of the program, like an `extern global string` store; C must not `free()` it.

**Null pointers.** A field access through a null `ptr<Class>` aborts with the
pointer extension's null-dereference fatal error, as for packed classes.

**Unions.** There are no union declarations. Declare each member struct as its
own extern class and overlay them on the same address with `ptr_cast`, the way
`SDL_Event` is read: check `$event->type` through a small header struct, then
cast the same pointer to the matching event struct.

```php
<?php
extern class SDL_CommonEvent { public u32 $type; public u32 $reserved; public u64 $timestamp; }
// $buffer: 128 bytes filled by SDL_PollEvent()
$common = ptr_cast<SDL_CommonEvent>($buffer);
if ($common->type === 0x300) {          // SDL_EVENT_KEY_DOWN
    $key = ptr_cast<SDL_KeyboardEvent>($buffer);
    echo $key->scancode;
}
```

Structs embedded by value inside another extern class are not supported yet;
declare the inner fields inline or reach them with `ptr_offset()` and
`ptr_cast`. [Packed classes](packed-classes.md) are a separate elephc-native
layout with 8-byte fields and nested records.

## CLI linker flags

| Flag | Description |
|---|---|
| `--link LIB` / `-lLIB` | Link additional library |
| `--link-path DIR` / `-LDIR` | Add library search path |
| `--framework NAME` | Link macOS framework |

Libraries in `extern "lib" {}` blocks are linked automatically.
