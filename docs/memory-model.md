# Memory Model

[← Back to Wiki](README.md) | Previous: [The Runtime](the-runtime.md)

---

elephc manages memory without a garbage collector, without `malloc`/`free`, and without any runtime library. Everything is either on the **stack** (automatic, per-function) or in **bump-allocated buffers** (global, never freed).

This page explains where every value lives in memory at runtime.

## The four memory regions

```
┌─────────────────────────────┐  High addresses
│         Stack                │  ← grows downward (sp decreases)
│  (function frames, locals)   │
├─────────────────────────────┤
│         (unused)             │
├─────────────────────────────┤
│       Heap buffer            │  _heap_buf: 1MB, bump-allocated
│  (arrays, dynamic data)      │
├─────────────────────────────┤
│     String buffer            │  _concat_buf: 64KB, bump-allocated
│  (string operation results)  │
├─────────────────────────────┤
│       Data section           │  String literals, float constants
│  (.data — read-only)         │
├─────────────────────────────┤
│       Code section           │  Instructions
│  (.text — executable)        │
└─────────────────────────────┘  Low addresses
```

## The stack

The stack is the primary storage for local variables. See [Introduction to ARM64 Assembly](arm64-assembly.md#the-stack-function-local-storage) for the basics.

### Stack frame layout

Each function has a stack frame. The [code generator](the-codegen.md) calculates the size during compilation by counting all local variables:

```
                         x29 (frame pointer)
                          │
                          ▼
┌────────────┬────────────┬────────────┬────────────┐
│  saved x30 │  saved x29 │   $x (8B)  │   $y (8B)  │ ...
└────────────┴────────────┴────────────┴────────────┘
  [x29, #8]    [x29, #0]   [x29, #-8]   [x29, #-16]
```

- `x29` and `x30` are saved at the top of the frame (positive offsets from `x29`)
- Local variables live at **negative offsets** from `x29`
- Strings take **two slots** (16 bytes): pointer at `[x29, #-offset]`, length at `[x29, #-(offset-8)]`
- The total frame size is always 16-byte aligned (ARM64 ABI requirement)

### Variable allocation

Variables are allocated stack slots when the [code generator](the-codegen.md) scans the function body (`collect_local_vars`). The allocation is determined at compile time — there's no dynamic stack growth.

| Type | Stack space | Stored as |
|---|---|---|
| `Int` | 8 bytes | Signed 64-bit value |
| `Float` | 8 bytes | IEEE 754 double |
| `Bool` | 8 bytes | 0 or 1 (stored as 64-bit for alignment) |
| `Str` | 16 bytes | 8-byte pointer + 8-byte length |
| `Array` | 8 bytes | Pointer to heap-allocated header |
| `Void` (null) | 8 bytes | Sentinel value `0x7FFFFFFFFFFFFFFE` |

### The null sentinel

`null` is represented as the integer `0x7FFFFFFFFFFFFFFE` — a value chosen to be distinguishable from any real integer (it's near `INT_MAX` but not equal to it). Before arithmetic operations, the codegen checks for this sentinel and replaces it with 0:

```asm
; coerce null to zero
movz x9, #0xFFFE
movk x9, #0xFFFF, lsl #16
movk x9, #0xFFFF, lsl #32
movk x9, #0x7FFF, lsl #48
cmp x0, x9
csel x0, xzr, x0, eq      ; if x0 == sentinel, replace with 0
```

See [ARM64 Instruction Reference](arm64-instructions.md#move-and-immediate) for how `movz`/`movk` work.

## The string buffer

```asm
.comm _concat_buf, 65536    ; 64KB buffer
.comm _concat_off, 8        ; current write offset
```

The string buffer (`_concat_buf`) is a 64KB region used by all string operations — `itoa`, `ftoa`, `concat`, `strtolower`, `str_replace`, etc. It's a **bump allocator**: each operation writes its result at the current offset and advances the offset.

### How it works

```
_concat_buf:
┌──────────┬──────────┬──────────┬────────────────────┐
│  "hello" │  "42"    │  "HELLO" │  (free space)      │
└──────────┴──────────┴──────────┴────────────────────┘
 offset=0    offset=5   offset=7   _concat_off = 12
```

When `__rt_itoa` converts `42` to a string:
1. Read `_concat_off` (current position, e.g., 5)
2. Write "42" at position 5-6
3. Update `_concat_off` to 7
4. Return pointer to position 5 and length 2

### Implications

- **Strings are never freed.** Once written, they stay in the buffer forever.
- **64KB limit.** Programs that produce many strings will eventually overflow. This is a known limitation.
- **String results are temporary.** A string returned by a [runtime routine](the-runtime.md) points into this buffer. If another string operation runs, it writes further into the buffer (it doesn't overwrite earlier results, because the offset only moves forward).
- **No mutation.** You can't modify a string in place — you always create a new one in the buffer.

## The heap

```asm
.comm _heap_buf, 1048576    ; 1MB buffer
.comm _heap_off, 8          ; current allocation offset
```

The heap (`_heap_buf`) is a 1MB region for dynamically-sized data — currently only arrays. Like the string buffer, it's a bump allocator.

### How heap allocation works

The runtime routine `__rt_heap_alloc` (see [The Runtime](the-runtime.md)):

```
Request: allocate 200 bytes
1. Read _heap_off → e.g., 1024
2. Return pointer to _heap_buf + 1024
3. Set _heap_off = 1224
```

No free, no reuse, no compaction. Memory is allocated and never reclaimed.

## Array layout

Arrays are heap-allocated with a 24-byte header followed by contiguous elements:

```
_heap_buf + offset:
┌──────────┬──────────┬──────────┬──────┬──────┬──────┬─────┐
│ length   │ capacity │ elem_sz  │ [0]  │ [1]  │ [2]  │ ... │
│ (8 bytes)│ (8 bytes)│ (8 bytes)│      │      │      │     │
└──────────┴──────────┴──────────┴──────┴──────┴──────┴─────┘
 offset+0   offset+8   offset+16  offset+24  ...
```

| Field | Size | Description |
|---|---|---|
| `length` | 8 bytes | Current number of elements |
| `capacity` | 8 bytes | Number of allocated slots |
| `elem_size` | 8 bytes | Size per element: 8 (int) or 16 (string) |

### Integer arrays

Each element is 8 bytes (one `i64`):

```
Header (24 bytes) │ elem[0] (8B) │ elem[1] (8B) │ elem[2] (8B) │ ...
```

Access: `base + 24 + (index × 8)`

### String arrays

Each element is 16 bytes (pointer + length):

```
Header (24 bytes) │ ptr[0] (8B) │ len[0] (8B) │ ptr[1] (8B) │ len[1] (8B) │ ...
```

Access: `base + 24 + (index × 16)` for pointer, `base + 24 + (index × 16) + 8` for length

### Array growth

When `array_push` finds that `length == capacity`, it:
1. Allocates a new, larger buffer (typically 2× capacity)
2. Copies all existing elements to the new buffer
3. Updates the header to point to the new data
4. The old data is abandoned (never freed)

## The data section

String literals and float constants are embedded directly in the binary:

```asm
.data
_str_0: .ascii "Hello, world!\n"
_str_1: .ascii "Error: "
.align 3
_float_0: .quad 0x400921FB54442D18    ; 3.14159...
_float_1: .quad 0x4000000000000000    ; 2.0
```

- Strings are stored as raw bytes (no null terminator — length is known at compile time)
- Floats are stored as 64-bit IEEE 754 bit patterns
- Identical literals are deduplicated (two `"hello"` in source = one `_str_0` in binary)

These are **read-only** — the program never modifies them. When a string operation needs to work with a literal, it reads from the data section and writes the result to the [string buffer](#the-string-buffer).

## Memory limits and trade-offs

| Resource | Size | What happens when full |
|---|---|---|
| Stack | OS default (~8MB) | Stack overflow (crash) |
| String buffer | 64KB | Buffer overflow (undefined behavior) |
| Heap | 1MB | Heap overflow (undefined behavior) |
| Data section | No fixed limit | Grows with number of unique literals |

These limits are acceptable for educational purposes but would need to be addressed for production use:
- The string buffer could use the heap instead (with proper allocation)
- The heap could use `mmap` system calls for dynamic growth
- Both could implement proper bounds checking

## No garbage collection

elephc has no garbage collector, no reference counting, no automatic memory management. Memory is allocated and never freed. This works because:

1. **Stack memory** is automatically reclaimed when functions return
2. **Program lifetime is short** — CLI tools that run and exit don't need to reclaim memory
3. **The design is simple** — no GC pauses, no write barriers, no reference cycles to worry about

The trade-off is clear: long-running programs that produce many strings or arrays will eventually run out of buffer space. This is the biggest limitation of the current [memory model](memory-model.md) and the most obvious area for future improvement.

---

[← Back to Wiki](README.md)
