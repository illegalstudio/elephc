# Compiler Extensions

[← Back to Wiki](README.md) | See also: [Language Reference](language-reference.md)

---

elephc compiles a static subset of PHP to native ARM64 binaries. All standard PHP syntax it supports is **100% compatible** with the PHP interpreter — the same program must produce the same output when run with `php`.

However, elephc also provides **compiler-specific extensions** that go beyond standard PHP. These features have no PHP equivalent and will not run under the PHP interpreter. They exist to enable use cases that PHP was never designed for: calling C libraries, accessing raw memory, writing game loops with predictable performance, and conditional compilation.

This document is the complete reference for every extension. If a feature is listed here, it is **not PHP** — it is elephc-only syntax.

---

## Table of contents

1. [Pointers](#pointers)
2. [FFI (Foreign Function Interface)](#ffi-foreign-function-interface)
3. [Hot-path data types](#hot-path-data-types)
4. [Conditional compilation](#conditional-compilation)
5. [CLI flags](#cli-flags)
6. [Design principles](#design-principles)
7. [Best practices](#best-practices)
8. [What NOT to do](#what-not-to-do)

---

## Pointers

Pointers provide low-level memory access. A pointer is a 64-bit memory address stored in a single register.

### Creating pointers

```php
<?php
$x = 42;
$p = ptr($x);        // take the address of a stack variable
$null = ptr_null();   // create a null pointer (0x0)
```

### Reading and writing through pointers

```php
<?php
$x = 10;
$p = ptr($x);
echo ptr_get($p);     // 10 — read 8 bytes at the address
ptr_set($p, 99);      // write 8 bytes at the address
echo $x;              // 99 — the variable was modified through the pointer
```

### Pointer arithmetic

```php
<?php
$x = 100;
$p = ptr($x);
$q = ptr_offset($p, 8);   // advance by 8 bytes
echo ptr_get($q);          // reads the next 8-byte value on the stack
```

`ptr_offset` adds a **byte** offset. The result preserves the pointer's type tag if one exists.

### Typed pointers and casting

```php
<?php
$p = ptr($x);
$typed = ptr_cast<MyClass>($p);   // change the type tag without changing the address
echo $p === $typed;               // 1 — same address
```

`ptr_cast<T>()` is a compile-time operation. The value in the register does not change — only the static type tag used by the type checker.

### Querying sizes

```php
<?php
echo ptr_sizeof("int");      // 8
echo ptr_sizeof("string");   // 16 (pointer + length)
echo ptr_sizeof("float");    // 8
echo ptr_sizeof("Point");    // computed from class properties
```

### Sub-word access

```php
<?php
$p = ptr($x);
ptr_write8($p, 0x41);        // write 1 byte
echo ptr_read8($p);           // read 1 byte (zero-extended)
ptr_write32($p, 12345);      // write 4 bytes
echo ptr_read32($p);          // read 4 bytes (zero-extended)
```

### Null safety

`ptr_get` and `ptr_set` abort with a fatal error on null pointer dereference:

```
Fatal error: null pointer dereference
```

Use `ptr_is_null($p)` to check before dereferencing.

### Echo and comparison

```php
<?php
$p = ptr($x);
echo $p;                      // prints hex address: 0x16f502348
echo ptr_null();              // prints: 0x0
echo $p === $q ? "same" : "different";   // pointer comparison by address
echo gettype($p);             // "pointer"
```

### Pointer function reference

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

---

## FFI (Foreign Function Interface)

FFI lets elephc programs call C library functions directly, with automatic type marshalling.

### Declaring extern functions

```php
<?php
// Single function, linked via -lSystem (default)
extern function abs(int $n): int;
extern function getpid(): int;

// With explicit library
extern "curl" function curl_easy_init(): ptr;

// Block syntax for multiple functions from one library
extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_GetTicks(): int;
    function SDL_Quit(): void;
}
```

### Supported C types

| elephc type | C equivalent | Register |
|---|---|---|
| `int` | `int64_t` / `long` | x0-x7 |
| `float` | `double` | d0-d7 |
| `bool` | `int` (0/1) | x0-x7 |
| `string` | `char*` (auto null-terminated) | x0-x7 |
| `ptr` | `void*` | x0-x7 |
| `ptr<T>` | `T*` | x0-x7 |
| `void` | void (return only) | — |
| `callable` | function pointer | x0-x7 |

### String conversion

elephc strings are `(pointer, length)` pairs. C expects null-terminated `char*`. The conversion is automatic:

- **Calling C**: elephc allocates a null-terminated copy, passes `char*`, frees after the call returns
- **C returns string**: elephc scans for `\0`, computes length, copies to owned heap storage

```php
<?php
extern function getenv(string $name): string;
$home = getenv("HOME");   // automatic conversion both ways
echo $home;               // works as a normal elephc string
```

### Callbacks

Pass elephc functions to C as function pointers using string literal names:

```php
<?php
extern function signal(int $sig, callable $handler): ptr;

function on_signal($sig) {
    echo "caught signal " . $sig . "\n";
}

signal(15, "on_signal");   // passes the address of _fn_on_signal
```

Callback functions must use C-compatible types only (int, float, bool, ptr, void). No strings, no arrays, no variadic, no defaults, no pass-by-reference.

### Extern globals

Access C global variables:

```php
<?php
extern global ptr $environ;
echo ptr_is_null($environ) ? "no env" : "has env";
```

Extern globals use GOT-relative addressing. String globals are automatically converted between `char*` and elephc strings.

### Extern classes (C structs)

Map C struct layouts for typed pointer access:

```php
<?php
extern class Point {
    public int $x;
    public int $y;
}

// Use with malloc or ptr_cast
extern function malloc(int $size): ptr;
$p = ptr_cast<Point>(malloc(16));
$p->x = 10;
$p->y = 20;
echo $p->x;   // 10
```

Extern classes have flat sequential layout (no class_id, no vtable, no 16-byte property slots). Field offsets match C struct packing with 8-byte alignment.

### CLI linker flags

```bash
elephc --link curl app.php          # adds -lcurl to linker
elephc -lcurl app.php               # short form
elephc --link-path /opt/homebrew/lib app.php   # adds -L path
elephc -L/opt/homebrew/lib app.php             # short form
elephc --framework Cocoa app.php    # adds -framework Cocoa (macOS)
```

Libraries declared in `extern "libname" { }` blocks are linked automatically.

---

## Hot-path data types

For game loops, renderers, and performance-critical code, elephc provides `packed class` and `buffer<T>` — contiguous data structures with zero hash lookup overhead.

### Why not PHP arrays?

PHP arrays are hash tables. Every access goes through `__rt_hash_get` (key hashing, linear probing, entry comparison). This is fine for general-purpose code but unacceptable for inner loops that touch thousands of elements per frame.

`buffer<T>` compiles to a single `ldr` instruction after an address calculation: `base + 16 + index * stride`. No hashing, no probing, no indirection.

### `packed class`

A `packed class` is a flat record with compile-time field offsets. Only POD types allowed.

```php
<?php
packed class Enemy {
    int $x;
    int $y;
    int $hp;
    int $state;
}
```

Constraints:
- Fields must be `int`, `float`, `bool`, `ptr`, or another `packed class`
- Union and nullable annotations are not allowed in packed fields because they do not have a fixed POD layout
- No strings, arrays, objects, or mixed values
- No methods, constructors, inheritance, traits, or interfaces
- No visibility modifiers (all fields are public)
- No default values
- Layout is sequential: field 0 at offset 0, field 1 at offset 8, etc.

### `buffer<T>`

A `buffer<T>` is a fixed-size contiguous array of POD values or packed records.

```php
<?php
// Scalar buffers
buffer<int> $ids = buffer_new<int>(1000);
buffer<float> $speeds = buffer_new<float>(1000);

// Packed class buffer (Array of Structs)
buffer<Enemy> $enemies = buffer_new<Enemy>(256);
```

`buffer<T>` only accepts POD scalar, pointer, or packed-record element types. Union types such as `buffer<int|string>` and nullable forms such as `buffer<?int>` are rejected because buffer elements must have a single fixed stride.

### Reading and writing

```php
<?php
buffer<int> $buf = buffer_new<int>(10);
$buf[3] = 42;          // direct store
echo $buf[3];           // direct load — no hash lookup

buffer<Enemy> $enemies = buffer_new<Enemy>(100);
$enemies[0]->x = 100;
$enemies[0]->hp = 50;
echo $enemies[0]->x;   // 100
```

When accessing a `buffer<PackedClass>`, the index expression returns a typed pointer to the element. Field access is then a direct offset load.

### Buffer length

```php
<?php
buffer<float> $data = buffer_new<float>(512);
echo buffer_len($data);   // 512
```

### Freeing buffers

```php
<?php
buffer<int> $buf = buffer_new<int>(1000);
// ... use the buffer ...
buffer_free($buf);   // release heap memory, nullify the variable
```

`buffer_free` releases the heap allocation and zeros the stack slot. Any subsequent read or write to the freed buffer produces a deterministic fatal error instead of a silent crash:

```
Fatal error: use of buffer after buffer_free()
```

Use it when allocating buffers inside loops or when a buffer is no longer needed:

```php
<?php
for ($i = 0; $i < 100; $i++) {
    buffer<int> $tmp = buffer_new<int>(10000);
    // ... use $tmp ...
    buffer_free($tmp);   // prevents heap exhaustion
}
```

**Restrictions:**
- `buffer_free` only accepts plain local variables — not ref params, globals, statics, or expressions
- If you have aliases (`$b = $a`), freeing `$a` does **not** invalidate `$b` — accessing `$b` after `buffer_free($a)` is undefined behavior, like `free()` in C
- Freeing the same buffer twice is undefined behavior

### Bounds checking

Buffer access is bounds-checked at runtime. Out-of-bounds access (including negative indices) terminates the program:

```
Fatal error: buffer index out of bounds
```

### Buffer memory layout

```
Offset 0:   [length: 8 bytes]     logical element count
Offset 8:   [stride: 8 bytes]     element size in bytes
Offset 16:  [element 0]
Offset 16 + stride: [element 1]
...
```

### SoA vs AoS patterns

**Structure of Arrays (SoA)** — parallel buffers, one per field:

```php
<?php
// Better cache locality when iterating one field at a time
buffer<float> $x = buffer_new<float>(1000);
buffer<float> $y = buffer_new<float>(1000);
buffer<int> $hp = buffer_new<int>(1000);

for ($i = 0; $i < 1000; $i++) {
    $x[$i] = $x[$i] + $speed * $dt;   // sequential memory access
}
```

**Array of Structures (AoS)** — one buffer of packed records:

```php
<?php
// Better when you always access all fields together
packed class Particle {
    float $x;
    float $y;
    float $vx;
    float $vy;
}
buffer<Particle> $particles = buffer_new<Particle>(10000);

for ($i = 0; $i < buffer_len($particles); $i++) {
    $particles[$i]->x = $particles[$i]->x + $particles[$i]->vx;
    $particles[$i]->y = $particles[$i]->y + $particles[$i]->vy;
}
```

### Limitations (v1)

- Fixed size — no `push`, `pop`, or dynamic resize
- No automatic scope cleanup — use `buffer_free($buf)` to release memory explicitly
- No conversion to/from PHP arrays
- No copy-on-write semantics
- No iteration with `foreach`
- No mixed element types
- Payload is zero-initialized by `buffer_new`, but re-reading after `buffer_free` and re-allocation is undefined

---

## Conditional compilation

`ifdef` selectively includes or excludes code blocks based on compile-time symbols.

### Syntax

```php
<?php
ifdef DEBUG {
    echo "debug mode\n";
    // expensive validation, logging, assertions
} else {
    echo "release mode\n";
}
```

### How it works

- Symbols are set via `--define` CLI flags
- `ifdef` is resolved **before** include resolution and type checking
- The inactive branch is completely removed from the AST — it does not need to be valid code
- Inactive branches can reference files that don't exist (`require "debug-tools.php"`) without error

### Use cases

```php
<?php
// Platform-specific code
ifdef USE_SDL {
    extern "SDL2" {
        function SDL_Init(int $flags): int;
    }
    SDL_Init(0x20);
}

// Debug-only assertions
ifdef DEBUG {
    if ($hp < 0) {
        echo "BUG: negative HP!\n";
        exit(1);
    }
}

// Feature flags
ifdef FEATURE_AUDIO {
    require "audio.php";
}
```

### CLI usage

```bash
elephc --define DEBUG app.php
elephc --define DEBUG --define USE_SDL app.php
elephc --define=FEATURE_AUDIO app.php
```

### Nesting

`ifdef` blocks can be nested:

```php
<?php
ifdef PLATFORM_MAC {
    ifdef USE_METAL {
        echo "Metal renderer\n";
    } else {
        echo "OpenGL renderer\n";
    }
}
```

### Constraints

- Symbols are simple names (no expressions, no `ifndef`, no `#if`)
- Symbols come only from `--define` flags — not from `const` or `define()`
- `ifdef` is not PHP syntax — programs using it will not run under `php`

---

## CLI flags

Complete list of elephc-specific CLI flags:

| Flag | Description |
|---|---|
| `--heap-size=BYTES` | Set heap buffer size (default 8MB, minimum 64KB) |
| `--gc-stats` | Print GC allocation/free statistics at exit |
| `--heap-debug` | Enable runtime heap verification (slow) |
| `--define SYMBOL` | Define a compile-time symbol for `ifdef` |
| `--link LIB` / `-lLIB` | Link an additional library |
| `--link-path DIR` / `-LDIR` | Add a library search path |
| `--framework NAME` | Link a macOS framework |

---

## Design principles

### Why separate extensions from PHP?

elephc's core value is PHP compatibility. Standard PHP programs must work identically. Extensions exist for use cases PHP cannot address:

1. **Pointers** — PHP has no memory access primitives. Game engines need framebuffer writes, struct access, DMA.
2. **FFI** — PHP's FFI extension is interpreted. elephc's extern functions compile to direct `bl` calls with zero overhead.
3. **Hot-path data** — PHP arrays are hash tables. Renderers need contiguous memory with O(1) indexed access.
4. **Conditional compilation** — PHP has no build system. `ifdef` provides zero-cost feature flags.

### Extensions are clearly distinguishable

Every extension uses syntax that PHP does not recognize:
- `ptr()`, `ptr_cast<T>()`, `buffer_new<T>()` — function names with angle brackets
- `extern function`, `extern class`, `extern global` — `extern` keyword
- `packed class` — `packed` keyword
- `buffer<T>` — generic type syntax in variable declarations
- `ifdef` — keyword

A developer reading the code can instantly tell which parts are PHP and which are elephc-specific.

---

## Best practices

### Keep extension code isolated

```php
<?php
// GOOD: extension code in a clearly separate file
// engine.php — elephc-only, not runnable under php
extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_CreateWindow(string $title, int $x, int $y, int $w, int $h, int $flags): ptr;
}

// game.php — pure PHP logic, testable under php
function update_score($current, $bonus) {
    return $current + $bonus;
}
```

### Use `ifdef` for optional platform features

```php
<?php
ifdef USE_SDL {
    require "sdl_renderer.php";
} else {
    require "null_renderer.php";
}
```

### Prefer SoA for large datasets

When iterating thousands of elements and touching only a few fields per iteration, SoA gives better cache locality:

```php
<?php
// GOOD: SoA — each loop touches one contiguous buffer
buffer<float> $x = buffer_new<float>(10000);
buffer<float> $y = buffer_new<float>(10000);
for ($i = 0; $i < 10000; $i++) {
    $x[$i] = $x[$i] + 1.0;
}

// LESS GOOD: AoS — each loop touches stride-spaced memory
buffer<Entity> $entities = buffer_new<Entity>(10000);
for ($i = 0; $i < 10000; $i++) {
    $entities[$i]->x = $entities[$i]->x + 1.0;   // skips over y, hp, state...
}
```

### Use `packed class` for structured hot-path data

```php
<?php
// GOOD: explicit layout, predictable performance
packed class Vertex {
    float $x;
    float $y;
    float $u;
    float $v;
    int $color;
}

// BAD: assoc array for hot-path data (hash lookup per access)
$vertex = ["x" => 1.0, "y" => 2.0, "u" => 0.0, "v" => 0.0, "color" => 0xFF];
```

### Use extern for system calls, not for reimplementing builtins

```php
<?php
// GOOD: calling a C function that elephc doesn't have
extern function mmap(ptr $addr, int $len, int $prot, int $flags, int $fd, int $offset): ptr;

// BAD: calling C strlen when elephc already has strlen()
extern function strlen(string $s): int;   // shadows the builtin — confusing
```

---

## What NOT to do

### Do not use pointers for general-purpose programming

Pointers bypass the type system and memory safety. Use them only when you need direct memory access (framebuffers, C struct fields, DMA).

```php
<?php
// BAD: using pointers instead of variables
$x = 42;
$p = ptr($x);
ptr_set($p, ptr_get($p) + 1);   // just write $x = $x + 1

// GOOD: using pointers for framebuffer access
$fb = ptr_cast<int>(SDL_GetFramebuffer($surface));
for ($i = 0; $i < $width * $height; $i++) {
    ptr_write32(ptr_offset($fb, $i * 4), $color);
}
```

### Do not use `buffer<T>` for small collections

The overhead of `buffer_new` (heap allocation) is not worth it for a few elements. Use PHP arrays.

```php
<?php
// BAD: buffer for 3 elements
buffer<int> $rgb = buffer_new<int>(3);

// GOOD: PHP array for small data
$rgb = [255, 128, 0];
```

### Do not mix extension code into pure PHP logic

Keep your code testable. Pure PHP functions can be tested with `php` directly. Extension code can only be tested with `elephc`.

```php
<?php
// BAD: mixing buffer access into business logic
function calculate_damage($enemies, $i) {
    return $enemies[$i]->hp * 2;   // requires buffer<Enemy>, untestable with php
}

// GOOD: separate data access from logic
function calculate_damage($hp) {
    return $hp * 2;   // pure PHP, testable anywhere
}
// caller: calculate_damage($enemies[$i]->hp)
```

### Buffer memory is zero-initialized

`buffer_new<T>(n)` zero-fills the entire payload before returning. Reading an element before writing it is safe and returns 0 (int/bool), 0.0 (float), or null (ptr).

### Do not pass buffers to PHP array functions

Buffers are not PHP arrays. They do not work with `count()`, `foreach`, `array_map()`, `sort()`, `in_array()`, or any other array function.

```php
<?php
buffer<int> $buf = buffer_new<int>(10);

// BAD: these will not work as expected
count($buf);         // returns 0 or crashes — $buf is a heap pointer, not an array
foreach ($buf as $v) { }   // type error — $buf is not iterable

// GOOD: use buffer_len and indexed loops
for ($i = 0; $i < buffer_len($buf); $i++) {
    echo $buf[$i] . "\n";
}
```
