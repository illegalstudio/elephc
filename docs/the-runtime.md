# The Runtime

[← Back to Wiki](README.md) | Previous: [The Code Generator](the-codegen.md) | Next: [Memory Model →](memory-model.md)

---

**Source:** `src/codegen/runtime/` — `mod.rs`, `strings/`, `arrays/`, `system/`

The runtime is a collection of **hand-written assembly routines** that handle operations too complex for inline code generation. When the [code generator](the-codegen.md) needs to convert an integer to a string or concatenate two strings, it emits a `bl __rt_itoa` or `bl __rt_concat` — a call to a runtime routine.

These routines are emitted as assembly functions at the end of every compiled program. They're not external libraries — they're part of the binary.

## Why a runtime?

Some operations can't be done with a few inline instructions:

- **Integer to string** (`itoa`): Requires a loop that divides by 10, extracts digits, and writes them right-to-left
- **String concatenation**: Needs to copy bytes from two source strings into a buffer
- **Array operations**: Require heap allocation, bounds checking, and element copying

These are 20-50+ instructions each. Inlining them at every call site would bloat the binary. Instead, they're emitted once and called with `bl`.

## Naming convention

All runtime routines start with `__rt_`:

```
__rt_itoa          integer → string
__rt_ftoa          float → string
__rt_concat        string + string → string
__rt_str_eq        string == string → bool
__rt_array_new     allocate a new array
__rt_build_argv    build $argv from C strings
```

## String routines

**Source:** `src/codegen/runtime/strings/`

### `__rt_itoa` — Integer to string

**File:** `strings/itoa.rs`

Converts a signed 64-bit integer in `x0` to a decimal string.

**Input:** `x0` = integer value
**Output:** `x1` = pointer to string, `x2` = length

**Algorithm:**
1. Check for negative → set flag, negate
2. Check for zero → output "0" directly
3. Loop: divide by 10 (`udiv` + `msub`), convert remainder to ASCII digit (`+ 48`), store right-to-left
4. Prepend '-' if negative
5. Update concat buffer offset

The digits are written **right-to-left** because division gives us the least significant digit first. The result is written into the [concat buffer](memory-model.md#the-string-buffer).

### `__rt_ftoa` — Float to string

**File:** `strings/ftoa.rs`

Converts a double-precision float in `d0` to a decimal string. Handles special cases: `INF`, `-INF`, `NAN`. For normal numbers, it separates the integer and fractional parts, converts each to digits, and joins them with a decimal point.

**Input:** `d0` = float value
**Output:** `x1` = pointer to string, `x2` = length

### `__rt_concat` — String concatenation

**File:** `strings/concat.rs`

Concatenates two strings by copying both into the [concat buffer](memory-model.md#the-string-buffer).

**Input:** `x1`/`x2` = right string (ptr/len), `x3`/`x4` = left string (ptr/len)
**Output:** `x1` = pointer to result, `x2` = total length

**Algorithm:**
1. Get current position in concat buffer
2. Copy left string bytes (byte-by-byte loop)
3. Copy right string bytes
4. Update buffer offset
5. Return pointer to start of result + total length

### `__rt_atoi` — String to integer

**File:** `strings/atoi.rs`

Parses a decimal string into a 64-bit integer. Handles optional leading `-` sign.

**Input:** `x1` = string pointer, `x2` = length
**Output:** `x0` = integer value

### `__rt_str_eq` — String equality

**File:** `strings/str_eq.rs`

Compares two strings byte-by-byte.

**Input:** `x1`/`x2` = first string, `x3`/`x4` = second string
**Output:** `x0` = 1 if equal, 0 if not

**Algorithm:**
1. Compare lengths — if different, return 0 immediately
2. Loop: compare byte by byte
3. If all bytes match, return 1

### Other string routines

Each routine follows the same pattern — inputs in registers, output in standard result registers:

| Routine | What it does | Input | Output |
|---|---|---|---|
| `__rt_strtolower` | Lowercase conversion | `x1`/`x2` | `x1`/`x2` |
| `__rt_strtoupper` | Uppercase conversion | `x1`/`x2` | `x1`/`x2` |
| `__rt_trim` | Strip whitespace | `x1`/`x2` | `x1`/`x2` |
| `__rt_ltrim` / `__rt_rtrim` | Strip left/right | `x1`/`x2` | `x1`/`x2` |
| `__rt_strrev` | Reverse string | `x1`/`x2` | `x1`/`x2` |
| `__rt_strpos` | Find substring | `x1`/`x2` + `x3`/`x4` | `x0` (index or -1) |
| `__rt_strrpos` | Find last occurrence | `x1`/`x2` + `x3`/`x4` | `x0` |
| `__rt_str_repeat` | Repeat N times | `x1`/`x2` + `x0` (count) | `x1`/`x2` |
| `__rt_str_replace` | Replace all occurrences | search + replace + subject | `x1`/`x2` |
| `__rt_explode` | Split by delimiter | delimiter + string | `x0` (array ptr) |
| `__rt_implode` | Join with glue | glue + array | `x1`/`x2` |
| `__rt_strcmp` | Binary comparison | two strings | `x0` (-1, 0, 1) |
| `__rt_strcasecmp` | Case-insensitive compare | two strings | `x0` |
| `__rt_chr` | ASCII code → char | `x0` | `x1`/`x2` |
| `__rt_addslashes` | Escape quotes/backslashes | `x1`/`x2` | `x1`/`x2` |
| `__rt_nl2br` | Insert `<br />` before newlines | `x1`/`x2` | `x1`/`x2` |
| `__rt_bin2hex` | Binary → hex string | `x1`/`x2` | `x1`/`x2` |
| `__rt_hex2bin` | Hex → binary | `x1`/`x2` | `x1`/`x2` |
| `__rt_md5` | MD5 hash | `x1`/`x2` | `x1`/`x2` |
| `__rt_sha1` | SHA1 hash | `x1`/`x2` | `x1`/`x2` |
| `__rt_sprintf` | Format string | format + args on stack | `x1`/`x2` |
| `__rt_base64_encode` | Base64 encode | `x1`/`x2` | `x1`/`x2` |
| `__rt_base64_decode` | Base64 decode | `x1`/`x2` | `x1`/`x2` |
| `__rt_urlencode` | URL encode | `x1`/`x2` | `x1`/`x2` |
| `__rt_urldecode` | URL decode | `x1`/`x2` | `x1`/`x2` |
| `__rt_htmlspecialchars` | HTML escape | `x1`/`x2` | `x1`/`x2` |

## Array routines

**Source:** `src/codegen/runtime/arrays/`

### `__rt_heap_alloc` — Bump allocator

**File:** `arrays/heap_alloc.rs`

Allocates `x0` bytes from the [heap buffer](memory-model.md#the-heap). Simple bump allocation: read current offset, add requested size, write new offset, return pointer.

**Input:** `x0` = bytes to allocate
**Output:** `x0` = pointer to allocated memory

### `__rt_array_new` — Create array

**File:** `arrays/array_new.rs`

Allocates and initializes an array header (24 bytes):

```
Offset 0:  length = 0
Offset 8:  capacity = initial capacity
Offset 16: elem_size (8 for int, 16 for string)
```

Then allocates space for the initial elements.

**Input:** `x0` = element size (8 or 16)
**Output:** `x0` = pointer to array header

### `__rt_array_push_int` / `__rt_array_push_str`

**Files:** `arrays/array_push_int.rs`, `arrays/array_push_str.rs`

Appends an element to an array. Checks capacity, reallocates if needed (copies existing elements to a larger buffer).

### `__rt_sort_int` — In-place sort

**File:** `arrays/sort_int.rs`

Sorts an integer array using a simple algorithm (typically insertion sort or selection sort — efficient enough for the small arrays elephc handles).

## System routines

**Source:** `src/codegen/runtime/system/`

### `__rt_build_argv` — Build $argv array

**File:** `system/build_argv.rs`

At program start, the OS passes `argc` (argument count) in `x0` and `argv` (pointer to C string pointers) in `x1`. This routine:

1. Creates a new string array
2. For each C string pointer in argv: measures the string length (scan for null byte), pushes ptr+len into the array
3. Returns the array pointer

## How routines are emitted

**File:** `src/codegen/runtime/mod.rs`

The `emit_runtime()` function calls every routine's emitter in sequence:

```rust
pub fn emit_runtime(emitter: &mut Emitter) {
    strings::itoa::emit_itoa(emitter);
    strings::ftoa::emit_ftoa(emitter);
    strings::concat::emit_concat(emitter);
    // ... 30+ more routines ...
    arrays::heap_alloc::emit_heap_alloc(emitter);
    arrays::array_new::emit_array_new(emitter);
    // ...
    system::build_argv::emit_build_argv(emitter);
}
```

All routines are included in every binary, even if unused. This is simpler than dead-code elimination (a potential future optimization).

## Runtime data

The runtime also declares global buffers using `.comm`:

```asm
.comm _concat_buf, 65536     ; 64KB string buffer
.comm _concat_off, 8         ; current offset into string buffer
.comm _heap_buf, 1048576     ; 1MB heap
.comm _heap_off, 8           ; current heap offset
```

See [Memory Model](memory-model.md) for details on how these buffers work.

---

Next: [Memory Model →](memory-model.md)
