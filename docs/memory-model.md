# Memory Model

[← Back to Wiki](README.md) | Previous: [The Runtime](the-runtime.md)

---

elephc manages memory without a garbage collector, without `malloc`/`free`, and without any runtime library. Everything is either on the **stack** (automatic, per-function) or in a **heap buffer** with a free-list allocator.

This page explains where every value lives in memory at runtime.

## The four memory regions

```
┌─────────────────────────────┐  High addresses
│         Stack                │  ← grows downward (sp decreases)
│  (function frames, locals)   │
├─────────────────────────────┤
│         (unused)             │
├─────────────────────────────┤
│       Heap buffer            │  _heap_buf: 8MB default (--heap-size)
│  (arrays, hash tables,       │  Free-list + bump allocator
│   persisted strings)         │
├─────────────────────────────┤
│     String buffer            │  _concat_buf: 64KB, scratch pad
│  (temporary string results)  │  Reset at each statement
├─────────────────────────────┤
│     I/O buffers              │  _cstr_buf: 4KB × 2, _eof_flags: 256B
│  (C-string conversion, EOF)  │
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

## The string buffer (scratch pad)

```asm
.comm _concat_buf, 65536    ; 64KB scratch buffer
.comm _concat_off, 8        ; current write offset (reset per statement)
```

The string buffer (`_concat_buf`) is a 64KB scratch region used by all string operations — `itoa`, `ftoa`, `concat`, `strtolower`, `str_replace`, etc. Each operation writes its result at the current offset and advances the offset.

**The buffer is reset to offset 0 at the start of every statement.** This means strings in the buffer are temporary — they only live for the duration of one statement's evaluation.

### How it works

Within a single statement like `echo strtolower("HELLO") . " " . $name;`:

```
_concat_buf:
┌──────────┬──────────┬──────────────┬──────────────────┐
│  "hello" │  " "     │  "hello Joe" │  (free space)    │
└──────────┴──────────┴──────────────┴──────────────────┘
 offset=0    offset=5   offset=6      _concat_off = 17
```

Each sub-expression writes its result further into the buffer. After the statement completes (echo writes to stdout), the next statement resets `_concat_off` to 0.

### Copy-on-store

When a string result is stored to a variable (e.g., `$x = "a" . "b";`), the codegen calls `__rt_str_persist` which copies the string from the concat buffer to the **heap**. This ensures:

- **Variables always point to heap memory**, never into the scratch buffer
- **The buffer can safely reset** without invalidating stored values
- **Hash table keys** are also persisted to heap (via `str_persist`)

### Implications

- **No overflow.** Because the buffer resets each statement, only one statement's worth of string operations need to fit in 64KB.
- **No mutation.** You can't modify a string in place — you always create a new one.
- **Scratch only.** The buffer is strictly temporary. Anything that needs to survive goes to the heap.

## The heap

```asm
.comm _heap_buf, 8388608    ; 8MB buffer (configurable via --heap-size)
.comm _heap_off, 8          ; current bump allocation offset
.comm _heap_free_list, 8    ; head of free block linked list
```

The heap (`_heap_buf`) is an 8MB region (by default) for dynamically-sized data — arrays, hash tables, and persisted strings. It uses a **free-list + bump hybrid allocator**.

### How heap allocation works

Every allocation has an **8-byte header** storing the block size:

```
┌──────────┬──────────────────┐
│ size (8B)│  user data ...   │
└──────────┴──────────────────┘
  header     ← pointer returned to caller
```

The runtime routine `__rt_heap_alloc`:

1. **Walk the free list** — check each freed block (first-fit). If a block with `size >= requested` is found, unlink it and return it.
2. **Bump allocate** — if no free block fits, allocate from the end of the heap: write header, advance `_heap_off`, return user pointer.
3. **Bounds check** — if the bump would exceed `_heap_max`, print a fatal error and exit.

Minimum allocation is 8 bytes (to fit the next pointer when the block is later freed).

### How heap freeing works

The runtime routine `__rt_heap_free`:

1. Read the block header at `user_pointer - 8` to get the size
2. Insert the block at the head of the free list (LIFO)
3. Free blocks have layout: `[size:8][next_ptr:8][...unused...]`

The variant `__rt_heap_free_safe` validates that the pointer is within `_heap_buf` range before freeing — safe to call with garbage, null, or `.data` section pointers.

### When memory is freed

- **Variable reassignment**: when a string or array variable is overwritten, the old value is freed via `__rt_heap_free_safe`
- **`unset()`**: frees the variable's heap allocation before nulling it
- **Process exit**: all memory is reclaimed by the OS

### Configurable heap size

The default heap is 8MB. For programs that need more (or less), use:

```bash
elephc --heap-size=16777216 heavy.php    # 16MB heap
```

The minimum is 64KB.

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

When `array_push` finds that `length >= capacity`, the array grows automatically:

1. `__rt_array_grow` allocates a new array with **2× capacity** (minimum 8)
2. Copies the 24-byte header and all elements to the new array
3. Frees the old array via `__rt_heap_free`
4. Returns the new array pointer

The caller updates its stored pointer to the new array. This means arrays are truly dynamic — you can push unlimited elements (limited only by heap size).

## Hash table layout (associative arrays)

Associative arrays use a separate heap-allocated structure — an open-addressing hash table with linear probing.

### Header (24 bytes)

```
┌──────────┬──────────┬──────────┐
│  count   │ capacity │ val_type │
│ (8 bytes)│ (8 bytes)│ (8 bytes)│
└──────────┴──────────┴──────────┘
 offset+0   offset+8   offset+16
```

| Field | Size | Description |
|---|---|---|
| `count` | 8 bytes | Number of occupied entries |
| `capacity` | 8 bytes | Total number of slots |
| `val_type` | 8 bytes | Value type tag (0=int, 1=str, 2=float, 3=bool) |

### Entries (40 bytes each)

Starting at offset +24, each slot is 40 bytes:

```
┌──────────┬──────────┬──────────┬──────────┬──────────┐
│ occupied │ key_ptr  │ key_len  │ value_lo │ value_hi │
│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│
└──────────┴──────────┴──────────┴──────────┴──────────┘
```

| Field | Description |
|---|---|
| `occupied` | 0 = empty, 1 = occupied, 2 = tombstone (deleted) |
| `key_ptr` | Pointer to key string bytes |
| `key_len` | Key string length |
| `value_lo` | Value (integer) or value pointer (string) |
| `value_hi` | String length (for string values), unused for int |

### Hashing and collision resolution

Keys are hashed with **FNV-1a** (fast, good distribution for short strings). Collisions are resolved by **linear probing** — if slot `hash % capacity` is occupied, try `(hash + 1) % capacity`, and so on.

Entry address: `base + 24 + (slot_index × 40)`

### Comparison with indexed arrays

| | Indexed array | Associative array |
|---|---|---|
| Header | 24 bytes | 24 bytes |
| Element size | 8 or 16 bytes | 40 bytes (fixed) |
| Access | O(1) by index | O(1) average by hash |
| Iteration | Sequential | Scan for occupied slots |
| Keys | Implicit (0, 1, 2, ...) | Explicit strings |

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

The runtime also emits static data tables:
- `_fmt_g` — printf format string for float-to-string conversion (`%.14G`)
- `_b64_encode_tbl` — 64-byte Base64 encoding lookup table
- `_b64_decode_tbl` — 256-byte Base64 decoding lookup table
- `_json_true`, `_json_false`, `_json_null` — JSON keyword strings (4, 5, and 4 bytes) used by `json_encode` for boolean and null values
- `_day_names` — 84-byte table (7 entries x 12 bytes each) with day names, lengths, and padding. Used by `date()` for day-of-week formatting
- `_month_names` — 144-byte table (12 entries x 12 bytes each) with month names, lengths, and padding. Used by `date()` for month formatting

### Global variables

Two 8-byte BSS slots store the program's command-line arguments:

```asm
.comm _global_argc, 8       ; saved argc from OS
.comm _global_argv, 8       ; saved argv pointer from OS
```

These are written once in `_main` (from the OS-provided `x0` and `x1`) and read by the `__rt_build_argv` routine to construct `$argv`.

### User global variables (`global $var`)

When a function uses `global $var`, the compiler allocates BSS storage for that variable:

```asm
.comm _gvar_x, 16, 3        ; 16 bytes for global $x (enough for string ptr+len or int/float)
.comm _gvar_y, 16, 3        ; 16 bytes for global $y
```

Each global variable gets 16 bytes of BSS storage (enough to hold any PHP value). The `_main` scope writes to these slots when assigning to variables that any function declares as `global`, and functions read/write through these slots instead of using local stack slots.

### Static variables (`static $var`)

Static variables persist their value across calls to the same function. Each static variable gets two BSS allocations:

```asm
.comm _static_counter_count, 16, 3    ; 16 bytes for the persisted value
.comm _static_counter_count_init, 8, 3 ; 8-byte init flag (0 = uninitialized)
```

The naming pattern is `_static_FUNCNAME_VARNAME`. The init flag ensures the initial value expression is evaluated only on the first call. At function epilogue, variables marked as static are saved back to their BSS storage.

## Memory limits and trade-offs

| Resource | Size | What happens when full |
|---|---|---|
| Stack | OS default (~8MB) | Stack overflow (crash) |
| String buffer | 64KB | Resets each statement — effectively unlimited |
| Heap | 8MB (configurable) | Fatal error: "heap memory exhausted" |
| Array capacity | Fixed at creation | Fatal error: "array capacity exceeded" |
| C-string buffers | 4KB each (×2) | Truncation of file paths |
| EOF flags | 256 bytes | Max 256 simultaneous file descriptors |
| Data section | No fixed limit | Grows with number of unique literals |

## Memory management strategy

elephc uses a **free-list allocator** — not a garbage collector, but not pure bump-allocation either. Memory is reclaimed in specific situations:

1. **Variable reassignment** — when `$x = "new value"` overwrites a string or array, the old heap block is freed and returned to the free list for reuse
2. **`unset($x)`** — explicitly frees the variable's heap allocation
3. **String buffer reset** — the concat buffer resets at each statement, with strings that need to survive copied to heap via `__rt_str_persist`
4. **Stack memory** — automatically reclaimed when functions return
5. **Process exit** — all memory reclaimed by the OS

### What is NOT freed

- **Array elements** — freeing an array frees the array structure but not strings inside it (shallow free)
- **Adjacent free blocks** are not coalesced — fragmentation can occur over time
- **Intermediate allocations** within a single expression — only the final result is persisted

### Performance characteristics

For a loop like `for ($i = 0; $i < 1000; $i++) { $s .= "x"; }`:
- Each iteration frees the old `$s` and allocates a new one
- Old blocks go to the free list, new blocks come from bump allocation (growing size)
- Net heap usage is O(N) for the final string, not O(N²)
- With 8MB heap, this handles thousands of iterations comfortably

---

[← Back to Wiki](README.md)
