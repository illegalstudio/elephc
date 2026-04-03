---
title: "Memory Model"
description: "Stack frames, heap allocation, and memory management."
sidebar:
  order: 8
---

elephc manages memory without calling `malloc`/`free` for PHP values directly. Storage lives on the **stack** (automatic, per-function), in fixed BSS regions, or in a compiler-managed **heap buffer** with a free-list allocator, reference counting, and a targeted cycle collector for array/hash/object graphs. The final binary still links `libSystem` for OS and libc services.

This page explains where every value lives in memory at runtime.

## Runtime memory regions

```
┌─────────────────────────────┐  High addresses
│         Stack                │  ← grows downward (sp decreases)
│  (function frames, locals)   │
├─────────────────────────────┤
│         (unused)             │
├─────────────────────────────┤
│       Heap buffer            │  _heap_buf: 8MB default (--heap-size)
│  (arrays, hash tables,       │  Free-list + bump allocator
│   objects, persisted strings) │
├─────────────────────────────┤
│     String buffer            │  _concat_buf: 64KB, scratch pad
│  (temporary string results)  │  Reset at each statement
├─────────────────────────────┤
│     I/O buffers              │  _cstr_buf: 4KB × 2, _eof_flags: 256B
│  (C-string conversion, EOF)  │
├─────────────────────────────┤
│   Runtime metadata (BSS)     │  _concat_off, _global_argc/_argv,
│  (heap state, counters,      │  _heap_off, _heap_free_list,
│   globals, static storage)   │  _heap_small_bins, _gc_allocs/_frees/_live/_peak,
│                              │  _gc_collecting/_gc_release_suppressed, ...
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
│  saved x29 │  saved x30 │   $x (8B)  │   $y (8B)  │ ...
└────────────┴────────────┴────────────┴────────────┘
  [x29, #0]    [x29, #8]   [x29, #-8]   [x29, #-16]
```

- `x29` and `x30` are saved at the top of the frame (positive offsets from `x29`)
- Local variables live at **negative offsets** from `x29`
- Strings take **two slots** (16 bytes): pointer at `[x29, #-offset]`, length at `[x29, #-(offset-8)]`
- The total frame size is always 16-byte aligned (ARM64 ABI requirement)

### Variable allocation

Variables are allocated stack slots when the [code generator](the-codegen.md) scans the function body (`collect_local_vars`). The allocation is determined at compile time — there's no dynamic stack growth.

For heap-backed values, stack slots also carry compile-time ownership metadata in codegen: `Owned`, `Borrowed`, `MaybeOwned`, or `NonHeap`. This metadata is not stored in the generated binary; it only guides when codegen must retain a borrowed heap value before storing it into a new owner, and which local aliases must not be blindly decreffed yet.

| Type | Stack space | Stored as |
|---|---|---|
| `Int` | 8 bytes | Signed 64-bit value |
| `Float` | 8 bytes | IEEE 754 double |
| `Bool` | 8 bytes | 0 or 1 (stored as 64-bit for alignment) |
| `Str` | 16 bytes | 8-byte pointer + 8-byte length |
| `Array` | 8 bytes | Pointer to heap-allocated header |
| `AssocArray` | 8 bytes | Pointer to heap-allocated hash table |
| `Mixed` | 8 bytes | Pointer to heap-allocated boxed mixed cell |
| `Void` (null) | 8 bytes | Sentinel value `0x7FFFFFFFFFFFFFFE` |
| `Object` | 8 bytes | Pointer to heap-allocated object |
| `Callable` | 8 bytes | Function pointer |
| `Pointer` | 8 bytes | Raw 64-bit address |
| `Buffer` | 8 bytes | Pointer to buffer header |
| `Packed` | 8 bytes | Metadata-only nominal type, accessed via pointer |
| `Union` | 8 bytes | Boxed runtime-tagged payload (same storage as Mixed) |

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

### Pointer values

Pointers are stored as raw 64-bit addresses. An opaque pointer and a typed `ptr<T>` value have the same runtime representation; the type tag only exists in the checker. Null pointers use address `0x0`, and dereference helpers explicitly trap on null via `__rt_ptr_check_nonnull`.

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
.comm _heap_free_list, 8    ; head of the general free block linked list
.comm _heap_small_bins, 32  ; 4 x 8-byte heads for <=8/16/32/64-byte cached blocks
.comm _heap_debug_enabled, 8 ; heap-debug toggle
.comm _gc_collecting, 8     ; cycle collector re-entry guard
.comm _gc_release_suppressed, 8 ; suppress nested collection during deep free
.comm _gc_allocs, 8         ; allocation counter
.comm _gc_frees, 8          ; free counter
.comm _gc_live, 8           ; current live heap footprint in bytes
.comm _gc_peak, 8           ; heap high-water mark
```

The heap (`_heap_buf`) is an 8MB region (by default) for dynamically-sized data — arrays, hash tables, objects, and persisted strings. It uses a **free-list + bump hybrid allocator** with segregated small-block bins for the hottest tiny allocations.

### How heap allocation works

Every allocation has a **16-byte header**: two 32-bit fields for block size and reference count, followed by an 8-byte uniform heap-kind tag:

```
┌───────────┬────────────┬────────────┬──────────────────┐
│ size (4B) │ refcnt (4B)│ kind (8B)  │  user data ...   │
└───────────┴────────────┴────────────┴──────────────────┘
       header (16 bytes total)          ← pointer returned to caller
```

The size is stored at header offset `+0`, the reference count at `+4`, and the heap kind tag at `+8`. New allocations start with refcount `1`; typed constructors then stamp the kind as `1=string`, `2=indexed array`, `3=assoc/hash`, `4=object`, `5=boxed mixed`, while raw helper buffers remain `0`. For array/hash containers, the low 16 bits of the kind word are persistent metadata: the low byte is still the heap kind, indexed arrays still pack their runtime `value_type` in the next byte, and bit 15 is reserved as the persistent copy-on-write container flag. Higher bits remain transient collector metadata.

The runtime routine `__rt_heap_alloc`:

1. **Probe the segregated small bins** — requests up to 64 bytes first check `_heap_small_bins` (`<=8`, `<=16`, `<=32`, `<=64`) and reuse a cached block from the smallest fitting class available.
2. **Walk the general free list** — if no cached small block fits, check the address-ordered free list (first-fit). If a block with `size >= requested` is found, either unlink it whole or split it so the remainder stays on the free list, then reset the allocated block's refcount to 1 and return it.
3. **Bump allocate** — if neither free path fits, allocate from the end of the heap: write size and refcount=1 to the header, advance `_heap_off`, return user pointer.
4. **Bounds check** — if the bump would exceed `_heap_max`, print a fatal error and exit.

Minimum allocation is 8 bytes (to fit the next pointer when the block is later freed).

### How heap freeing works

The runtime routine `__rt_heap_free`:

1. Read the block size (32-bit) from the 16-byte header at `user_pointer - 16`
2. If the block is exactly at the bump tail, shrink `_heap_off` immediately
3. Otherwise, payloads up to 64 bytes are cached into one of four segregated small-bin heads (`<=8`, `<=16`, `<=32`, `<=64`) so later tiny allocations can reuse them without scanning the larger free list
4. Larger non-tail blocks are inserted into the general free list in address order, merged with adjacent free neighbors, and repeatedly trim any now-free tail chain back into `_heap_off`
5. Free blocks reuse the same 16-byte header, clear the kind back to `0`, and then store the next pointer immediately after it: `[size:4][refcnt:4][kind:8][next_ptr:8][...unused...]`

The variant `__rt_heap_free_safe` validates that the pointer is within `_heap_buf` range before freeing — safe to call with garbage, null, or `.data` section pointers.

### Heap debug mode

Passing `--heap-debug` enables additional runtime verification without changing normal ownership behavior:

- `__rt_heap_free` rejects duplicate insertion of the same block into the free list (`double free`)
- `__rt_incref` / `__rt_decref_*` reject zero-refcount heap blocks before mutating them (`bad refcount`)
- `__rt_heap_alloc` / `__rt_heap_free` validate the ordered free list plus the segregated small-bin chains and trap on out-of-range, overlapping, cyclic, mis-sized, or merely-adjacent free blocks (`free-list corruption`)
- `__rt_heap_free` poisons freed payload bytes with `0xA5`, so stale raw reads stand out immediately in debug repros
- process exit prints a heap-debug summary with alloc/free counts, live blocks, live bytes, a leak summary line, and the peak live-byte watermark

When one of these checks trips, the program exits with a fatal heap-debug error instead of continuing with corrupted allocator state.

### When memory is freed

- **Variable reassignment**: when a heap-backed local/global/static slot is overwritten, codegen releases the previous owner through the appropriate runtime path (`__rt_heap_free_safe` for persisted strings, `__rt_decref_*` for refcounted arrays / hashes / objects)
- **`unset()`**: releases the current heap-backed value before nulling the slot
- **Targeted cycle collection**: when decref reaches a container/object graph that may only be keeping itself alive, `__rt_gc_collect_cycles` counts heap-only incoming edges, marks externally reachable blocks, and deep-frees the remaining unreachable array/hash/object island
- **Process exit**: all memory is reclaimed by the OS

### Configurable heap size

The default heap is 8MB. For programs that need more (or less), use:

```bash
elephc --heap-size=16777216 heavy.php    # 16MB heap
elephc --gc-stats heavy.php              # print alloc/free counters to stderr
elephc --heap-debug heavy.php            # enable runtime heap verification
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

1. `__rt_array_grow` first runs `__rt_array_ensure_unique`, so shared arrays split before reallocation
2. Allocates a new array with **2× capacity** (minimum 8)
3. Copies the 24-byte header and all elements to the new array
4. Frees the previous unique storage and returns the new array pointer

The caller updates its stored pointer to the new array. This means arrays are truly dynamic — you can push unlimited elements (limited only by heap size). Direct indexed writes into empty arrays now also grow the backing storage and extend `length` to cover the highest written index.

### Copy-on-write containers

Indexed arrays and associative arrays now follow **shared-until-modified** semantics:

1. Plain assignment or by-value argument passing shares the existing heap container and bumps its refcount
2. The first mutating write runs `__rt_array_ensure_unique` or `__rt_hash_ensure_unique`
3. If the refcount is already 1, the write proceeds in place
4. If the refcount is greater than 1, the runtime clones the container structure, retains nested heap-backed children (or re-persists immutable strings/keys), decrements the mutator's old owner slot, rewrites the mutating owner to the clone, and only then performs the write

This is what lets PHP-style code such as `$b = $a; $b[0] = 9;` leave `$a` unchanged without requiring deep copies on every assignment. Nested arrays and hashes remain shallow-shared until their own first mutation.

## Hash table layout (associative arrays)

Associative arrays use a separate heap-allocated structure: an open-addressing hash table for lookup plus an insertion-order linked list threaded through the entries.

### Header (40 bytes)

```
┌──────────┬──────────┬──────────┬──────────┬──────────┐
│  count   │ capacity │ val_type │   head   │   tail   │
│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│
└──────────┴──────────┴──────────┴──────────┴──────────┘
 offset+0   offset+8   offset+16  offset+24  offset+32
```

| Field | Size | Description |
|---|---|---|
| `count` | 8 bytes | Number of occupied entries |
| `capacity` | 8 bytes | Total number of slots |
| `val_type` | 8 bytes | Coarse value-type summary (0=int, 1=str, 2=float, 3=bool, 4=array, 5=assoc, 6=object, 7=mixed) |
| `head` | 8 bytes | Slot index of the first inserted entry, or `-1` when empty |
| `tail` | 8 bytes | Slot index of the most recently inserted entry, or `-1` when empty |

### Entries (64 bytes each)

Starting at offset +40, each slot is 64 bytes:

```
┌──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┐
│ occupied │ key_ptr  │ key_len  │ value_lo │ value_hi │ value_tag│   prev   │   next   │
│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│
└──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┘
```

| Field | Description |
|---|---|
| `occupied` | 0 = empty, 1 = occupied, 2 = tombstone (deleted) |
| `key_ptr` | Pointer to key string bytes |
| `key_len` | Key string length |
| `value_lo` | Value (integer) or value pointer (string) |
| `value_hi` | String length (for string values), unused for single-word payloads |
| `value_tag` | Authoritative per-entry runtime tag used by lookup, iteration, JSON, search, and GC |
| `prev` | Previous inserted slot index, or `-1` for the head entry |
| `next` | Next inserted slot index, or `-1` for the tail entry |

### Hashing and collision resolution

Keys are hashed with **FNV-1a** (fast, good distribution for short strings). Collisions are resolved by **linear probing** — if slot `hash % capacity` is occupied, try `(hash + 1) % capacity`, and so on.

Entry address: `base + 40 + (slot_index × 64)`

### Iteration order

Lookup still probes physical buckets, but iteration walks the `head -> next -> ... -> tail` chain. The header `val_type` is only a summary now; correctness-critical paths read each entry's `value_tag`. This preserves PHP insertion order across:

- `foreach` on associative arrays
- `array_keys()` and `array_values()`
- `array_search()` / `in_array()` when duplicate values exist
- `json_encode()` on associative arrays
- Rehashing during growth and copy-on-write cloning

### Comparison with indexed arrays

| | Indexed array | Associative array |
|---|---|---|
| Header | 24 bytes | 40 bytes |
| Element size | 8 or 16 bytes | 64 bytes (fixed) |
| Access | O(1) by index | O(1) average by hash |
| Iteration | Sequential | Insertion-order linked walk over occupied slots |
| Keys | Implicit (0, 1, 2, ...) | Explicit strings |

## Object layout

Objects are heap-allocated with a fixed layout determined at compile time. Each object starts with an 8-byte class identifier, followed by 16 bytes per property across the full inheritance chain:

```
_heap_buf + offset:
┌──────────┬──────────────────┬──────────────────┬─────┐
│ class_id │   prop[0] (16B)  │   prop[1] (16B)  │ ... │
│ (8 bytes)│                  │                  │     │
└──────────┴──────────────────┴──────────────────┴─────┘
 offset+0    offset+8           offset+24          ...
```

| Field | Size | Description |
|---|---|---|
| `class_id` | 8 bytes | Identifies which class this object belongs to |
| `prop[n]` | 16 bytes | Property value (16 bytes regardless of type, for uniform offsets) |

Total object size: `8 + (num_properties × 16)`

Property access is O(1) — the compiler resolves each property's final inherited offset at compile time and emits a direct load/store at that offset. No runtime lookup or hash table is needed.

Unlike arrays, objects are not resizable. The number of properties is fixed by the class declaration. Properties are stored in parent-first order, then by the child class's own declarations.

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

The runtime data layer is split into `emit_runtime_data_fixed()` (shared buffers, diagnostics, lookup tables) and `emit_runtime_data_user()` (per-program globals, statics, enum-case slots, and metadata). Together they emit these static data tables:
- `_fmt_g` — printf format string for float-to-string conversion (`%.14G`)
- `_b64_encode_tbl` — 64-byte Base64 encoding lookup table
- `_b64_decode_tbl` — 256-byte Base64 decoding lookup table
- `_heap_err_msg`, `_arr_cap_err_msg`, `_ptr_null_err_msg` — fatal runtime error strings
- `_buffer_bounds_msg`, `_buffer_uaf_msg`, `_match_unhandled_msg`, `_enum_from_msg` — fatal runtime error strings for buffers, `match`, and enums
- `_pcre_space`, `_pcre_digit`, `_pcre_word`, `_pcre_nspace`, `_pcre_ndigit`, `_pcre_nword` — regex shorthand replacement strings used by the POSIX regex bridge
- `_json_true`, `_json_false`, `_json_null` — JSON keyword strings (4, 5, and 4 bytes) used by `json_encode` for boolean and null values
- `_day_names` — 84-byte table (7 entries x 12 bytes each) with day names, lengths, and padding. Used by `date()` for day-of-week formatting
- `_month_names` — 144-byte table (12 entries x 12 bytes each) with month names, lengths, and padding. Used by `date()` for month formatting
- `_class_gc_desc_count`, `_class_gc_desc_ptrs`, `_class_gc_desc_<id>` — per-class property traversal descriptors used by object deep-free and cycle collection
- `_class_vtable_ptrs`, `_class_vtable_<id>` — per-class virtual tables used for inherited instance-method dispatch
- `_class_static_vtable_ptrs`, `_class_static_vtable_<id>` — per-class static-method tables used for late static binding
- enum-case `.comm` symbols produced via `enum_case_symbol(...)` — one 8-byte singleton storage slot per declared enum case

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

### Enum case singleton storage

User-defined enums also contribute BSS storage. During `emit_runtime_data_user()`, the compiler emits one 8-byte `.comm` symbol per declared case, using the mangled name from `enum_case_symbol(...)`.

These slots let enum cases behave like stable singleton values at runtime: codegen can load the canonical case address directly, and helper paths such as `Enum::from()` can compare or return those canonical case objects without constructing ad-hoc heap values.

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
| Heap metadata | `_heap_off`, `_heap_free_list`, `_heap_small_bins`, `_heap_debug_enabled`, `_gc_*` flags/counters = 104 bytes total | Fixed-size bookkeeping, not user-visible |
| CLI globals | `_global_argc`, `_global_argv` = 16 bytes total | Fixed-size bookkeeping |
| User globals | 16 bytes per `global $var` slot | Grows with number of referenced globals |
| Static vars | 24 bytes per `static $var` (`16 + 8 init flag`) | Grows with number of declared static locals |
| Array capacity | Fixed at creation until grow/re-hash logic runs | Fatal error: "array capacity exceeded" if a hard limit is hit |
| C-string buffers | 4KB each (×2) | Long converted paths/strings are truncated to buffer size |
| EOF flags | 256 bytes | Max 256 simultaneous file descriptors |
| Data section | No fixed limit | Grows with number of unique literals |

## Memory management strategy

elephc uses a **free-list allocator with reference counting plus a targeted cycle collector** — not pure bump-allocation, and not a whole-heap tracing runtime either. Memory is reclaimed in specific situations:

1. **Reference counting** — every heap allocation carries a 32-bit refcount (initialized to 1). When a reference is shared, `__rt_incref` increments it. When a reference is dropped, `__rt_decref_array`, `__rt_decref_hash`, or `__rt_decref_object` decrements it and frees the block when it reaches zero
2. **Copy-on-write splitting for arrays/hashes** — plain assignment still shares container storage, but the first mutating write clones a shared container before modifying it
3. **Codegen ownership tracking** — locals, globals, statics, `foreach` variables, `list(...)` targets, and call arguments are classified as owned or borrowed at compile time so new owners retain borrowed heap values before storing them
4. **Variable reassignment** — when `$x = "new value"` overwrites a string or array, the old heap block is freed or decreffed and returned to the allocator for reuse
5. **`unset($x)`** — explicitly frees the variable's heap allocation
6. **FFI string-call cleanup** — temporary C strings created for `extern function foo(string $s)` calls are released immediately after the native call returns
7. **String buffer reset** — the concat buffer resets at each statement, with strings that need to survive copied to heap via `__rt_str_persist`
8. **Stack memory** — automatically reclaimed when functions return
9. **Process exit** — all memory reclaimed by the OS

### What is NOT freed

- **Non-adjacent free blocks** are still not compacted — fragmentation can still occur over time even though adjacent neighbors are coalesced on free and oversized free blocks are split on allocation
- **Pointer targets** are not ownership-tracked just because a raw pointer exists; the pointer value itself is only an address
- **Intermediate scratch strings** in `_concat_buf` are not individually freed — the buffer is simply reset per statement
- **General function epilogues** do not blanket-decref all heap locals. They now selectively clean up slots proven `Owned`, and exhaustive `if` / `elseif` / `else` branches can restore that cleanup when every fallthrough branch directly assigns the same heap-backed type to the same local. More dynamic borrowed/control-flow paths still remain excluded on purpose
- **Container-copying builtins** no longer blindly duplicate borrowed heap handles for common nested payload paths: refcounted runtime variants now retain values before new arrays/hash tables take ownership (`array` literals with spreads, `array_merge`, `array_chunk`, `array_slice`, `array_reverse`, `array_pad`, `array_unique`, `array_splice`, `array_diff`, `array_intersect`, `array_filter`, `array_fill`, `array_combine`, `array_fill_keys`)
- **Regression coverage now explicitly exercises** local aliases, borrowed nested-container returns, `Owned`/`Borrowed` control-flow merges, and scope-exit paths so future ownership work has focused tripwires instead of relying only on large end-to-end suites
- **Raw/off-heap ownership cycles** are still outside the collector. `ptr` values, extern-managed buffers, and raw helper allocations (`kind=0`) are not traversed just because an address exists somewhere

### Targeted cycle collection

The runtime now includes a targeted collector for heap-backed `array`, associative-array/hash, and `object` graphs:

- the allocator header carries a uniform heap-kind tag (`string`, `array`, `hash`, `object`, raw)
- indexed arrays pack their runtime `value_type` into the same kind word so the collector knows whether their elements can contain nested heap pointers
- objects record runtime property tags/metadata, with `_class_gc_desc_*` tables as a compile-time fallback for property traversal
- mixed release paths use `__rt_decref_any`, so deep-free and GC walks can release nested strings/arrays/hashes/objects through one uniform dispatcher

`__rt_gc_collect_cycles` is intentionally narrower than a full tracing GC: it ignores strings and raw helper buffers, clears transient metadata, counts heap-only incoming edges, marks externally reachable container/object blocks, then frees the unmarked remainder with deep-release helpers. That keeps the collector focused on the structural leak class that plain refcounting cannot solve without turning the whole runtime into a moving or stop-the-world heap.

### Performance characteristics

For a loop like `for ($i = 0; $i < 1000; $i++) { $s .= "x"; }`:
- Each iteration frees the old `$s` and allocates a new one
- Old blocks go to the free list, new blocks come from bump allocation (growing size)
- Net heap usage is O(N) for the final string, not O(N²)
- With 8MB heap, this handles thousands of iterations comfortably
