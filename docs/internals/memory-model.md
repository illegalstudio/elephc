---
title: "Memory Model"
description: "Stack frames, heap allocation, and memory management."
sidebar:
  order: 10
---

elephc manages memory without calling `malloc`/`free` for PHP values directly. Storage lives on the **stack** (automatic, per-function), in fixed BSS regions, or in a compiler-managed **heap buffer** with a free-list allocator, reference counting, and a targeted cycle collector for array/hash/object graphs. The final binary still links the target platform's system C library for OS and libc services.

This page explains where every value lives in memory at runtime.

For `--emit phpext`, each PHP request owns a snapshot of these native writable
regions. The adapter restores snapshots at stable addresses under a module-local
mutex, preserving internal pointers while isolating request heaps and statics.
Native strings and arrays are copied through private transport storage, then
converted using real Zend APIs. Zend allocations and native allocations never
share ownership. Snapshot buffers and partial transport allocations are freed
at their request/call boundaries. See [PHP extensions](../beyond-php/phpext.md).

## Runtime memory regions

```
┌─────────────────────────────┐  High addresses
│         Stack                │  ← grows downward (sp decreases)
│  (function frames, locals)   │
├─────────────────────────────┤
│         (unused)             │
├─────────────────────────────┤
│       Heap buffer            │  _heap_buf: 128MB default (--heap-size)
│  (arrays, hash tables,       │  Free-list + bump allocator
│   objects, persisted strings) │
├─────────────────────────────┤
│ Buffer descriptor registry   │  _buffer_registry: 4097 x 48B
│  (generation-safe handles)   │  index 0 reserved; 4096 usable slots
├─────────────────────────────┤
│     String buffer            │  _concat_buf: heap-sized, scratch pad
│  (temporary string results)  │  Reset at each statement
├─────────────────────────────┤
│   Output capture buffers     │  _print_r_buf: 64KB inline return-mode capture,
│  (print_r return mode, ob_*  │  _ob_ptrs/_lens/_caps + handler tables:
│   buffer-stack bookkeeping)  │  64 slots x 8B each (ob buffer bodies
│                              │  are heap-allocated)
├─────────────────────────────┤
│     I/O and stream buffers   │  _cstr_buf/_cstr_buf2, _eof_flags,
│  (C strings, streams, TLS,   │  _stream_*, _http_*, _ftp_*, wrapper/filter
│   wrappers, filters)         │  tables, protocol/service/principal lookup buffers
├─────────────────────────────┤
│ Runtime metadata (BSS/data)  │  _concat_off, _global_argc/_argv,
│  (heap state, counters,      │  _heap_off, _heap_free_list,
│   globals, static storage)   │  _heap_small_bins, _heap_debug_enabled,
│                              │  _buffer_registry_free/_next,
│                              │  _gc_allocs/_frees/_live/_peak,
│                              │  _gc_* lifecycle, timing, and pin state,
│                              │  _exc_handler_top, _exc_call_frame_top,
│                              │  _exc_value, _php_*_handler_*,
│                              │  _magic_set_guard_head,
│                              │  _rt_diag_suppression, _rt_diag_pending_*,
│                              │  _php_diagnostic_*,
│                              │  _rt_unmanaged_ref_borrow_top,
│                              │  _obj_handle_index, _obj_handle_free/_top,
│                              │  _obj_weak_reference_index,
│                              │  _resource_inventory_head/_tail,
│                              │  _json_last_error, _json_active_*,
│                              │  _json_indent_depth, _json_validate_*,
│                              │  _json_decode_assoc, _json_error_*,
│                              │  _fiber_current, _fiber_main_saved_*,
│                              │  _generator_class_id,
│                              │  _print_r_mode/_print_r_off,
│                              │  _ob_level/_ob_in_handler/_ob_flushing,
│                              │  elephc_web_capture,
│                              │  _include_once_*, _fn_variant_active_*,
│                              │  _elephc_crypto_*_fn, _elephc_bcmath_*_fn,
│                              │  _elephc_tls_*_fn,
│                              │  _elephc_eval_*_fn, ...
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

On the EIR backend the frame also reserves one save slot per callee-saved register that the linear-scan register allocator uses. Eligible scalar PHP locals are promoted to SSA values, so their repeated loads and stores can disappear and loop-carried values can live in registers. The frame still reserves local slots, and address-taken, by-reference, global/static, refcounted, and volatile locals continue to use them. See [The IR](the-ir.md) for scalar promotion and register allocation.

### Frame-local numeric cells

Checked integer add/subtract/multiply results may use a 24-byte `mixed` cell in
the current stack frame when codegen proves the entire lifetime cannot escape.
The cell has the ordinary runtime tag and payload but no heap header, and it
must never reach a heap release or outlive its producing activation. Its
overflow branch computes the operation's float result from the original
operands. References, `eval`, suspension, delayed reads and unknown consumers
retain ordinary heap storage. See
[Frame-local checked numeric cells](the-codegen.md#frame-local-checked-numeric-cells).

### Variable allocation

EIR lowering declares PHP locals and hidden temporary slots before codegen.
`src/codegen/frame.rs::layout_for_function()` then assigns their fixed offsets
from the EIR metadata, alongside SSA backing slots, frame-local numeric cells
and saved registers. The allocation is determined at compile time; there is no
dynamic stack growth for these slots.

For heap-backed values, stack slots also carry compile-time ownership metadata in codegen: `Owned`, `Borrowed`, `MaybeOwned`, or `NonHeap`. This metadata is not stored in the generated binary; it only guides when codegen must retain a borrowed heap value before storing it into a new owner, and which local aliases must not be blindly decreffed yet.

| Type | Stack space | Stored as |
|---|---|---|
| `Int` | 8 bytes | Signed 64-bit value |
| `Float` | 8 bytes | IEEE 754 double |
| `Bool` | 8 bytes | 0 or 1 (stored as 64-bit for alignment) |
| `Str` | 16 bytes | 8-byte pointer + 8-byte length |
| `Array` | 8 bytes | Pointer to heap-allocated header |
| `AssocArray` | 8 bytes | Pointer to heap-allocated hash table |
| `Iterable` | 8 bytes | Type-erased heap pointer for arrays or Traversable objects |
| `Mixed` | 8 bytes | Pointer to a boxed Mixed cell, normally heap-allocated |
| `Void` (null) | 8 bytes | Sentinel value `0x7FFFFFFFFFFFFFFE` |
| `Object` | 8 bytes | Pointer to heap-allocated object |
| `Callable` | 8 bytes | Function pointer |
| `Pointer` | 8 bytes | Raw 64-bit address |
| `Resource` | 8 bytes | Native resource payload, such as a stream descriptor |
| `Buffer` | 8 bytes | Opaque `(generation:u32 << 32) \| descriptor_index:u32` handle |
| `Packed` | 8 bytes | Metadata-only nominal type, accessed via pointer |
| `Union` | 8 bytes | Boxed runtime-tagged payload (same storage as Mixed) |
| `TaggedScalar` | 16 bytes | 8-byte payload + 8-byte runtime tag (tagged null representation) |

### Nonescaping checked numeric results

Checked integer add, subtract and multiply can use a 24-byte cell reserved in the
current function's frame. The cell retains the ordinary Mixed tag and two payload
words. Overflow converts both original integers to double and writes a float tag;
the optimization does not narrow the arithmetic domain or replace promotion with
a fatal error. Frame sizing and instruction emission support all five targets.

Transient producers in one block may reuse a cell after its last payload read,
including reads through transparent ownership copies. A numeric emitter reads
both inputs before writing its result, allowing reuse at that boundary. Values
stored in any local retain dedicated cells, and different blocks do not share.
This limits additional stack use to simultaneously live transient values instead
of the total number of operations in a long expression.

`src/codegen/stack_numeric.rs` proves the complete use graph before frame layout.
Only same-block scalar observations, closed numeric producers, ownership
scaffolding and one ordinary local or owned operand slot may consume a selected
pointer. Declared integer returns copy the payload and retain their type check.
Every access to the owning slot must preserve the numeric storage contract. A
borrowed pointer must be converted before that slot is stored or released, or any
producer that can back it executes again. Independent cells may intervene.
Published operand slots require one same-block store, balanced owner records and
final retirement. Their cleanup records remain intact when a property read can
throw, and the dispatcher rejects frame addresses. Returns of the cell,
pointer-valued block arguments, a second
local, container stores and unknown consumers retain heap storage. Parameters,
globals, statics, references, eval storage and suspended activations do not receive
this local storage optimization. Private slots in the executable main frame use
the same complete lifetime proof; global-visible values still retain heap storage.

Loop-carried numeric slots are lowered with their final Mixed representation from
entry, including arithmetic results copied through another local. Closed numeric
add/subtract/multiply and increment inputs preserve an existing float tag on later
iterations. Counters retain scalar lowering only after a discarded SSA/range trial
proves every update safe; mutually observed counters are proved together, and nested
loop invariants are simplified before the range check. Unknown aliases and dynamic
scope access reject the proof. The proof is per frame slot, and a slot is per
binding: a straight-line reassignment between two different numeric types starts a
new binding (see [Local retyping](the-type-checker.md#local-retyping-and-strict-locals-mode)),
so a counter name reused by a later loop with a boxed seed does not widen the slot of
an earlier, provably integer loop.

These frame cells have no heap header and cannot own children. Proven retains and
releases of their SSA pointers emit no refcount calls. A selected local may still
hold a numeric heap seed; its direct Mixed cleanup releases that seed and ignores
frame addresses through the runtime's existing heap-range check. This avoids a
protected destructor callback for values that cannot execute PHP cleanup code.

### Eval subclass property storage

In eval-enabled modules, non-final user classes reserve a GC-visible hash pointer
after their fixed property slots. Eval-declared subclasses use that hash for
additional fields while retaining the native parent's existing field offsets.
The runtime layout metadata includes the tail in allocation, cloning, deep release,
and cycle traversal on every supported target.

This storage capability is separate from `#[AllowDynamicProperties]`. Ordinary
native instances do not gain dynamic-property permission: fallback access to the
extra hash requires a registered eval object. Declared native properties retain
their usual visibility checks. Hash entries own retained boxed cells, and writes
publish COW replacements through the owning hash slot.

### Null representations

elephc has two representations for PHP `null` in scalar slots, selected per compilation by
`--null-repr=sentinel|tagged` (or `ELEPHC_NULL_REPR`). The tagged representation is the
default; the sentinel is the compatibility opt-out.

#### The in-band sentinel (compatibility opt-out)

`null` is represented as the integer `0x7FFFFFFFFFFFFFFE` (`PHP_INT_MAX - 1`). Because every
64-bit pattern is a valid PHP int, this sentinel collides with the real integer
`9223372036854775806`, which is misread as `null` by null checks. Before arithmetic
operations, the codegen checks for this sentinel and replaces it with 0:

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

#### The tagged scalar representation (default)

Under the tagged representation, null-capable scalar slots use an inline two-word
`{payload, tag}` pair (`TaggedScalar`) instead of the in-band sentinel: the payload travels
in the integer result register (`x0`/`rax`) and the runtime tag in the adjacent register
(`x1`/`rdx`), mirroring the string pointer/length convention. The tag reuses the runtime
value tag scheme (0 = int, 8 = null), so a tagged scalar is word-compatible with a boxed
Mixed cell's tag/payload words. On the stack the payload sits at `offset` and the tag at
`offset - 8`.

Null-capable producers — miss-capable int array reads, `array_pop`/`array_shift` on int
arrays — yield a tagged scalar; consumers (`echo`, `var_dump`, `is_null`, `??`, `??=`,
`isset`, `empty`, `gettype`, casts, arithmetic narrowing, `===` through the Mixed boxing
path) dispatch on the tag word. A plain non-nullable `Int` is statically never null, so its
sentinel checks disappear entirely and the full 64-bit range round-trips:

```asm
; null check on a tagged scalar
cmp x1, #8                ; runtime tag 8 = PHP null
b.eq value_is_null
```

A tagged null carries the sentinel as its payload word, so boxing it into a Mixed
cell produces `{tag 8, sentinel}` words and un-audited consumers degrade
to sentinel behavior. Declared `?int` parameters, returns, and properties are tagged
scalars too under the default mode, so they round-trip the full 64-bit range; a `?int`
property slot stores the payload at its slot offset and the runtime tag at `offset + 8`,
and its literal default must be written as that same `{payload, tag}` pair rather than as
a pointer to a boxed Mixed cell.

**A tagged scalar reaching a `mixed` parameter must be boxed, and the decision is made on the
REPRESENTATION rather than the declared type.** `int|null` is the one union
`PhpType::codegen_repr()` does not map to `Mixed`, so a helper that matches
`PhpType::Mixed | PhpType::Union(_)` as "already boxed" silently covers it too and emits nothing.
The callee then reads the raw payload word as a Mixed pointer, and the caller's own
`__rt_decref_mixed` runs on it — measured as a segfault on `var_export($nullableInt)` whether the
value held an integer or null (#1040). The same shape is worth checking at any other match arm
that treats a union as boxed.

The per-class property descriptors the runtime walkers read (`_class_vd_desc_*` for `var_dump`,
`_class_prop_desc_*` for `print_r`/`var_export`, `_class_json_desc_*`, `_class_serprop_*` for
`serialize`/`unserialize`/`get_object_vars`/`(array)`, and `_class_serpdiag_*` for the
`unserialize` hydration type guard) had the same hole: a `?int` property got
the union's static tag 7, so every walker dereferenced the integer payload as a Mixed cell
pointer (#1503). No static tag can describe that slot, because its tag is only known at run
time, so those descriptors carry `TAGGED_SCALAR_PROPERTY_TAG` (12) instead, and each walker
replaces it with the slot's high word before dispatching
(`emit_resolve_tagged_scalar_property_tag`). `unserialize` writes the `{payload, tag}` pair
back into such a slot, and `__rt_obj_store_prop` first consults the parallel `_class_serpdiag_*`
row: it rejects a boxed tag the declared type does not accept with PHP's catchable `TypeError`,
and rewrites an accepted `int` into a `float` only when the declared type includes
`float` but not `int`; an `int|float` slot keeps the integer. The GC descriptor
(`_class_gc_desc_*`) records the slot as `0`: it owns
no heap reference, and tag 7 there had handed the payload to decref, clone and cycle marking.

### Pointer values

Pointers are stored as raw 64-bit addresses. An opaque pointer and a typed `ptr<T>` value have the same runtime representation; the type tag only exists in the checker. Null pointers use address `0x0`, and dereference helpers explicitly trap on null via `__rt_ptr_check_nonnull`.

A `?ptr<T>` uses the same word, with PHP `null` as address `0x0`. A PHP local that receives `null`, packed record addresses and pointers at different points gets that pointer-word frame slot too (`pointer_word_storage` in `src/ir/builder.rs`) instead of a boxed Mixed cell. The slot type is final only after the whole body is lowered, so codegen writes a `null` stored into such a slot as `0x0` and reads the slot back as the null sentinel where lowering knows the local holds `null`. Lowering types the local `?ptr<T>` where it can be either: at `if` merges, from a `?ptr<T>` loop contract the checker records for a local that enters a loop holding `null`, and after a null-only local is bound to a `?ptr<T>` by-reference parameter.

### Buffer handles, descriptors, and payloads

A `buffer<T>` local stores an opaque 64-bit handle rather than a payload or header pointer:

```
Bits 63..32  generation (non-zero u32)
Bits 31..0   descriptor index (1..=4096)
```

The static `_buffer_registry` contains a reserved descriptor at index zero plus 4096 usable 48-byte descriptors:

```
Offset  Size  Field
  0      8    payload pointer
  8      8    logical element count
 16      8    element stride
 24      8    generation slot (low u32 used)
 32      8    active marker
 40      8    free-list successor index
```

`__rt_buffer_resolve` validates the handle index, non-zero generation, active marker, and exact generation match before length, read, write, or free paths use descriptor metadata. `buffer_new<T>()` allocates exactly `length * stride` payload bytes from the compiler-managed heap and zeroes only that payload. `buffer_free()` marks the descriptor inactive before releasing the detached payload. Reusable descriptors enter the `_buffer_registry_free` list and receive a new generation for their next lifetime, which keeps stale aliases invalid after slot or heap reuse. A descriptor whose u32 generation reaches its maximum is retired instead of wrapping to zero.

Whole packed-element assignment lowers to `BufferSet` (or `BufferSetResolved` after
optimization) and copies the packed class's complete stride through
`src/codegen/lower_inst/packed_record_copy.rs`, shared with nested packed-field stores.
The assignment checks the destination handle and index; a source element read checks its
own handle and index before supplying its address. The copy allocates nothing and does no
reference counting: nested records and padding are bytes, while pointer fields retain their
addresses. Typed pointer sources use the null guard and require caller-managed liveness.
Resolved packed stores remain call-clobbering for register allocation, unlike scalar stores.

Buffers created inside `eval()` do not use `_buffer_registry`. Magician keeps its own thread-local registry (`crates/elephc-magician/src/interpreter/builtins/raw_memory/buffer_registry.rs`) with the same lifecycle: `buffer_new()` publishes a zeroed `calloc` payload of `length * 8` bytes in a slot and answers the handle `(generation << 32) | (1 << 31) | slot`, `buffer_len()` and `buffer_ptr()` resolve the handle before reading the slot, and `buffer_free()` unpublishes the slot before freeing the payload, so a stale alias dies with the same `use of buffer after buffer_free()` fatal. Bit 31 keeps eval handles disjoint from compiled descriptor indices, generations stop at `2^31 - 1` so every handle is a positive PHP integer, and a slot that reaches that generation is retired.

### Fiber stacks and scheduler state

`Fiber` objects own native stacks rather than borrowing the caller's stack. The runtime allocates each fiber stack through `mmap` (256 KiB usable by default, plus the guard page), protects the bottom 16 KiB with `mprotect(PROT_NONE)` as a guard page, and stores both the mapping base and total mapped size in the Fiber object so `__rt_fiber_free_stack` can later return it with `munmap`.

The currently running fiber is tracked in `_fiber_current`. When execution switches away from the main stack, `_fiber_main_saved_sp`, `_fiber_main_saved_exc`, and `_fiber_main_saved_call_frame` preserve the main stack pointer, exception-handler chain, and activation-record cleanup chain. A suspended Fiber stores the same state inside its object payload (`saved_sp`, `own_exc_head`, and `own_call_frame`), so `__rt_fiber_switch` can swap between main and fiber contexts without mixing exception or cleanup chains.

Every compiled function prologue also consults `_stack_limit`. The main-thread
floor, derived from `RLIMIT_STACK`, is retained in `_stack_limit_main`; Fiber and
Generator switches replace the active value with their guarded-stack floor and
restore the main value on return. Reaching the floor raises the compiler's
maximum-call-stack fatal instead of falling through to a raw guard-page fault.

String offset reads return a borrowed one-byte view into their receiver. If the receiver is an
owned temporary, EIR lowering acquires the selected byte before releasing the receiver, so a
property, array element, or call result can be indexed without leaving a dangling view.
A byte needed after another expression is materialized before that expression runs, so
replacing the source cannot invalidate a pending key, comparison, dispatch, or call operand.
When acquiring or persisting a present byte view, the backend stages that byte on the stack
before calling the string helper. This prevents preceding payload bytes from masquerading as
a transferable concat allocation header. Empty and missing views bypass byte staging so the
string helper preserves its sentinel behavior.
For a byte selected from a heap-backed concat temporary, acquisition copies that byte
independently and the receiver release frees the concat block only while it still carries
`CONCAT_TEMP_HEAP_KIND`. A full-string store that already adopted and retagged the block
keeps its owner. Explicit yielded keys are boxed before their temporary byte owner retires;
the resulting single-use Mixed box transfers into the generator before suspension.
Silent `empty()` string probes use PHP string truthiness, including the one-byte string `"0"`.

Pending concat operands and computed array keys are published across an operand window when
its EIR effects include a catchable throw or a warning that can call a throwing user error
handler, or the window contains a direct throw terminator. Lowering
first discovers those effects, then persists fresh transient strings and places the owner
records before the window, retiring them after the consumer. This covers whole strings as well as persisted byte views;
non-throwing scalar windows introduce no owner slots that could inhibit integer sinking or CSE.

## The string buffer (scratch pad)

```asm
.comm _concat_buf, <heap-size>  ; heap-sized scratch buffer (default 8 MB)
.comm _concat_off, 8        ; current write offset (reset per statement, to the frame base)
```

The string buffer (`_concat_buf`) is a heap-sized scratch region (default 8 MB, matching the configured heap budget) used by all string operations — `itoa`, `ftoa`, `concat`, `strtolower`, `str_replace`, etc. Each operation writes its result at the current offset and advances the offset.

**The buffer is reset at the start of every statement — to the frame's inherited base, not necessarily 0.** Strings in the buffer are temporary: they live only for the duration of one statement's evaluation, *plus* (for an argument passed into a call) the lifetime of the callee. See [Cross-call slice arguments](#cross-call-slice-arguments) below.

### How it works

Within a single statement like `echo strtolower("HELLO") . " " . $name;`:

```
_concat_buf:
┌──────────┬──────────┬──────────────┬──────────────────┐
│  "hello" │  " "     │  "hello Joe" │  (free space)    │
└──────────┴──────────┴──────────────┴──────────────────┘
 offset=0    offset=5   offset=6      _concat_off = 17
```

Each sub-expression writes its result further into the buffer. After the statement completes (echo writes to stdout), the next statement resets `_concat_off` back to the current frame's base offset (0 in `main`).

### Cross-call slice arguments

A string operation returns a *borrowed slice* into `_concat_buf` (a pointer + length), not a heap copy. When such a slice is passed **as an argument** to a function, method, or closure, the callee runs its own statements — and resetting `_concat_off` all the way to 0 would overwrite the caller's slice bytes before the callee could read them.

To prevent that, each frame records, on entry, the `_concat_off` value it inherited from the caller (the high-water mark below which the caller's live slices sit). That value is the frame's **base**: per-statement resets restore `_concat_off` to the base rather than 0, so the callee's own concatenations append *above* the caller's slices instead of clobbering them. `main` (and other root contexts) have a base of 0, so their behaviour is unchanged. The cursor is also saved/restored around each nested call so the caller can keep concatenating after the call returns.

A consequence is that `_concat_buf` usage grows with the depth of nested calls that are *holding live slice arguments* (each frame reserves its caller's region). The arena tracks `--heap-size`, so this is bounded by the program's configured memory budget rather than a fixed 64 KiB cap.

### Copy-on-store

When a string result is stored to a variable (e.g., `$x = "a" . "b";`), the codegen calls `__rt_str_persist` which copies the string from the concat buffer to the **heap**. This ensures:

- **Variables always point to heap memory**, never into the scratch buffer
- **The buffer can safely reset** without invalidating stored values
- **Hash table keys** are also persisted to heap (via `str_persist`)

Transient adoption accepts only an exact live allocation base. The allocator records each
8-byte payload granule in `_heap_live_bases`, a bitmap using one byte per 64 heap bytes.
Allocation sets the base bit, free clears it before reuse or coalescing, and web arena reset
clears the used bitmap range before resetting `_heap_off`. The probe checks alignment, range,
the complete transient kind and the live-base bit in constant time. User-controlled bytes
before an interior substring cannot impersonate a heap header, and unrelated heap blocks
never need to be scanned.

### Outgrowing the buffer

The shared arena is where results *prefer* to live, not a ceiling on how large one can be. Every runtime producer reserves its destination through `__rt_concat_reserve`, which hands back scratch while the result still fits and an owned heap block when it does not; `__rt_concat_publish` then advances `_concat_off` only for the scratch case, deciding which it was from the pointer's own address rather than a flag. A producer whose size is not known until it has run — `implode()`, `stream_get_contents()` — starts in scratch and moves the bytes it has already written into a larger owned block with `__rt_concat_grow` when the next piece no longer fits.

A `.` result (and the `__rt_concat_grow` producers) that outgrows the buffer is stamped `CONCAT_TEMP_HEAP_KIND` instead of heap kind 1: like a scratch slice it has no owner that frees it, and the first consumer that persists it, a store or an `Acquire`, adopts the block in place by retagging it as an owned string rather than copying it, which is what keeps `$s .= ...` from duplicating a large accumulator on every append. That takeover is only safe while nothing else will release the same block. A user function that may return its argument hands the block back as its call result, an owning temporary the caller releases, so before such a call the caller claims a fresh transient argument (a `.`, interpolation or scalar-to-string result, never a builtin view such as `substr($s, 0)` or `$s[0]`, which points into a live string) with `EnsureOwned` (`__rt_str_claim`): the block is retagged as owned without a copy, every later persist duplicates it, and the argument is released after the call unless the callee returned it (`ReleaseUnlessAliases`). Without the claim, `$x = id($big . "b")` stored a block that the release of the call result then freed.

This is a bounds check, not an optimization. A producer that skips it writes straight past the current reservation into adjacent runtime state, which corrupts data silently in a CLI program and faults in a `--web` worker. The arena is sized by `--heap-size`, but it is still a fixed budget for one statement: every producer must reserve or grow instead of writing at a pointer it did not own. `implode()` used to do that (issue #515).

### Implications

- **Bounded usage.** Because the buffer resets each statement, only one statement's worth of string operations needs to fit in the configured arena — plus the slice arguments held by any enclosing calls on the current stack (see [Cross-call slice arguments](#cross-call-slice-arguments)). The arena follows `--heap-size` (default 8 MB), and a result that does not fit takes the heap fallback above rather than overflowing. A result larger than the configured budget reports a controlled capacity fatal.
- **Scratch copies.** Concatenation writes a new slice into `_concat_buf`, or a new heap block when the result outgrows the buffer. That scratch path leaves existing string bytes unchanged. In-place extension is the consuming heap append in [Performance characteristics](#performance-characteristics): a unique kind-1 buffer whose right-hand side does not overlap it.
- **Scratch only.** The buffer is strictly temporary. Anything that needs to survive goes to the heap.

## The heap

```asm
.comm _heap_buf, 134217728  ; 128MB buffer (configurable via --heap-size)
.comm _heap_off, 8          ; current bump allocation offset
.comm _heap_free_list, 8    ; head of the general free block linked list
.comm _heap_small_bins, 2048 ; 256 x 8-byte heads, one per exact payload size 8..2048
.comm _heap_bin_marks, N     ; heap/16 + 2 bytes: one "parked in a bin" byte per granule
.comm _heap_consolidated_frees, 8 ; _gc_frees at the end of the last allocator consolidation
.comm _heap_debug_enabled, 8 ; heap-debug toggle
.comm _gc_collecting, 8     ; cycle collector re-entry guard
.comm _gc_release_suppressed, 8 ; suppress nested collection during deep free
.comm _gc_allocs, 8         ; allocation counter
.comm _gc_frees, 8          ; free counter
.comm _gc_live, 8           ; current live heap footprint in bytes
.comm _gc_peak, 8           ; heap high-water mark
```

The heap (`_heap_buf`) is a 128 MB region by default (PHP's default `memory_limit`) for dynamically-sized data — arrays, hash tables, objects, and persisted strings. It uses a **free-list + bump hybrid allocator** with segregated small-block bins for the hottest tiny allocations.

### How heap allocation works

Every allocation has a **16-byte header**: two 32-bit fields for block size and reference count, followed by an 8-byte uniform heap-kind tag:

```
┌───────────┬────────────┬────────────┬──────────────────┐
│ size (4B) │ refcnt (4B)│ kind (8B)  │  user data ...   │
└───────────┴────────────┴────────────┴──────────────────┘
       header (16 bytes total)          ← pointer returned to caller
```

The size is stored at header offset `+0`, the reference count at `+4`, and the heap kind tag at `+8`. New allocations start with refcount `1`; typed constructors then stamp the kind as `1=string`, `2=indexed array`, `3=assoc/hash`, `4=object`, `5=boxed mixed`, `7=concat temporary`, `8=reference cell`, and `9=callable descriptor`, while raw helper buffers remain `0`. Compact runtime-thrown Throwable payloads (exception/error objects allocated by throw paths) are stamped kind `6=throwable`; the uniform release dispatchers (`__rt_decref_any`, `__rt_object_free_deep`) accept kind `6` and release it through the object path. Generator frames are stamped as kind `4` object blocks because `Generator` is a built-in class with a custom payload layout.

On x86_64 the 8-byte kind word also carries an ownership marker: the ASCII bytes `"ELPH"` in its high 32 bits. Every x86_64 heap stamp goes through the shared `codegen_support::sentinels` helpers — `x86_64_heap_kind_word(low_bits)` builds the full word (magic + packed kind/COW/value_type low bits) and checkers compare against `X86_64_HEAP_MAGIC_HI32` — instead of hand-typed immediates. Refcount and free helpers ignore pointers whose header does not carry the marker, so foreign or static pointers silently opt out of refcounting. For array/hash containers, the low 16 bits of the kind word are persistent metadata: the low byte is still the heap kind, indexed arrays still pack their runtime `value_type` in the next byte, and bit 15 is reserved as the persistent copy-on-write container flag. Higher bits remain transient collector metadata.

Throwable subclasses with properties or constructors supplied by a user-defined
ancestor keep ordinary object storage. Their inherited `Error::__construct` and
`Exception::__construct` bodies use the typed EIR `object.throwable_initialize`
operation. It consults the receiver's previous-slot descriptor, installs owned
replacement fields, then releases displaced owners. The same constructor can
therefore reinitialize a compact native Throwable or an ordinary eval/subclass
object without mixing raw previous pointers and nullable Mixed cells.

Mixed add, subtract and multiply may borrow an integer RHS cell from a 32-byte,
16-byte-aligned stack span for the duration of that single runtime call. The cell
has the usual 24-byte tag/payload layout, with integer tag zero, but no managed
heap header. Only these read-only intrinsics may receive it: they must not retain,
release, store or return its pointer. Their result remains a fresh owned heap
cell, including after integer overflow promotes to float. Codegen uses this path
only for an exact integer constant beside a boxed Mixed left operand; bool, null,
float and dynamic operands keep ordinary materialization. Boxed increment and
decrement use the same loan for their integer delta. No caller-visible value,
reference binding or loop type proof changes.

Deep release of objects, indexed arrays, hashes, Mixed cells, and callable
captures contains destructor exceptions in a native cleanup boundary. Each
cleanup frame keeps a pending exception chain, completes the remaining child
releases, frees its container, and restores the enclosing GC suppression state
before propagating. The typed-property unset bridge has a versioned Throwable
output argument so this propagation returns to Magician as an eval exception
instead of jumping across Rust frames.

`__rt_cleanup_invoke` can release a validated scalar Mixed cell without creating
a new jump buffer. It first authenticates the exact `__rt_decref_any` callback,
managed heap storage, a live owner count, the Mixed heap kind, and an int, float,
bool, or null runtime tag. It invokes that same callback pointer, so ordinary
refcounting and collector sweep suppression remain authoritative. This path
reports no new exception and preserves the enclosing pending exception state.
A separate proof authenticates the exact `__rt_decref_array` callback and a
live indexed array with logical length zero. Reserved capacity owns no child
slots, so the original dispatcher can release that array without a new handler.
Hash-promoted storage, nonempty arrays, strings, objects, resources, reference
cells, unknown tags, and other callbacks use the protected cleanup boundary.

Dynamic eval creates its activation context before synchronizing the caller scope,
but may postpone native catalog registration for a proven numeric-only fragment.
The execution proof also requires every retained local and global scope value to
be a trivial scalar, so context destruction cannot invoke a callback before an
upgrade. Generated eval opts into a separate deferred-aware entry point; the
existing execute ABI retains eager global-class synchronization.
The proof accepts only straight-line numeric expressions, stores and returns with
existing int/float cells (or fresh numeric locals). Undefined reads, nonnumeric
values, reference/global/typed bindings, native symbol use, calls and other syntax
require the complete catalog before PHP executes. Replacing a nonnumeric cell also
fails the proof, because releasing it could invoke a destructor or callback.
A guard before scope synchronization also requests metadata if replacing any old
cell could invoke PHP, or if an indirect or typed binding needs validation. Both
guards share one registration transaction. A later fragment upgrades the same
context without replacing its scope or handlers.
Until that upgrade, numeric-only execution also postpones global class imports, so
native declarations retain their usual position before dynamic declarations in
introspection. A completed catalog is still shared by module generation and thread.

Interpreter value releases and native-call argument cleanup use a versioned
release boundary with an owned Throwable accumulator. Releasing a group finishes
every element and preserves earlier destructor exceptions in the resulting chain.
The release status reports only a newly caught exception, separately from that
accumulator. An earlier pending throw must not turn successful temporary cleanup
into a failure that interrupts native by-reference writeback.
The eval object-edge callback returns that owned exception box to native code;
it never propagates by jumping out of the Rust registry iteration.
Scope replacement, unset, and free similarly return exceptions through versioned
Rust entries. Native adapters propagate them only after the scope mutation or
complete scope retirement has finished; cleared native cleanup slots prevent
reentrant unwinding from consuming a retired scope or local owner twice.
Promoted local reference cells follow the same rule: detach the owner before
releasing the payload, contain any payload exception, and free the cell before
propagating. This applies both to explicit retirement and function epilogues.
Object-owned property reference cells use heap kind `8`, with their payload type
in header bits `8..14`. Their two payload words remain the value's low word and
its high word or string length. Class GC descriptor tag `11` identifies these
owned cells; constructor-promoted borrowed reference addresses stay non-owning.
A call temporary created for a fixed scalar, string, or array property sets its
header flag bit `19` and appends four metadata words after the payload: a static
diagnostic-suffix pointer and length, a declared-type acceptance code, and a
reserved word. The callee's `StoreRefCell` guard reads that metadata before
coercion and raises PHP's reference-form `TypeError` when the source cannot bind
to the property. Ordinary local and variadic cells leave the flag clear, so their
layout and release path are unchanged.
Each alias owns the cell separately from the contained value. Rebinding acquires
the new cell before retiring the previous local, which may own the source object.
Cloning separates a singleton cell and shares one with live aliases, excluding
temporary collector pins when making that decision.

Resolved reference-returning callees produce two distinct things at the `return`,
and keeping them apart is what makes the boundary correct. `AcquireRefCell`
materializes the returned cell address ONCE and publishes it as a typed `Pointer`
SSA result; the return terminator transports that snapshot. Separately, when the
address owns a managed cell, the retained owner goes into a `ReturnRefCell` lease
slot: normal return transfers it, while an exception during epilogue cleanup
releases it, and a second `return` inside a `finally` publishes its replacement
before retiring the superseded lease, because that retirement can run a throwing
payload destructor.

Because the returned reference is the snapshot rather than a re-read of the
variable, a `finally` that rebinds the returned variable and falls through cannot
change what was returned. This mirrors PHP, which materializes the reference with
`MAKE_REF` before the finally rather than re-reading the variable afterwards.

An accepted by-reference return source is either a MANAGED cell whose ownership
transfers with the address, or an EXACT bounded active borrow whose ultimate consumer
copies the pointee before any cleanup can run (see the `array_walk()` exception below).
Everything else is refused. For the managed transfer, a promoted property slot already
holds a cell; an ordinary addressable local is promoted in place first, which preserves
the variable's identity.

That promotion uses the DECLARED result's payload representation, not the local's
narrower inferred storage: a concretely typed array local inside an `: array` function
widens to `Mixed` before its cell exists, because the caller dereferences the cell with
the declared shape and a later write through either alias has to keep meaning. Storage
that other aliases ALREADY share cannot be re-shaped that way, so it is checked instead
of widened:

- A by-reference parameter's storage belongs to the caller. A payload representation
  disagreeing with the declared result is refused at compile time.
- An object property's slot is shared by every holder of the object. When the receiver's
  class is statically known, a disagreement is likewise refused at compile time. When it
  is not (a `mixed` receiver, or the `$this` of a `Closure::bind` closure), the check
  moves into the guarded `LoadPropRefCellChecked` lowering, which decides PER CANDIDATE
  CLASS after the runtime class-id dispatch: a compatible class loads its cell, and an
  incompatible one raises a catchable `Error` BEFORE any pointer is published, so the
  receiver and its other aliases are untouched. Rejecting every `Mixed` receiver instead
  would break the compatible classes that reach the same function.

Compatibility is representation, not identity: two object classes share one pointer
layout and are interchangeable here, while container element layouts are part of the
payload and are compared recursively.

A source that is none of the accepted shapes is refused during lowering with a source
diagnostic instead of being lowered as a value. Those refusals describe this lowering's
subset, not PHP: PHP does return references to array elements and to other
reference-returning calls, PHP's references carry no payload type at all, and the
refusals exist only because no owning cell can be transferred for them yet.

Because a by-reference return's guard can throw, an OWNING temporary receiver is rooted
in the call-operand-owner chain across both the guarded cell load and `AcquireRefCell`,
and retired exactly once after the snapshot is published. A borrowed receiver (a plain
local or parameter the caller still owns) is never rooted and never released there.

`Closure::bind(fn &() => $this->prop, $newThis, ...)` has a direct specialization for
reference assignment. Ordinary by-value calls through its descriptor copy the pointee
into `Mixed`; that value does not transport an alias. The closure
literal is lowered once, in source order, with the bound property's type as its result
type and its `$this` capture boxed as `Mixed` whatever scope the literal was written in.
The bound descriptor is then a second `closure_new` over that same compiled function whose
only capture is the receiver's box. That descriptor OWNS the box, and the direct call
borrows it, so the descriptor owns one receiver reference: releasing the closure releases the
box, which releases the receiver, which is where its destructor runs. The literal's own
descriptor is rooted across receiver and scope evaluation and retired afterwards, and the
bound descriptor is staged in its own owner slot before either root so it stays reachable
while those retirements run destructors.

A by-reference function whose declared result is missing runs no return-coverage
analysis in the checker, so an inferred non-void one can still fall through. The
result register carries a cell ADDRESS the caller dereferences and may alias, so
that path raises the same catchable `Error` rather than handing back the null
placeholder a by-value return would use.

Provenance the frame cannot see is settled at run time. A function returning its own
by-reference parameter may have been handed an array-interior address by its caller,
which the owner lookup answers zero for. `AcquireRefCell` therefore fails closed:
a zero owner raises the catchable `__rt_borrowed_reference_return_error` rather than
retaining nothing and publishing a null or soon-to-be-freed interior pointer. The
message is separate from the `array_walk()` borrow escape so the diagnostic matches
the program that raised it.

That owner-zero rule has exactly one accepted exception, and it is checked against
the runtime rather than assumed: an address that is an EXACT node of the active
unmanaged-borrow chain is a live boxed `array_walk()` element. Nested reference
relays are permitted while that borrow is active: the frame that publishes the address
is not necessarily the one that consumes it, and what bounds the lifetime is the active
walk itself, not the identity of the direct caller. The ULTIMATE consumer (the
descriptor invoker or the walk that owns the borrow chain) copies the final pointee
into an owned `Mixed` before releasing the element, so each relay is accepted with NO
lease and still transports its own snapshot. Every other owner-zero address, including
an ordinary array element relayed through a by-reference parameter, still fails closed
rather than escaping with no bound at all, and the separate escape guards that reject
publishing such a borrow into a property, a promoted constructor property or a closure
capture are unaffected.

The caller either adopts the lease for reference assignment or acquires the
contained value and retires the lease. Both adoptions happen immediately after the
call, before argument temporaries, evaluation intermediates or an owning receiver
are retired.

A reference assignment needs the lease to survive longer than that adoption point:
it still has to retire the previous binding of its target and publish the alias, and
both can run a throwing destructor. Its staging local and ref-cell owner slot are
therefore declared and PUBLISHED in the call-operand cleanup chain BEFORE the source
expression is lowered, so the record nests outside every argument root the call
publishes and is detached only once the alias owns the cell. A same-frame `catch`,
which runs no whole-frame cleanup, still releases the payload exactly once. The
record's cleanup discipline follows the slot: a ref-cell owner always uses the
generic `__rt_decref_any` entry, which dispatches heap kind 8 to
`__rt_reference_cell_release`, never the callable-descriptor entry its payload type
would otherwise select.

A by-value use of the same call publishes its own staging on the same terms, and
for the same reason: the copy can only be taken once the caller's argument
temporaries, evaluation intermediates and owning receiver have been retired, and
each of those runs a destructor that can throw. Until then the lease is the only
owner of the pointee, so its record is published before the arguments are
evaluated and detached only after that cleanup, keeping the chain strictly LIFO.
The payload is loaded at that point and the lease is retired immediately
afterwards. The load is a COPY, not an alias of the lease: a boxed `Mixed`
pointee is cloned rather than retained, because the lease keeps the callee's own
mutable box and a later write through the same reference mutates it in place.
Every other refcounted payload keeps the plain retain, which is exactly PHP's
copy-on-write by-value array copy and preserves the alias and reference-writeback
behavior a later write through the cell relies on. This is the same rule a
by-value `return` of a reference-cell read applies, so the resource special case
is handled by the runtime clone.

Publication of that staging is mandatory, not opportunistic. Every call site
resolves its callee signature BEFORE evaluating arguments, so there is no path on
which a by-reference-returning call reaches its adoption point without a published
record; lowering fails closed instead of falling back to the immediate unrooted
copy it used to take.

Owned by-value arguments of a reference-returning call are rooted like any other
call's, including a fresh container materialized for an omitted by-reference default,
so a throwing callee cannot strand them. Actual reference places stay unrooted
because their storage belongs to the caller.

A reference assignment is only accepted when the SELECTED lowering adopted such a
lease. A declared by-reference signature is not sufficient on its own, because a
call routed through a dynamic descriptor invoker receives an owned boxed copy while
the invoker retires the cell; binding that result would read a payload word as an
address, so lowering refuses it.

Refusals are collected in a thread-local sink that `lower_program` drains before
validation. Collection is rollback-safe: the speculative region `stmt::repr_fixpoint`
lowers and discards rolls its refusals back with it, and the guaranteed final lowering
of that region records them again.

Callable operands follow the same publication rule as call arguments. A callable
descriptor's callback is published before the argument container is built, because
building it runs PHP expressions, and a freshly evaluated callback (a callable
array holding a `new` receiver, or a computed function name) is owned by nothing
else. The callback record nests outside the container record and is retired last.
An immediately invoked closure literal is called directly through its captured
values rather than its descriptor, so that unused descriptor is published for the
duration of the call and retired afterwards instead of being abandoned. A
statically resolved extern or builtin callable runs the same argument ledger and
the same post-call argument release as the identical direct call. The statically
lowered `array_map()` fast path publishes its partially built result and reloads
the current pointer after every push, and it applies only to source elements whose
evaluation cannot be observed, because it interleaves element evaluation with
callback invocation while PHP evaluates the whole source array first.

Descriptor invocation also carries the physical caller's scalar-binding mode,
independently of where a callable was created. Its hidden frame-owned profile
preserves legacy weak class ids and global `-1`; strict profiles encode
`-class_id - 3`, including global `-2`. The native boundary and invoker forward
that same word across throws and nested calls. Source-sensitive builtin wrappers
select strict or weak EIR paths from it, while the `clone()` wrapper decodes the
original scope before checking hook visibility. No mutable global carries this
state. Weak Stringable union preparation copies and roots the source cell before
invoking `__toString()`, then retires both source and converted-string leases.

Argument containers are published the same way whether they are built from a
signature or not. A `call_user_func()` / `call_user_func_array()` container is
rooted for its whole construction and reloaded from that slot after every
insertion, because an insertion can reallocate the payload and because a later
argument expression can throw while the container is the only owner of everything
already inserted.

Those builders are TOTAL. Every argument shape has a container form: named
arguments build a boxed hash, anything else builds an indexed array, and a spread
mixed with named arguments merges into the same hash with runtime numeric keys
(PHP requires unpacking to precede explicit named arguments, so the positional
numbering stays contiguous). Declining a shape after the callback has already been
evaluated is not an option: the caller's fallback would evaluate that callback
expression a second time and run its side effects twice. For the same reason a
callback whose storage shape has no descriptor arm reuses the value that was
already lowered rather than returning to the caller, and the bound-closure
lowerings decide their whole structural shape before emitting anything.

A call's own owned result is staged as well. Retiring the argument roots, the
evaluation intermediates, the argument container, a descriptor callback or an
owning receiver all run PHP destructors that can throw into a `catch` in the SAME
frame, and until the staging holds it the result is only an SSA temporary that no
record can see. The staging slot is a one-shot owned temporary that the frame
prologue zero-initializes; it is published BEFORE every operand root of that call,
the result is MOVED into it right after the call with no extra retain, and it is
popped LAST, because the runtime's operand scope is a plain LIFO stack that cannot
detach an arbitrary named record. On the normal path the slot is CLEARED rather
than released, which transfers the single reference back to the expression's
consumer; on the unwind path the record releases it exactly once, before the catch
body runs. The slot is declared with the exact result type the call is emitted
with. Only heap-bearing results publish heap-owner records. A rejected call can leave
a scalar result slot uninitialized, and heap cleanup must never interpret its bits
as a pointer. For heap-bearing results, two further cases are deliberately excluded:
a by-reference-returning callee, whose
result is the transferred cell the reference staging already owns, and a callee
whose summary says its result may be one of its arguments, because that argument's
release is guarded by a runtime alias comparison and rooting both would release one
shared reference twice.

Ordinary descriptor calls box the referenced value before releasing the cell.
Cell-owner lookup validates allocation boundaries before adopting an unknown
pointer, so borrowed frame and inline-array addresses never become heap owners.
That lookup scans allocation headers after the fast kind/range rejection; known
property aliases retain directly without this scan.

Plain local overwrites and `unset()` also retire the previous slot owner before
calling its release helper. The EIR retirement operation records that local
mutation and the observable effects of a potentially throwing destructor.

Property element removal fetches separated storage with `PropGetForWrite` and
removes the key in place. It never stores a container back after releasing the
removed value, because that value's destructor may replace the property or throw.
Owned receiver expressions and keys stay in unwind-visible EIR operand slots.
If key conversion can invoke a warning handler, native lowering retains a snapshot
owner of the selected hash in a temporary cleanup scope. After conversion, another
live owner allows this snapshot to be retired before deletion, preserving ordinary
destructor interleaving. A detached hash keeps its snapshot through deletion or
exception unwinding. The old property value is never stored back.

When a boxed property holds an `ArrayAccess` object, the native
`__rt_mixed_object_offset_unset` helper retains the selected object across dispatch.
A callback may clear the supplying property while still using its receiver. The
helper guards this receiver before entering PHP or an SPL offset conversion that
can invoke an error handler. PHP methods borrow the original boxed key, so a
second guard protects that key during the method call. Normal cleanup retires the
key before the receiver; an escaping throw releases both through the shared
activation chain. SPL helpers consume the original key themselves and apply
their own offset conversion rules. `OffsetUnset` carries conservative callback
effects because these methods and warning handlers can observe or mutate PHP state.

Ordinary executable PHP frames and library frames both publish exception-cleanup
activations. Each PHP catch saves the current activation as the unwind stop, so
the catching function and its callers keep their live local owners. The unwinder
detaches each younger, abandoned activation before running its
callback. That callback clears local owners before release and contains each
destructor exception, preserving the original exception chain while finishing
the remaining locals, reference cells, and eval handles. Cleanup of a frame must
not depend on the program being built as a shared library.
Descriptor invokers also bound native calls, not only eval callbacks. An escaping
exception releases all acquired argument owners and any interrupted return value;
further destructor exceptions are accumulated before native propagation or the
eval ABI's null-result return.
An eval-declared destructor's receiver lease is separate from the owner being
released. Even when the destructor throws, that final owner must still be consumed;
the runtime release preserves the pending exception and collects any further child
destructor exceptions before returning to eval.

Native local synchronization gives the eval scope an independent snapshot owner.
Container and Mixed parameters expose their active copy-on-write shadow under the
PHP parameter name, not the inactive ABI entry slot or the internal `#cow` name.
Reload acquires the replacement, publishes it into raw or reference-cell storage,
then releases the displaced native owner, including unchanged cells and missing
entries. By-value parameters that eval can replace own their initial frame values;
by-reference parameters continue to use the caller's storage. String reloads
persist exactly once, including casts that initially return scratch bytes.
Global reload follows the same publish-before-release rule for both ordinary
Mixed cells and web superglobals with raw string, indexed-array, or hash storage.
An unchanged pointer still replaces an independent owner; an unset entry must
retire the previous payload before leaving an empty global slot.

A callable parameter returned as `Mixed` receives a separate boxed descriptor
reference. The caller must still retire a temporary callable argument after the
call, even when the return-alias summary is conservative. Returning a raw
`callable` keeps the separate transfer rules for that representation.

The runtime routine `__rt_heap_alloc`:

1. **Pop the exact size-class bin** — `_heap_small_bins` holds one chain head per 8-byte payload class from 8 to 2048 bytes (`HEAP_BIN_MAX`); the head for payload `n` sits at offset `n - 8`. A request up to 2048 bytes pops the head of its own class: one load, no scan.
2. **Walk the general free list** — if the bin is empty, or the request is larger, check the address-ordered free list (first-fit). If a block with `size >= requested` is found, either unlink it whole or split it. A split remainder with payload up to 2048 bytes enters its exact bin, like an ordinary freed block; a larger remainder stays in address order on the general free list. Then reset the allocated block's refcount to 1 and return it.
3. **Bump allocate** — if neither free path fits, allocate from the end of the heap: write size and refcount=1 to the header, advance `_heap_off`, return user pointer.
4. **Split a larger binned block** — if the bump would exceed `_heap_max`, the bins above the request are searched (at most one load per class) and the first valid block is split like a free-list block. Exact bins never split on their own, so without this step free memory parked in other size classes could not serve the request.
5. **Consolidate and retry** — if no larger binned block exists, `__rt_heap_consolidate` merges every free block (see below) and the allocation is retried from step 1. The pass is skipped when `_gc_frees` still equals `_heap_consolidated_frees` (nothing freed since the previous pass, so nothing to merge). Only a skipped pass, or one that changes nothing, ends in the fatal "heap memory exhausted" error.

Minimum allocation is 8 bytes (to fit the next pointer when the block is later freed). A split requires room for a new 16-byte header and at least eight payload bytes. Its free header has reference count and kind zero and belongs to exactly one chain. Moving that already-free remainder into a bin does not count as an allocation or free and does not change live bytes.

### Consolidation

Binned blocks are not coalesced when they are freed, which is what keeps allocation and free O(1) however many blocks are free. `__rt_heap_consolidate` merges them later, in one pass linear in the heap:

1. Every block reachable from a bin or the ordered list is marked free (a sentinel in its kind word), and the bins and the list are emptied.
2. One walk over the tiled heap, from `_heap_buf` to the bump end, merges each maximal run of marked blocks under one header. A run reaching the bump end lowers `_heap_off`; any other run goes back to its exact bin (on an allocation retry) or onto the address-ordered list.

Only blocks reached from the bins or the list are merged. A live block whose refcount is momentarily zero during a release is never marked, so it is never touched. `gc_mem_caches()` runs the same pass, listing every run so the bins end up empty, and returns the bytes it took out of the bins; it adds no free events.

### How heap freeing works

The runtime routine `__rt_heap_free`:

1. Read the block size (32-bit) from the 16-byte header at `user_pointer - 16`
2. If the block is exactly at the bump tail, shrink `_heap_off` immediately
3. Otherwise, payloads up to 2048 bytes are pushed onto the exact bin for their size, so a later allocation of the same size reuses them without scanning anything
   Web-mode frees first validate exact membership in the allocator live-base bitmap;
   duplicate detection does not scan the cached chain. Heap-debug bin marks and full
   chain validation remain available for diagnostics.
4. Larger non-tail blocks are inserted into the general free list in address order and merged with adjacent free neighbors. Only a merged block ending at the bump frontier starts a tail search, which repeatedly trims the now-free tail chain back into `_heap_off`; other insertions skip that scan
5. Free blocks reuse the same 16-byte header, clear the kind back to `0`, and then store the next pointer immediately after it: `[size:4][refcnt:4][kind:8][next_ptr:8][...unused...]`

The variant `__rt_heap_free_safe` validates that the pointer is within `_heap_buf` range before freeing — safe to call with garbage, null, or `.data` section pointers.

### Reading heap usage inside a program

`memory_get_usage(bool $real_usage = false): int` returns live native heap bytes.
`memory_get_peak_usage(bool $real_usage = false): int` returns their high watermark.
Both include allocator rounding and the 16-byte header of each live block, including
internal runtime allocations. Released blocks stop contributing to live usage even
when cached for reuse. In-place growth updates the same byte counters without
counting as a new allocation.

With `real_usage: true`, both functions return the full fixed arena capacity selected
by `--heap-size`, including its free space. The arena is reserved in BSS for the
lifetime of the program. This value is neither resident set size nor the bump offset.
`memory_reset_peak_usage(): void` resets the live-byte watermark to current live
usage. It cannot shrink the arena, so the reserved-byte peak stays constant.

The elephc extensions `memory_get_allocations(): int` and `memory_get_frees(): int`
read cumulative successful allocation and release counts. Reusing a cached or
free-list block counts as an allocation. These counters survive peak resets and
are hidden by `--strict-php`. All five native operations take constant time and
allocate no memory, allowing before/after samples around frame phases. See
`examples/memory-phases/main.php` for a 10,000-entity example.

These measurements cover `_heap_buf`, not concat scratch, stack storage, other
BSS buffers, libc allocations, or Rust bridge and interpreter allocations. In
`eval()`, the functions sample the same native heap before boxing their result.
Eval argument/result cells and callable machinery can themselves allocate, so
interpreted measurements include that overhead where it uses the native heap.

### Heap debug mode

Passing `--heap-debug` enables additional runtime verification without changing normal ownership behavior:

- `__rt_heap_free` rejects duplicate insertion of the same block into the free list (`double free`). For a binned size this check is O(1): `_heap_bin_marks` keeps one byte per 16-byte heap granule, set when a guarded free parks a block in a bin and cleared when the block leaves it (allocation, consolidation, the `--web` reset). The bin is scanned only when the mark is already set, which a double free and a rare stale mark both explain. The `--web` heap guard uses the same check.
- `__rt_incref` / `__rt_decref_*` reject zero-refcount heap blocks before mutating them (`bad refcount`)
- `__rt_heap_alloc` / `__rt_heap_free` validate the ordered free list plus every exact size-class bin chain and trap on out-of-range, overlapping, cyclic, mis-sized, or merely-adjacent free blocks (`free-list corruption`)
- `__rt_heap_free` poisons freed payload bytes with `0xA5`, so stale raw reads stand out immediately in debug repros
- process exit prints a heap-debug summary with alloc/free counts, live blocks, live bytes, a leak summary line, and the live-byte watermark since the latest explicit or request reset
- non-clean summaries also report up to 64 live block headers: heap-relative offset, total bytes, heap kind, and reference count, without printing payload contents

When one of these checks trips, the program exits with a fatal heap-debug error instead of continuing with corrupted allocator state.

In full-reset `--web` mode, the next request reclaims the previous PHP heap arena after typed cleanup. This bulk release brings cumulative frees up to cumulative allocations and resets live bytes and their peak watermark to zero for the next request. Per-request counter differences therefore describe the current arena, not blocks already reclaimed by earlier resets.

### When memory is freed

- **Variable reassignment**: when a heap-backed local/global/static slot is overwritten, codegen releases the previous owner through the appropriate runtime path (`__rt_heap_free_safe` for persisted strings, `__rt_decref_*` for refcounted arrays / hashes / objects). When a store inside a loop is lowered before a later store has widened the slot to boxed storage, lowering emits a deferred `release_local_slot` and the backend decides against the slot's FINAL widened storage type. This covers two shapes. The slot may look untracked at the earlier store — an inner `for` counter re-initialized by the outer body but widened Int→Mixed by its `++` update (issue #534) — or it may already be tracked and still not final: a local typed `Object("Shape")` is widened to `Mixed` by a later store of a different class, since `widened_local_storage_type` has no arm for a pair of object types (issue #479). The second one is the more dangerous of the two, because the eager release it replaces is not a no-op but an unbox WITH RETAIN: typed at the concrete slot against boxed storage, it hands back the inner pointer holding a fresh reference, the release cancels exactly that reference, and the box is never freed — leaking the box and the value it pins, once per iteration. `Mixed`/`Union` storage keeps the eager path because it is terminal, and `Str` keeps it deliberately: its eager release runs through the ownership analysis, which knows a `.rodata` literal pointer must not be freed, and the slot-typed cleanup does not model that
- **`unset()`**: releases the current heap-backed value before nulling the slot
- **Targeted cycle collection**: `__rt_gc_collect_cycles` counts heap-only incoming edges and marks externally reachable blocks. Before sweeping an unreachable graph containing objects, it snapshots and pins its candidate nodes, runs their destructors while their data remains intact, then repeats root analysis to observe mutations and resurrection. Pin owners are discounted from root counts. Surviving nodes lose only their temporary pins; unreachable nodes are deep-freed, and the separately allocated snapshot chunks are discarded without reading reclaimed PHP storage
- **Generator frame release**: Generator frames are object-kind heap blocks, but their custom Mixed slots and active `yield from` delegate are released by a Generator-specific branch in object deep-free
- **Object destructors (`__destruct`)**: `__rt_call_object_destructor` dispatches through the eval hook or class-id-indexed `_class_destruct_ptrs` table with a borrowed `$this`. Refcount bit 31 guards active execution. For collector-triggered destruction, heap-kind bit 14 remembers completion after that temporary guard is cleared, so a resurrected object is not destructed again on later collection or final release. Eval class/property metadata survives until actual object release. Ordinary last-owner destruction still runs at the top of `__rt_object_free_deep`, before property release; resurrection on that ordinary path remains unsupported
- **Process exit**: all memory is reclaimed by the OS

Collector-triggered destructors run under individual native exception handlers.
Both explicit collection and automatic EIR safe points have conservative callback
effects, including `MAY_THROW`, so optimizers must preserve surrounding catches
and cannot move collection across observable destructor-side work.
An escaping Throwable is retained as a collector root while the remaining
destructors, reachability recounts, and sweep finish. Multiple exceptions preserve
existing `previous` links and are chained without repeating an object identity.
Only after snapshot disposal, timing completion, and restoration of the collector
flags does the runtime rethrow to the caller. Eval uses an owned Throwable output
across its C boundary, never a native unwind through live Rust frames.

Descriptor-backed array callback wrappers transfer their temporary boxed argument
array to `__rt_callable_invoke_owned_args`. Its local native handler releases that
owner before returning the owned callback result or propagating a callback throw.
The boundary also preserves the caller's exception and diagnostic state. This
does not transfer ownership of the enclosing array helper's source snapshot or
partial result: those remain that helper's responsibility.

### Configurable heap size

The default heap is 128 MB (134217728 bytes, PHP's default `memory_limit`). For programs that need more (or less), use:

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

The wider slot is why a runtime helper that walks an indexed array cannot assume 8-byte payloads. `array_splice()` used to do exactly that on a string receiver, so the removed-elements array came back holding raw heap pointers surfaced as PHP integers; string arrays now go through the dedicated `__rt_array_splice_str` / `__rt_array_splice_insert_str` pair. A string array also owns its payloads exclusively — a copy-on-write split re-persists every slot and a deep free releases every slot — so moving a slot between two string arrays transfers ownership with it, while inserting one duplicates it with `__rt_str_persist`.

### Array growth

When `array_push` finds that `length >= capacity`, the array grows automatically:

1. `__rt_array_grow` first runs `__rt_array_ensure_unique`, so shared arrays split before any storage mutation.
2. It computes **2× capacity** (minimum 8) and the corresponding payload size.
3. If the unique allocation ends at the current bump tail and the extension fits in the configured heap, `__rt_heap_grow_tail` advances the bump offset, updates the block size and live-byte accounting, and keeps the same array pointer. This path does not copy elements or increment allocation/free counters.
4. Otherwise the existing fallback allocates a new array, copies the live elements, frees the previous unique storage, and returns the replacement pointer.

The caller always republishes the returned pointer, even though successful tail extension preserves it. A later live allocation, including a separate array, prevents tail ownership and therefore forces the allocate-copy-free fallback. Arrays remain dynamic within the configured heap limit. Direct indexed writes into empty arrays also grow the backing storage and extend `length` to cover the highest written index.

### Scalar indexed-array overwrites

`__rt_array_set_int` stores the eight-byte payload used by typed integer, float,
and boolean indexed arrays. After rejecting a negative index, it writes directly
and returns the same owner when the unsigned refcount is at most one and the
index is below the logical length. This leaf path needs no frame, allocation,
or copy-on-write helper call.

Shared arrays and writes at or beyond the logical length use the existing slow
path: establish a unique owner, normalize an empty array's scalar shape, grow
capacity when necessary, initialize gaps, and publish the new length. Callers
must still store the returned owner because separation or growth can relocate
the array. The same conditions apply on AArch64 and x86_64.

### Copy-on-write containers

Indexed arrays and associative arrays now follow **shared-until-modified** semantics:

1. Plain assignment or by-value argument passing shares the existing heap container and bumps its refcount
2. The first mutating write runs `__rt_array_ensure_unique` or `__rt_hash_ensure_unique`
3. If the refcount is already 1, the write proceeds in place
4. If the refcount is greater than 1, the runtime clones the container structure, retains nested heap-backed children (or re-persists immutable strings/keys), decrements the mutator's old owner slot, rewrites the mutating owner to the clone, and only then performs the write

For hashes, these comparisons use the logical PHP owner count: the physical reference count minus the header's internal lifetime pins. Pins retain storage through callbacks without introducing an extra PHP value owner.

This is what lets PHP-style code such as `$b = $a; $b[0] = 9;` leave `$a` unchanged without requiring deep copies on every assignment. Nested arrays and hashes remain shallow-shared until their own first mutation.

Copy-on-write splitting and indexed-array growth can relocate the container, so a mutating builtin must publish the returned pointer back into the place its receiver was read from. A plain local is its own frame slot; a **by-reference parameter** is read through a reference cell and must be republished through that cell. Hash growth preserves the separated header and replaces its entry allocation, but callers must still publish the result because the initial copy-on-write split may have changed the header.

## Hash table layout (associative arrays)

Associative arrays use a separate heap-allocated structure: an open-addressing hash table for lookup plus an insertion-order linked list threaded through the entries.

### Stable header (64 bytes)

```
┌──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┐
│  count   │ capacity │ val_type │   head   │   tail   │ entries  │   pins   │next_index│
│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (pointer)│ (8 bytes)│ (8 bytes)│
└──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┘
 offset+0   offset+8   offset+16  offset+24  offset+32  offset+40  offset+48  offset+56
```

| Field | Size | Description |
|---|---|---|
| `count` | 8 bytes | Number of occupied entries |
| `capacity` | 8 bytes | Total number of slots |
| `val_type` | 8 bytes | Coarse value-type summary (0=int, 1=str, 2=float, 3=bool, 4=array, 5=assoc, 6=object, 7=mixed, 8=null) |
| `head` | 8 bytes | Slot index of the first inserted entry, or `-1` when empty |
| `tail` | 8 bytes | Slot index of the most recently inserted entry, or `-1` when empty |
| `entries` | 8 bytes | Sole-owned pointer to a separate raw heap allocation containing the slots |
| `pins` | 8 bytes | Internal lifetime roots included in the physical refcount but excluded from PHP copy-on-write ownership |
| `next_index` | 8 bytes | Signed next automatic integer key; initially `PHP_INT_MIN`, advancing on integer insertion and saturating at `PHP_INT_MAX` |

Deleting an entry preserves `next_index`. Growth keeps the counter in the stable header, and COW clones copy it even when the highest inserted key is gone. Typed EIR, boxed Mixed, and eval appends consume the same native index rule. A never-indexed hash appends at zero; subsequent negative keys follow PHP's signed successor rule. At saturation, the maximum key can be reused after `unset`, but an occupied maximum raises `Error`. The nonthrowing `__rt_hash_try_next_index` probe reports the same exhaustion condition for native query registration.

Eval reconstructs an unset array as a hash, preserving integer gaps and copying exact index history after the retained entries. Its index callback returns a status without unwinding through Rust; Magician evaluates the RHS before choosing an index and schedules exhaustion through its own exception context. Zend `zval` packing and unpacking preserve the same counter in `nNextFreeElement`.

The header carries the associative array's reference count and heap kind. Entry storage has raw heap kind `0`; the garbage collector reaches its PHP values through the owning header. Deep release frees keys and values, then the entry allocation, then the header. Both allocations participate in heap accounting.

`__rt_hash_pin` retains one physical root and increments `pins`. `__rt_hash_unpin` removes the pin before releasing that root, so final release uses ordinary deep cleanup and may invoke destructors. Each pin must have one matching unpin. A pin keeps the hash reachable during cycle collection without changing ordinary PHP copy-on-write decisions. New hashes and COW clones start with zero pins; growth preserves the original header's pin count.

`__rt_hash_grow` first applies ordinary copy-on-write separation. Rehashing then replaces only the entry allocation. Internal construction writers can use `__rt_hash_grow_owned` to preserve aliases intentionally exposed while an output array is being initialized. This entry skips separation and must not be used for ordinary PHP array writes. Borrowed entry addresses must be reacquired after mutation.

The query-construction removal primitive, `__rt_mbstring_query_hash_remove`, pins its selected root, detaches the entry's key/value owners, repairs insertion-order links, and publishes the tombstone and updated count before invoking destructors. It preserves automatic-index history and exposed root aliases. A write-guard claim prevents double release when an enclosing construction destructor already owns the removed value. Detached owners and the final pin are released through `__rt_cleanup_call`; pending exceptions are returned after cleanup. The helper never accesses the removed entry after callbacks, so destructor insertions, growth, and retargeting remain intact.

`__rt_mbstring_query_hash_enter` returns one lifetime pin on a writable child hash. It preserves a unique child reached through unique ordinary Mixed boxes, excluding existing lifetime pins from PHP ownership. A shared box, shared child, or active write-guard borrow requires a shallow child copy. Persistent PHP reference wrappers are replaced by empty arrays for query entry; they are not traversed as ordinary storage. Dense children are promoted to hashes with retained contents. Replacements use `__rt_mbstring_query_hash_store_array`, the owned-array entry into the shared guarded construction writer. The child is pinned before old-value destruction, and its cursor remains valid after parent retargeting or pending cleanup. The caller must release that returned pin, including after status two.

`__rt_mbstring_query_register` forwards the V5 host's normalized field plan to the shared Rust `elephc_mbstring_query_apply_v1` executor with a versioned native storage table. The executor transfers each newly pinned child before releasing its parent cursor. A failed append probe stops the field before any later nesting-limit removal. For root removal, it first releases the nested cursor, then resolves the writer's live root again so destructor retargeting remains visible. It reports nesting overflow only when that removal instruction is reached. Pending exceptions preserve completed mutations and are returned after every published cursor is released through protected cleanup; fatal storage statuses stop execution. Root selection ignores non-array values and reports unsupported shared indexed promotion as a failure. Public `mb_parse_str()` reaches this callback through the native and eval query adapters; unsupported output-reference shapes fail before the shared executor mutates caller storage.

The complete native query host is `__rt_mbstring_query_invoke`. Its V5 table shares argument cloning/pinning, destructive reference initialization, diagnostics, result materialization, and request-lifetime adoption of displaced values with the V4 capture host. `MbNativeQueryV1` begins with the existing 16-byte capture state, followed by an independent policy context and configuration/filter pointers. The caller keeps this 40-byte record and its context alive and immutable through the call, except for the capture prefix's displaced-owner slot. A missing configuration callback disables query output before argument cloning or initialization; an absent filter selects identity filtering. Configuration and filtering receive their policy context, while ordinary value callbacks retain the original eval context. Owned policy-result leases must be compatible with the native host's existing release callback; static borrowed bytes can use a null owner. The non-unwinding invocation returns value/status/length/kind after cleanup. `__rt_mbstring_query_native` propagates a pending PHP exception only after that shared call returns. Public INI routing supplies the live query settings, the shared contract enforces argument guards, and the AOT or eval lvalue adapter publishes the resulting array into supported reference storage.

### Entries (64 bytes each)

Starting at the pointer stored at header offset `+40`, each slot is 64 bytes:

```
┌──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┐
│ occupied │ key_ptr  │ key_len  │ value_lo │ value_hi │ value_tag│   prev   │   next   │
│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│ (8 bytes)│
└──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┘
```

| Field | Description |
|---|---|
| `occupied` | 0 = empty, 1 = occupied, 2 = tombstone (deleted) |
| `key_ptr` | String key pointer, or the integer key payload when `key_len == -1` |
| `key_len` | String key length, or `-1` sentinel for integer keys |
| `value_lo` | Value (integer) or value pointer (string) |
| `value_hi` | String length (for string values), unused for single-word payloads |
| `value_tag` | Authoritative per-entry runtime tag used by lookup, iteration, JSON, search, and GC |
| `prev` | Previous inserted slot index, or `-1` for the head entry |
| `next` | Next inserted slot index, or `-1` for the tail entry |

### Hashing and collision resolution

String keys are normalized before lookup or insertion: PHP integer-form numeric strings become integer keys, while leading-zero strings such as `"01"` remain string keys. String keys are hashed with **FNV-1a** (fast, good distribution for short strings); integer keys use a scalar integer mix. Collisions are resolved by **linear probing** — if slot `hash % capacity` is occupied, try `(hash + 1) % capacity`, and so on.

Entry address: `load_pointer(base + 40) + (slot_index × 64)`. Codegen and runtime readers share this calculation through `hash_layout`.

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
| Header | 24 bytes | 64 bytes, with separate entry storage |
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

## Generator coroutine layout

`Generator` objects are heap-allocated object-kind blocks that reuse the
232-byte Fiber payload. The first word is the built-in Generator class id, so
ordinary `instanceof Generator` and `Iterator` checks still work. Offsets
`8..176` retain the Fiber state, native stack ownership, saved stack pointer,
callable/wrapper, caller, transfer and exception state, and seven boxed
`start_args` slots. The generator-specific fields occupy the Fiber layout's
reserved tail:

```
Offset  Size  Field
184      8    last_key boxed Mixed pointer
192      8    last_value boxed Mixed pointer
200      8    return_value boxed Mixed pointer
208      8    auto_key_counter
216      8    delegated_iter pointer used by `yield from`
224      8    unused reserved word
```

The constructor boxes the generator's parameters and closure captures into the
seven `start_args` slots, then the generated coroutine wrapper unboxes them into
the body's ordinary EIR function frame when execution starts. Locals therefore
live on the generator's mmap-backed native stack and survive `yield` because the
whole stack is suspended in place. The Mixed fields own boxed cells while
present. When a generator object is released, object deep-free detects
`_generator_class_id` and releases `last_key`, `last_value`, `return_value`, and
any active delegated iterator through the same refcounted runtime paths used
elsewhere; Fiber cleanup also returns the coroutine stack mapping to the OS.

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

These are **read-only** — the program never modifies them. When a string operation needs to work with a literal, it reads from the data section and writes the result to the [string buffer](#the-string-buffer-scratch-pad).

The runtime data layer is split into fixed shared data, user-program data, and dynamic `instanceof` lookup formatting under `src/codegen_support/runtime/data/`. Together they emit these static data tables:
- `_fmt_g` — printf format string for float-to-string conversion (`%.14G`)
- `_b64_encode_tbl` — 64-byte Base64 encoding lookup table
- `_b64_decode_tbl` — 256-byte Base64 decoding lookup table
- `_spl_autoload_exts_default`, `_spl_autoload_exts_ptr`, `_spl_autoload_exts_len` — mutable SPL autoload extension state
- `_heap_err_msg`, `_arr_cap_err_msg`, `_ptr_null_err_msg` — fatal runtime error strings
- `_buffer_bounds_msg`, `_buffer_uaf_msg`, `_buffer_alloc_size_msg`, `_buffer_registry_exhausted_msg`, `_match_unhandled_msg`, `_static_prop_private_access_msg`, `_instanceof_target_type_msg`, `_iterable_unsupported_kind_msg` — fatal runtime error strings for buffers, `match`, late-bound private static-property access, dynamic `instanceof` target validation, and iterable dispatch
- `_fiber_msg_*` — Fiber state-error message strings used when constructing `FiberError`
- `_rt_diag_suppression`, `_diag_fopen_failed_msg`, `_diag_file_get_contents_failed_msg`, `_diag_define_already_defined_msg` — runtime warning suppression depth and warning strings used by `@`
- `_resource_id_prefix` — prefix used by resource display helpers
- `_obj_handle_index`, `_obj_handle_free`, `_obj_handle_free_top` — direct heap-granule-to-object-handle index plus the LIFO pool of reusable PHP object handles
- `_obj_weak_reference_index` - one non-owning wrapper pointer per 16-byte heap granule, cleared before referent or wrapper reuse; `_weak_map_key_retire_fn` selects the live WeakMap retirement callback, and `_weak_reference_class_id` / `_weak_map_class_id` identify the emitted classes
- `_web_heap_guard_enabled` — enables per-request live-block accounting for persistent `--web` workers
- `_resource_id_keys`, `_resource_id_vals` — open-addressed native-resource-to-PHP-id map; resource ids and object handles deliberately use separate numbering spaces
- `_vd_indent`, `_vd_seen`, `_vd_seen_n` — current `var_dump()` indentation and its bounded recursion-detection stack
- `_debuginfo_guard`, `_debuginfo_pad` — set around a run-time `__debugInfo()` call made by `var_dump()`/`print_r()`. `_debuginfo_guard` holds the exception-handler top at the call (tagged with bit 0): a throw that reaches it without a newer handler is reported as PHP's uncaught warning, then `__debuginfo() must return an array`, exit 255, as in PHP. `_debuginfo_pad` says whether `var_dump()` had already written the value's indent before that report. `__rt_obj_debuginfo_fetch` saves both, with `_debuginfo_call_line`, in its frame and restores them after the call, so a dump nested inside `__debugInfo()` does not leak its values into the enclosing one.
- `_callable_strict_profile` — selects the strict-PHP callable builtin table when `--strict-php` is active
- `_php_uname_mode_len_msg`, `_php_uname_mode_value_msg` — fatal `php_uname()` diagnostics for invalid mode arguments
- `_filetype_*`, `_stat_key_*`, `_dirname_*`, `_pathinfo_key_*`, `_tmpfile_template` — file metadata, path, stat-array, and temporary-file lookup strings used by I/O helpers
- `_locale_utf8_name`, `_locale_env_name` — locale selectors used by runtime helpers that need host locale fallback
- `_json_true`, `_json_false`, `_json_null` — JSON keyword strings (4, 5, and 4 bytes) used by `json_encode` for boolean and null values
- `_json_int_max_str`, `_json_int_min_str` — decimal threshold strings used by `JSON_BIGINT_AS_STRING`
- `_json_err_msg_0` ... `_json_err_msg_10`, `_json_err_msg_table`, `_json_err_msg_count`, `_json_err_loc_prefix`, `_json_err_loc_colon` — `json_last_error_msg()` message lookup data and decode-location suffix fragments
- `_day_names` — 84-byte table (7 entries x 12 bytes each) with day names, lengths, and padding. Used by `date()` for day-of-week formatting
- `_month_names` — 144-byte table (12 entries x 12 bytes each) with month names, lengths, and padding. Used by `date()` for month formatting
- `_strtotime_keyword_tab`, `_strtotime_unit_tab` — keyword, weekday, modifier, and unit lookup tables used by `strtotime()`
- `_instanceof_target_count`, `_instanceof_target_entries`, `_instanceof_name_*` — case-insensitive class/interface name metadata for dynamic `instanceof` string targets, including leading-backslash aliases
- `_generator_class_id` — per-program class id used to recognize Generator frames during object deep-free
- `_json_exception_class_id`, `_stdclass_class_id` — per-program class ids used by JSON throw paths and stdClass dynamic-property helpers
- `_class_gc_desc_count`, `_class_gc_desc_ptrs`, `_class_gc_desc_<id>` — per-class property traversal descriptors used by object deep-free and cycle collection
- `_class_json_desc_ptrs`, `_class_json_desc_<id>`, `_class_json_pname_<id>_<slot>` — per-class JSON descriptors used by object encoding and JsonSerializable dispatch
- `_class_tostring_count`, `_class_tostring_ptrs` — dense per-class `__toString` method table used by runtime string coercions of boxed `mixed` objects
- `_class_iface_method_count` (plus per-interface method tables), `_class_serprop_declaring_ptrs`, `_class_serprop_declaring_missing`, `_class_serprop_declaring_<id>` — interface-method ordering and property declaring-class tables used by `get_object_vars()` visibility filtering and object-to-array projection
- `_class_serpdiag_ptrs`, `_class_serpdiag_<id>`, `_class_serpdiag_missing`, `_class_serpsuffix_<id>_<slot>` — per-class serialize-property hydration guard read by `__rt_obj_store_prop`: the declared-type message suffix, the set of accepted boxed value tags, and the flag to widen an accepted `int` into a `float`
- `_class_attribute_count`, `_class_attribute_ptrs`, `_class_attributes_<id>` — per-class PHP attribute metadata emitted from `ClassInfo`; current helper and Reflection APIs materialize supported static lookups during codegen instead of performing dynamic runtime class/member lookup
- `_class_vtable_ptrs`, `_class_vtable_<id>` — per-class virtual tables used for inherited instance-method dispatch
- `_class_static_vtable_ptrs`, `_class_static_vtable_<id>` — per-class static-method tables used for late static binding
- `_class_destruct_ptrs` — class_id-indexed `__destruct` method pointers (or `0`) consulted during object deep-free
- `_classes_by_name`, `_classes_by_name_count` — case-insensitive class-name lookup table used by `new $variable()` instantiation
- `_zlib_fwrite_fn`, `_zlib_close_fn`, `_bz2_fwrite_fn`, `_bz2_close_fn`, `_iconv_fwrite_fn`, `_iconv_close_fn` — late-bound stream compression and iconv bridge entry points
- `_phar_zlib_inflate_init2_fn`, `_phar_zlib_inflate_fn`, `_phar_zlib_inflate_end_fn`, `_phar_bz2_decompress_fn` — late-bound PHAR decompression entry points
- `_elephc_tls_connect_fn`, `_elephc_tls_connect_insecure_fn`, `_elephc_tls_connect_cafile_fn`, `_elephc_tls_connect_capath_fn`, `_elephc_tls_connect_peer_name_fn`, `_elephc_tls_connect_client_cert_fn`, `_elephc_tls_attach_fd_fn`, `_elephc_tls_attach_fd_client_cert_fn`, `_elephc_tls_read_fn`, `_elephc_tls_write_fn`, `_elephc_tls_close_fn` — late-bound TLS session entry points
- `_elephc_crypto_hash_fn`, `_elephc_crypto_hmac_fn`, `_elephc_crypto_init_fn`, `_elephc_crypto_update_fn`, `_elephc_crypto_final_fn`, `_elephc_crypto_clone_fn`, `_elephc_crypto_free_fn`, `_elephc_crypto_is_finalized_fn`, `_elephc_crypto_cipher_iv_length_fn`, `_elephc_crypto_cipher_methods_fn`, `_elephc_crypto_encrypt_fn`, `_elephc_crypto_decrypt_fn` — late-bound hashing, incremental-context, and OpenSSL-compatible symmetric-crypto entry points
- `_elephc_bcmath_add_fn`, `_elephc_bcmath_sub_fn`, `_elephc_bcmath_mul_fn`, `_elephc_bcmath_div_fn`, `_elephc_bcmath_mod_fn`, `_elephc_bcmath_divmod_fn`, `_elephc_bcmath_pow_fn`, `_elephc_bcmath_powmod_fn`, `_elephc_bcmath_sqrt_fn`, `_elephc_bcmath_comp_fn`, `_elephc_bcmath_ceil_fn`, `_elephc_bcmath_floor_fn`, `_elephc_bcmath_round_fn`, `_elephc_bcmath_get_scale_fn`, `_elephc_bcmath_set_scale_fn`, `_elephc_bcmath_last_error_fn`, `_elephc_bcmath_free_fn` — late-bound decimal bridge entry points shared by arithmetic lowering and error/result marshalling
- `_elephc_iconv_call_fn`, `_elephc_iconv_release_fn`, and `_elephc_curl_*_fn`: late-bound character-set conversion and curl easy, multi, share, callback, and multipart bridge entry points
- enum-case `.comm` symbols produced via `enum_case_symbol(...)` — one 8-byte singleton storage slot per declared enum case

### Request and cleanup metadata

The fixed runtime also reserves the following BSS metadata. Every symbol in
this table occupies 8 bytes; queue nodes, handler-stack entries, and temporary
values themselves live on the heap. These slots supplement the scratch buffers
and heap counters described above.

| Symbols | Purpose |
|---|---|
| `_mbstring_catalog_array` | Cached native mbstring catalog array |
| `_mbstring_deferred_capture_head`, `_mbstring_deferred_capture_tail`, `_mbstring_deferred_capture_draining` | Deferred capture queue and its reentrancy guard |
| `_mbstring_ini_native_active` | Native mbstring INI-state activation marker |
| `_unser_temporaries` | Temporary-value cleanup state for unserialization |
| `_elephc_eval_ob_release_fn` | Eval output-handler retirement callback, including owned Throwable output |
| `_php_backtrace_next_line` | Source line for the next native backtrace frame |
| `_gc_pending_throw` | Pending exception escaping cycle-collector cleanup |
| `_hash_write_guard_top` | Active hash-mutation cleanup guard chain |
| `_gc_runs`, `_gc_collected` | Productive collection count and cumulative collected nodes |
| `_gc_application_started`, `_gc_collector_started`, `_gc_free_started`, `_gc_destructor_started` | Start timestamps for application and collection phases |
| `_gc_collector_time`, `_gc_destructor_time`, `_gc_free_time`, `_gc_destructor_depth` | Accumulated phase durations and nested destructor depth |
| `_php_error_handler_value`, `_php_error_handler_callable`, `_php_error_handler_mask` | Original PHP handler value, normalized callable, and accepted error levels |
| `_php_error_handler_context`, `_php_error_handler_context_release`, `_php_error_handler_stack` | Eval handler context, its release callback, and prior error-handler registrations |
| `_php_exception_handler_value`, `_php_exception_handler_callable` | Original PHP exception-handler value and normalized callable |
| `_php_exception_handler_context`, `_php_exception_handler_context_release`, `_php_exception_handler_stack` | Eval handler context, its release callback, and prior exception-handler registrations |

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

### Static properties (`ClassName::$prop`)

Static properties are class-scoped storage rather than object fields. During `emit_runtime_data_user()`, each effective declaring class property gets one 16-byte BSS slot:

```asm
.comm _static_prop_Counter_count, 16, 3 ; 16 bytes for Counter::$count
```

The naming pattern comes from `static_property_symbol(...)`. Inherited static properties point back to the declaring class slot, so `Base::$count` and `Child::$count` share storage when the property is declared only on `Base`. When a subclass redeclares the static property, that subclass receives its own slot and `static::$count` dispatches to it through the called-class id at runtime. `_main` evaluates static-property defaults before user statements run, and later reads/writes load from or store to the resolved slot directly.

A `buffer<T>`, `ptr` or `ptr<T>` static property keeps its handle or address in the slot's low word, as an instance property does. The word owns nothing: overwriting it frees no buffer, and shutdown releases nothing for it. Element writes (`Class::$buf[$i] = $v`) resolve the handle the slot holds and store into the buffer payload without writing the slot back. A typed pointer slot whose default is `null` is initialized to the zero word, while a typed buffer or pointer slot without a default carries the uninitialized marker in its high word until the first store.

Converting a boxed `Mixed` value into a concrete scalar, string, or object property does not transfer the source box. EIR releases an owning temporary after the store and leaves borrowed boxes untouched. Scalar payloads need no owner; strings are persisted exactly once and object payloads are independently retained. Matching boxed-to-boxed storage instead transfers an owned box, or acquires a borrowed source before storing it.

### Pending compound append leases

Plain nested assigns with converted keys use the same leases after publishing each parent.
Their generated live-guard Assign markers are consumed by EIR before statement lowering:
the apparent copy never acquires another owner and never reads a missing child. Key
conversion precedes the guard, so a throwing diagnostic retains the published parent,
and a returning diagnostic can invalidate a selection that no PHP owner preserves.

Parser-generated append cell aliases are ordinary retained Mixed boxes, not persistent PHP
reference sets. When a child lease replaces an older compiler parent, EIR pops the parent's
record, publishes the child, then releases the old parent slot. This keeps unwinding safe
without making dead compiler ancestors look like live PHP owners.

A selected object child retains that same rooted cell while EIR dispatches an append
through `ArrayAccess::offsetSet(null, value)`. The method borrows the receiver and
RHS roots; ordinary array/null children instead use the consuming boxed append helper.
Both branches retire the caller's roots after mutation and expose them to exception
unwinding when a user method throws.
Callable payloads are Closure objects but have no class-table object header. The boxed
setter releases a rejected value before entering the shared class-name Error helper,
rather than reading a callable descriptor as an object or dropping the write.

A fresh parent is preallocated with the indexed or associative representation required by
its next key before the missing-key callback runs. Around conversion and reads, a typed, non-retaining
EIR query counts both cell and payload owners. Their sum must be exactly three:
one pending compiler cell lease, its payload reference, and one surviving PHP owner.
A stored cell with an unshared payload remains writable, while a detached cell can
continue through exactly one independently boxed payload copy. Zero or multiple PHP
owners make the pending target stale. A final-place failure bypasses the operator and
yields null (or 1 for prefix increment). A non-final failure instead rebinds only the
compiler lease to a fresh empty Mixed array or hash, never to a saved PHP copy. A separate
keeper owns that scratch across child-lease transitions; a detached flag bypasses later
owner checks for this expression. Later keys, diagnostics, and the leaf operation still run.
If conversion caused the first loss, only that dimension's missing-key read is skipped.
A final conversion loss cancels before reading the missing leaf. Multiple root copies
that share one child slot do not add independent child owners. The detached-payload
case uses dedicated array conversion and hash insertion entries that preserve the already
selected payload instead of crossing another COW boundary. Both normal and cancellation
branches retire active leases in LIFO order. ARM64 and x86_64 use the same ownership
rules; non-array offsets keep their ordinary dispatch.

Nested write lowering captures computed keys and property receivers in source order, roots
them across RHS evaluation, and traverses writable parents afterwards. Each child is published
before an older receiver or key owner is retired. Short internal handlers preserve a pending
false-conversion exception through key-sensitive opcode completion, then transfer that
exception or chain it under a new key error without leaving an owner in both the frame and the
in-flight exception cell.

Local and static false-parent updates query the selected Mixed cell's reference count after notification
without acquiring another reference. A count of one means only the pending write pin remains;
the leaf read is skipped and an owned null is selected instead. The ARM64 and x86_64 emitters
read the same uniform heap header, including on the iOS targets.

## Memory limits and trade-offs

| Resource | Size | What happens when full |
|---|---|---|
| Stack | OS default (~8MB) | Stack overflow (crash) |
| String buffer | `--heap-size` (default 8 MB) | Resets each statement — bounded by the program budget |
| print_r capture | `_print_r_buf` = 64KB inline; `_print_r_base`/`_print_r_cap` track the current block; `_print_r_mode`, `_print_r_off` = 8 bytes each | `print_r($value, true)` renders into the capture buffer and grows past the inline block through `__rt_concat_grow`, so the full rendered string is returned |
| Output-buffer (`ob_*`) stack | `_ob_level`, `_ob_in_handler`, `_ob_flushing`, `_ob_implicit_flush` = 8 bytes each; `_ob_ptrs`, `_ob_lens`, `_ob_caps`, `_ob_handler_stubs`, `_ob_handler_envs`, `_ob_name_ptrs`, `_ob_name_lens`, `_ob_chunk_sizes`, `_ob_flags`, `_ob_started` = 512 bytes each (64 slots) | Up to 64 nested `ob_start()` levels; the buffer bodies themselves are heap-allocated (16KB default capacity, page-aligned chunk size + 1 when a chunk size is set, grown by doubling) |
| Web capture flag | `elephc_web_capture` = 8 bytes | `--web` bridge output-capture flag read by `__rt_stdout_write`; zero selects the plain `write(1, ...)` syscall path |
| Eval bridge hook slots | `_elephc_eval_ob_handler_fn`, `_elephc_eval_dynamic_object_destruct_fn`, `_elephc_eval_closure_bind_fn`, `_elephc_eval_wrap_callback_fn`, `_elephc_eval_object_gc_child_fn`, `_elephc_eval_object_release_fn`, `_elephc_eval_array_reference_retire_fn`, `_elephc_eval_object_clone_fn` = 8 bytes each | Late-bound Magician callbacks for eval-managed handlers, closures, callbacks, objects, references, and cloning |
| Heap | 128 MB (configurable) | Fatal error: "heap memory exhausted" |
| Heap and GC metadata | `_heap_off`, `_heap_free_list`, `_heap_debug_enabled`, `_heap_max`, and each `_gc_*` flag, counter, pointer, or timer = 8 bytes each; `_heap_small_bins` = 2048 bytes; `_heap_bin_marks` = heap size / 16 + 2 bytes | Fixed-size allocator, cycle-collector, destructor, pin, and timing bookkeeping |
| Weak-reference identity index | `_obj_weak_reference_index` = `(heap_size / 16 + 2) * 8` bytes, or 64 MiB plus 16 bytes for the default 128 MiB heap | One non-owning wrapper pointer per heap granule; it neither keeps keys alive nor acts as a collector root |
| Buffer descriptors | `_buffer_registry` = 196656 bytes (4097 descriptors × 48 bytes), `_buffer_registry_free` = 8 bytes, `_buffer_registry_next` = 8 bytes | Up to 4096 live buffers; freed descriptors are recycled with a new generation, while exhaustion aborts with `Fatal error: buffer registry exhausted` (an invalid requested size aborts earlier with `Fatal error: buffer_new() length is negative or exceeds the maximum buffer size`) |
| Exception and user-handler state | `_exc_handler_top`, `_exc_call_frame_top`, `_exc_value`, `_magic_set_guard_head`, and every `_php_error_handler_*` / `_php_exception_handler_*` slot = 8 bytes each | Active exception state and current user handlers; nested handler registrations use heap-backed linked stacks |
| Fiber scheduler state | `_fiber_current`, `_fiber_main_saved_sp`, `_fiber_main_saved_exc`, `_fiber_main_saved_call_frame`, `_fiber_main_saved_magic_set_guard` = 40 bytes total | Current-fiber and main-frame resume bookkeeping, including the magic-set recursion guard |
| Runtime diagnostics | `_rt_diag_suppression`, `_rt_diag_pending_ptr`, `_rt_diag_pending_len`, `_php_diagnostic_file`, `_php_diagnostic_file_len`, `_php_diagnostic_line` = 8 bytes each | Warning suppression plus deferred diagnostic message and source-location state |
| Resource inventory | `_resource_inventory_head`, `_resource_inventory_tail` = 16 bytes total | Heap-backed history used by `get_resources()`, including closed incarnations after descriptor reuse |
| Unmanaged reference-borrow state | `_rt_unmanaged_ref_borrow_top` = 8 bytes | Active boxed array-walk borrow chain used to reject escaping unmanaged element references |
| JSON state | `_json_last_error`, `_json_active_flags`, `_json_active_depth`, `_json_indent_depth`, `_json_depth_limit`, `_json_validate_idx`, `_json_validate_ptr`, `_json_validate_len`, `_json_decode_assoc`, `_json_error_source_ptr`, `_json_error_source_len`, `_json_error_candidate_ptr`, `_json_error_location_active`, `_json_error_line`, `_json_error_column`, `_json_error_scan_count`, `_json_error_scan_bytes` = 136 bytes total | Fixed-size bookkeeping for JSON calls and decode error locations. The candidate pointer is recorded in constant time; line and column are scanned from the original source only when an error is recorded. |
| Serialize/unserialize state | `_ser_value_counter`, `_ser_obj_count`, `_ser_legacy_active`, `_unser_count`, `_unser_legacy_active` = 8 bytes each; `_ser_obj_ptrs`, `_ser_obj_idxs`, `_unser_values` = 512KB each; `_unser_depth`, `_unser_allowed_mode`, `_unser_allowed_list`, `_unser_allowed_list_mixed`, `_unser_active`, `_unser_context` = 8 bytes each | `serialize()` object-dedup counters/maps and `unserialize()` reference registry, plus the decode depth limit, `allowed_classes` policy/list, active flag, and reentrancy snapshot; overflow degrades gracefully (serialize stops deduping, unserialize fails the ref) and reentrant decodes restore the outer context while legacy Serializable callbacks share references |
| Date/time state | `_strtotime_clock`, `_php_default_tz_len` = 8 bytes each; `_strtotime_iso_buf` = 64 bytes; `_php_tz_env`, `_php_tz_save` = 264 bytes each | `strtotime()` clock override, the scratch copy a one-digit-month/day ISO date is zero-padded into, plus default-timezone (`date_default_timezone_*`) env/save buffers and stored identifier length |
| CLI globals | `_global_argc`, `_global_argv` = 16 bytes total | Fixed-size bookkeeping |
| User globals | 16 bytes per `global $var` slot | Grows with number of referenced globals |
| Static vars | 24 bytes per `static $var` (`16 + 8 init flag`) | Grows with number of declared static locals |
| Static properties | 16 bytes per effective declaring class static property | Grows with number of declared and redeclared static properties |
| Array capacity | Fixed at creation until grow/re-hash logic runs | `__rt_array_new` / `__rt_hash_new` validate the requested size first: negative capacities allocate an empty payload region, and a `capacity * elem_size` (or `capacity * 64`) that does not fit in a non-negative machine word aborts with "Fatal error: requested array size exceeds the maximum allowed array size" instead of wrapping to a small allocation |
| Range element count | `range()` sizes its result from `\|end - start\| + 1` | An interval needing more than 1073741823 elements never reaches the allocator: the lowering guard raises reference PHP's catchable `ValueError` ("The supplied range exceeds the maximum array size: start=… end=… step=…") first, naming the ordered endpoints and `abs($step)`. The runtime's own overflow abort remains as the backstop |
| Fill element count | `array_fill()` sizes its result from `$count` | A `$count` past `INT_MAX` (2147483647) never reaches the allocator: the lowering guard raises reference PHP's catchable `ValueError` ("array_fill(): Argument #2 ($count) is too large"). Counts at or below the bound whose payload does not fit the heap still report "Fatal error: heap memory exhausted", which is what reference PHP reports as a memory-limit fatal too |
| Buffer length | Fixed at `buffer_new<T>()` | A negative length, a `length * stride` that does not fit in a non-negative machine word, or a runtime-converted length (int arithmetic, `mixed`, `int\|float`) that is not a whole number in the `int` range, aborts with "Fatal error: buffer_new() length is negative or exceeds the maximum buffer size" |
| C-string buffers | `_cstr_buf`, `_cstr_buf2` = 4KB inline each; `_cstr_base`/`_cstr_cap` and `_cstr2_base`/`_cstr2_cap` track the current block; `_empty_str` = 1 byte | A path, pattern, or command longer than 4KB grows into an owned block through `__rt_cstr_grow`/`__rt_cstr2_grow` instead of being truncated or overwriting adjacent runtime state; `_empty_str` is a safe zero-length string pointer |
| File descriptor state | `_eof_flags`, `_stream_read_filters`, `_stream_write_filters` = 256 bytes each; `_popen_files`, `_dir_handles`, `_glob_handles`, `_zstream_handles`, `_bzstream_handles`, `_iconv_handles`, `_tls_sessions`, `_stream_chunk_size` = 2048 bytes each; on Apple targets only, `_iconv_write_options` = 256 bytes | Per-fd stream, process, directory, compression, iconv, TLS, and chunk-size bookkeeping for up to 256 descriptors; `_iconv_write_options` holds each `convert.iconv` write filter's own `//TRANSLIT`/`//IGNORE` bits |
| Stream filter scratch | `_stream_filter_buf`, `_stream_grow_scratch` = 64KB each | Scratch space for stream filters, including length-growing filters such as base64 and quoted-printable encoders |
| Stream context and callbacks | `_stream_context_options`, `_stream_notification_callback`, `_stream_connect_host`, `_stream_open_opened_path_scratch`, `_url_stat_matched` | Current stream-context options hash, notification callback, TLS peer host, wrapper opened-path scratch, and wrapper url_stat match flag |
| TLS, compression, iconv, curl, crypto, and BCMath function slots | `_elephc_tls_*_fn`, `_zlib_*_fn`, `_bz2_*_fn`, `_phar_zlib_*_fn`, `_phar_bz2_*_fn`, `_iconv_*_fn`, `_elephc_iconv_*_fn`, `_elephc_curl_*_fn`, `_elephc_crypto_*_fn`, `_elephc_bcmath_*_fn` = 8 bytes per slot | Late-bound function pointers keep optional native support pay-for-use and are published only when the corresponding bridge is linked |
| HTTP/HTTPS/FTP buffers | `_http_resp_buf`, `_https_resp_buf`, `_user_wrapper_drain_buf`, `_phar_write_out` = 1MB each; `_http_req_scratch` = 8KB; `_http_redirect_path_buf`, `_fgc_url_retr` = 2KB each; `_fgc_url_addr`, `_fsockopen_addr` = 512 bytes each; `_ftp_resp_buf` = 4KB; `_ftp_data_addr`, `_ftp_cmd_scratch` = 64 bytes each; `_ftp_use_tls` = 8 bytes (FTPS handshake flag) | Protocol-specific response, request, redirect, FTP/FTPS, wrapper, and PHAR writer scratch buffers and flags |
| HTTP active context | `_http_active_ignore_errors`, `_http_active_max_redirects`, `_http_active_timeout_seconds`, `_http_active_proxy_ptr`, `_http_active_proxy_len`, `_http_active_host_ptr`, `_http_active_host_len`, `_http_redirect_path_len` | Fixed-size state shared between HTTP request construction and redirect/open helpers |
| Socket address scratch | `_recvfrom_addr_ptr`, `_recvfrom_addr_len`, `_accept_peer_ptr`, `_accept_peer_len` = 8 bytes each | Stores peer/address strings returned through by-reference socket parameters |
| Protocol/service lookup buffers | `_protoent_buf` = 32KB, `_servent_buf` = 1MB | Scratch buffers for protocol and service database lookups |
| Principal lookup buffers | `_principal_lookup_buf` = 4KB, `_etc_passwd_path`, `_etc_group_path`, `_principal_lookup_read_mode` | Scratch line buffer and fixed literals for `/etc/passwd` / `/etc/group` scans used by `chown()` / `chgrp()` string-name resolution without NSS |
| User wrapper and filter registries | `_user_wrappers`, `_user_wrapper_handles` = 2048 bytes each; `_user_filter_registry`, `_user_filter_instances` = 4096 bytes each | Registered stream wrappers, active wrapper handles, user filter definitions, and attached filter instances |
| PHAR writer state | `_phar_write_len`, `_phar_write_tpl_len`, `_phar_write_path_ptr`, `_phar_write_path_len`, `_phar_write_entry_ptr`, `_phar_write_entry_len`, `_phar_write_url_ptr`, `_phar_write_url_len` = 8 bytes each | State paired with the 1MB `_phar_write_out` archive buffer |
| Data section | No fixed limit | Grows with number of unique literals |

## Memory management strategy

The opt-in web application lifecycle keeps this heap valid for the worker.
Its registered application, ordinary globals, and statics remain owning roots.
Request functions release local owners, the boundary detaches and releases
superglobals, then explicit cycle collection and resource inventory pruning
retire remaining request graphs. Newly stored application values follow normal
string persistence, reference counting, and COW; no boot-allocation immortality
or request-arena pointer escape is involved. The default web request lifecycle
continues to reclaim its entire arena. See
[Persistent Web Applications](../beyond-php/web-application.md).

elephc uses a **free-list allocator with reference counting plus a targeted cycle collector** — not pure bump-allocation, and not a whole-heap tracing runtime either. Memory is reclaimed in specific situations:

1. **Reference counting** — every heap allocation carries a 32-bit refcount (initialized to 1). When a reference is shared, `__rt_incref` increments it. When a reference is dropped, `__rt_decref_array`, `__rt_decref_hash`, or `__rt_decref_object` decrements it and frees the block when it reaches zero
2. **Copy-on-write splitting for arrays/hashes** — plain assignment still shares container storage, but the first mutating write clones a shared container before modifying it
3. **Codegen ownership tracking** — locals, globals, statics, `foreach` variables, `list(...)` targets, and call arguments are classified as owned or borrowed at compile time so new owners retain borrowed heap values before storing them
4. **Variable reassignment** — when `$x = "new value"` overwrites a string or array, the old heap block is freed or decreffed and returned to the allocator for reuse
5. **`unset($x)`** — explicitly frees the variable's heap allocation
6. **FFI string-call cleanup** — temporary C strings created for `extern function foo(string $s)` calls are released immediately after the native call returns
7. **String buffer reset** — the concat buffer resets at each statement, with strings that need to survive copied to heap via `__rt_str_persist`
8. **Stack memory** — automatically reclaimed when functions return
9. **Generator frame release** — Generator frames participate in object refcounting, with a custom deep-free branch for their frame slots and delegated iterator
10. **Resource scope-cleanup** — Mixed-boxed resources (tag 9) carry a resource-kind subtype in the high payload word, and `__rt_mixed_free_deep` runs the matching destructor when the box is released: kind 1 = native stream fd (`close()`), kind 2 = HashContext handle (`elephc_crypto_free` through `__rt_hash_ctx_free`), kind 3 = `popen` pipe (`__rt_pclose`, which closes the `FILE*` and reaps the child), kind 4 = `opendir` stream (`__rt_closedir`). Kind 0 resources (generic resources) are skipped, and every fd-backed kind also skips handles `>= 0x40000000` — synthetic wrapper handles and the `-1` sentinel that an explicit `fclose`/`pclose`/`closedir` stamps into the box so the descriptor is never released twice (even if its fd number was reused). Alias safety comes from the Mixed box refcount — `$b = $a` increfs the box, so only the last release triggers the destructor
11. **Process exit** — all memory reclaimed by the OS

### JSON decoder construction owners

The native JSON parser builds an acyclic graph of fresh Mixed cells and
containers. Array insertion retains a borrowed child, so the parser releases
its original child reference immediately after insertion. Hash insertion
adopts the fresh value cell and persists the key bytes; the temporary decoded
key cell is released after insertion. Boxing a fresh array, hash, or stdClass
retires the builder's original container reference after Mixed acquires its own.

Recursive parse failures return normally while each parser frame releases its
partial container and any pending key. The outer `__rt_json_decode_mixed`
boundary temporarily masks only `JSON_THROW_ON_ERROR`, restores the caller's
flags after recursion, and then publishes the recorded exception. Parsing
invokes no PHP callbacks, so the temporary flag mask cannot be observed by user
code. Diagnostics retain the original parse error while cleanup completes.

Positional source strings are persisted into owned evaluation roots before
later arguments run. Those roots retire on success, parser exceptions, and
exceptions raised while evaluating a later argument. A decoded alias remains
alive through its own reference; releasing the parser's construction references
does not bypass normal copy-on-write or final-owner destruction.

### Unboxing a Mixed hands back a reference

`Op::MixedUnbox` extracts concrete storage from a checked Mixed source. Its callers include typed local/property assignment (`coerce_typed_assign_value`), call-argument and closure coercion, static method-call receivers, typed foreach slots, object-return coercion, and merge temps that retain a `??`-kept `?C` cell as an object or callable (#1628). Its lowering ends in `emit_unbox_mixed_to_owned_refcounted_result`, so a refcounted payload carries its own reference rather than borrowing the box's.

Call parameters declared `iterable` receive a raw array, hash or Traversable pointer, not the surrounding Mixed cell. After runtime argument validation, their layout-neutral unbox acquires the payload before the source lease is released. Enum interface calls also keep static-unpack arrays and projected values rooted while checking types before a later arity shortage. A typed variadic collector promoted to Mixed storage must pass through an EIR unpack/rebox adapter; forwarding its raw words would confuse typed element storage with boxed entries and lose named keys or actual counts.

An enum interface call zeroes a heap-result slot before publishing its cleanup
record. Every candidate may reject argument binding before storing a result, so
the guard cannot rely on a later successful store to make frame initialization
include that slot. `ZeroLocalSlot` initializes both string words, or the single
array/object pointer, and participates in the backend's local-store analysis.

An object return can consume an independent Mixed assignment-result cell, not just borrow a frame local. Return coercion first acquires the object payload and then releases that owned cell. Borrowed parameter/local cells retain their existing frame cleanup instead; releasing them as independent return temporaries would retire the same owner twice.

That makes it an owning temporary, and `LoweringContext::value_is_owning_temporary` says so. The distinction is not cosmetic: while the unboxed value was read as a borrow, the store that consumed it acquired a second reference, and the one slot release in the function epilogue never balanced it. The unboxed object stayed alive for the rest of the process, and so did everything it owned — five heap blocks per call for a `ReflectionAttribute`, whose name string and argument array go with it.

The shape to watch for when adding another unbox site: an acquire is only correct on top of a BORROW. Ask what the producing op's lowering already did before pairing one with it.

The property-write caller has a leak of its own that this does not reach — three blocks per `$obj->untypedProp = $mixed;` where the property is refined to an object type, measured identically with the predicate on and off (#1115).

### What is NOT freed

- **Non-adjacent free blocks** are still not compacted — fragmentation can still occur over time even though adjacent neighbors are coalesced on free and oversized free blocks are split on allocation
- **Pointer targets** are not ownership-tracked just because a raw pointer exists; the pointer value itself is only an address
- **Intermediate scratch strings** in `_concat_buf` are not individually freed — the buffer is simply reset per statement
- **General function epilogues** do not blanket-decref all heap locals. They now selectively clean up slots proven `Owned`, and exhaustive `if` / `elseif` / `else` branches can restore that cleanup when every fallthrough branch directly assigns the same heap-backed type to the same local. More dynamic borrowed/control-flow paths still remain excluded on purpose
- **Container-copying builtins** no longer blindly duplicate borrowed heap handles for common nested payload paths: refcounted runtime variants now retain values before new arrays/hash tables take ownership (`array` literals with spreads, `array_merge`, `array_chunk`, `array_slice`, `array_reverse`, `array_pad`, `array_unique`, `array_splice`, `array_diff`, `array_intersect`, `array_filter`, `array_fill`, `array_combine`, `array_fill_keys`)
- **Regression coverage now explicitly exercises** local aliases, borrowed nested-container returns, `Owned`/`Borrowed` control-flow merges, and scope-exit paths so future ownership work has focused tripwires instead of relying only on large end-to-end suites
- **Copies during protected destruction** can observe a Mixed cell whose last owner has already transferred to child release. `__rt_reference_array_copy` clones that borrowed cell's PHP value instead of retaining its zero-owner storage. Hash copies can retain an object whose destructor is active and read it after the original capture completes. Copying nested containers already partway through their own child destruction remains incomplete.
- **Raw/off-heap ownership cycles** are still outside the collector. `ptr` values, extern-managed buffers, and raw helper allocations (`kind=0`) are not traversed just because an address exists somewhere
- **Kind-0 resources** (generic/unknown resource kind, including synthetic user-wrapper handles `>= 0x40000000`) are not auto-freed by the Mixed deep-free path — their lifecycle remains managed by the wrapper layer or the user's explicit `close()` call. Kinds 1–4 (native stream fd, HashContext, `popen` pipe, `opendir` stream) are auto-released at scope exit
- **HashContext reuse after `hash_final()`** is memory-safe but not PHP-equivalent: `elephc_crypto_final` finalizes a *clone* and leaves the original handle live and owned by its Mixed box, so the box's kind-2 destructor frees it exactly once. A second `hash_final()` or a `hash_update()`/`hash_copy()` on the same handle therefore does not double-free or use-after-free (where PHP throws "Supplied resource is not a valid Hash Context resource"), it simply keeps hashing the still-live context (documented in `src/codegen_support/runtime/strings/hash_context.rs`)

### Targeted cycle collection

The runtime now includes a targeted collector for heap-backed `array`, associative-array/hash, and `object` graphs:

- the allocator header carries a uniform heap-kind tag (`raw`, `string`, `array`, `hash`, `object`, `boxed mixed`, `throwable`, `owned reference cell`)
- indexed arrays pack their runtime `value_type` into the same kind word so the collector knows whether their elements can contain nested heap pointers
- objects record runtime property tags/metadata, with `_class_gc_desc_*` tables as a compile-time fallback for property traversal; Generator frames are object-kind blocks with a custom deep-free branch keyed by `_generator_class_id`
- owned property references form `object -> cell -> value` graph edges; a local cell alias therefore roots the referenced graph independently of the original object
- mixed release paths use `__rt_decref_any`, so deep-free and GC walks can release nested strings/arrays/hashes/objects through one uniform dispatcher

`__rt_gc_collect_cycles` is intentionally narrower than a full tracing GC: it ignores strings and raw helper buffers as candidates, clears transient marks, counts heap-only incoming edges, and marks externally reachable container/object blocks. An object-destructor phase pins graph candidates before running PHP, then repeats root analysis before freeing the unmarked remainder. Heap-kind bit 18 identifies an artificial pin, distinct from snapshot membership bit 17, destructor completion bit 14, and reachability bit 16. `_gc_collecting` suppresses recursive collection throughout these phases; `_gc_freeing_unreachable` suppresses decrements of doomed graph children only during the final sweep. Destructor-side writes therefore update real ownership normally. `_gc_pin_head` links C-allocated snapshot chunks outside the PHP heap. This remains a non-moving collector for structural cycles, not Zend collector-buffer parity.

On x86_64, one outgoing-edge traversal caches incoming counts in transient
kind-word bits 19 through 30. The high 32-bit ELPH marker and actual refcounts
remain intact. Counts saturate at 4095; a saturated candidate uses the original
full-precision rescan of the selected graph before any root decision. Already reachable nodes
need no additional root classification. Each root scan clears cached fields
before destructor callbacks, and destructor mutations trigger fresh counting.
The ARM64 collector continues using its existing 32-bit incoming-count field.
Both ABIs enumerate retained eval-object children through the same callback.

Native callable descriptors are managed kind-9 graph nodes. Every dynamic producer
initializes capture storage and stamps that kind; static descriptors remain outside
the live heap. Callable slots (value tag 10) in arrays, hashes, object properties,
Mixed boxes and reference cells contribute real ownership edges. The shared capture
visitor mirrors final release metadata, including nested descriptors and by-reference
capture cells normalized through `__rt_reference_cell_owner`. This helper validates
the exact live allocation through the allocator bitmap before checking its kind,
with constant work independent of heap size. Nonfinal descriptor
releases queue candidates. Final sweep invokes protected descriptor teardown and
suppresses only doomed snapshot child owners; strings and excluded children still
release normally. Weak observers, throwing destructors and resurrection keep the
same lifetime rules as other graph nodes.

`__rt_gc_object_children` combines retained eval edges with native Fiber, Generator
and SPL owners, plus incomplete-object property hashes. The Fiber walk follows its callable, transfer value, pending
throwable, and any owned legacy trailing capture slots. Its visible start arguments,
caller pointers and saved frame pointers are borrowed and never contribute incoming
owner counts. Generator start arguments are independently owned boxed values;
its visitor also follows transfer, pending throwable, last key, last value and return
value slots. SplDoublyLinkedList, SplStack, SplQueue and SplFixedArray own their
internal storage array. Incomplete objects use class sentinel `-2` and own one
property hash at offset 24; their persisted name is a string, excluded from graph
edges. The visitor requires at least 32 payload bytes for that layout. Class identities and allocation bounds guard native layout
accesses. Structural work estimates inspect the same slots, and teardown routes
these owners through the common dispatcher to suppress doomed selected children.
Suspended coroutine activations publish pure typed owner visitors. Ordinary PHP
activation records retain their cleanup/frame/backtrace prefix and add a visitor
at offset 40 (48 bytes total). Compact 32-byte transferred-owner guards keep their
self-data prefix; native call-operand records already have 48 bytes. Frame walks
recognize these layouts before reading extended metadata. Saved registers and
caller pointers remain borrowed; the owned locals represented by activations are
real coroutine children.
Warning dispatch publishes the same complete 48-byte layout and visits its owned
argument and result boxes. Suspended error-handler registrations use a specialized
compact-record visitor for their saved PHP value, callable and initialized result
slot. Borrowed callback arguments and opaque eval-context handles are excluded.
Native cleanup clears each released slot before executing callbacks.

The generated visitor uses the same slot inventory, moved/borrowed exclusions,
reference-state predicates and initialized zero slots as exceptional frame cleanup.
Managed reference cells remain nodes; raw fallback cells expose their owned payload.
A private native hash set deduplicates physical owner-slot addresses across local
and call-operand relay records, while distinct slots still count as distinct owners.
Nested coroutine walks have separate contexts. Pure-cost mode allocates no set and
counts both activation records and inspected slots without following PHP values.

Native Fiber/Generator destructor entries force-close a suspended body before stack
release. A private class-sentinel unwind marker cannot match PHP catches; ordinary
finally lowering and exceptional cleanup retire the body locals. The entry boundary
absorbs only this marker. Real finally exceptions use protected destructor cleanup,
and the marker is never attached as a PHP-visible previous exception. Yield during
forced Generator close raises Error; Fiber suspension during forced Fiber close raises FiberError. Other Fiber
operations and nested engine-driven close retain PHP semantics. Doomed coroutine nodes run this stage with the collector's destructor pins
before a fresh reachability recount, so finally mutations and resurrection precede
sweep. Never-started or completed coroutines require no forced body execution.
The AST destruction-effect model accounts for these implicit callback throws even
when the class declares no PHP __destruct.
Eval catch clauses use the ordinary dynamic/native class and interface matcher,
including for `Throwable`, so they cannot consume the private unwind object.
Eval `finally` blocks still run and retain PHP control-override semantics.

`GeneratorYieldFrom` publishes an owning temporary through the ordinary EIR
call-operand ledger. Exceptional unwinding releases that owner before entering a
same-frame `finally`; borrowed local and external aliases keep their existing
lifetime. Its result is staged before argument cleanup so throwing teardown does
not strand the returned cell. `Fiber::start()` also boxes wire arguments explicitly
in EIR, under source-evaluation and call-operand owners. The native start buffer
continues to borrow those cells; callback parameter owners survive suspension,
while caller cleanup retires fresh wire boxes on return and throw.

Native callable proofs store logical capture indexes and validate current capacity,
environment binding counts, value tags and child identities. Reference bindings prove
the normalized owning cell; its changing payload is validated by the next typed edge.
Opaque eval callbacks and native object slots remain outside the proof cache and use
ordinary immediate collection when no separate rooted proof exists.

Application lifecycle enables a candidate queue instead of the physical heap scan.
Nonfinal graph decrements, including COW owner retirement, queue borrowed identities
with kind bit 31 for deduplication. The exact allocator live-base bitmap rejects
retired or interior addresses before a header is read. Reused storage participates
only if independently queued again. Discovery visits the forward closure iteratively
through the same typed child layouts used by ordinary marking.

A per-program visitor reads initialized owning globals and statics before expansion.
It anchors those roots, normalized Mixed/reference payloads, and direct object children,
without traversing array/hash contents. A non-owning spanning forest additionally records
paths through typed object, array, hash, Mixed and reference slots. Each lookup validates
exact live bases, current layout tags, bounds and child identities until it reaches a
fresh root anchor. Hash records store logical indexes and reacquire current entry storage
after rehashing. No mutation barrier is required: stale proofs fail validation and the
ordinary collector handles the affected graph. Opaque callback edges remain outside this
native-slot cache. Bootstrap builds and sorts the forest. Before path validation,
candidate discovery checks current snapshot membership to avoid duplicate work.
A parallel memo word records either a successful or an unproved path for the
current preparation epoch. An iterative second parent walk stamps every checked
prefix, including failures, so shared ancestors are not repeatedly revalidated.
No PHP callbacks run during preparation. Every prepare, including destructor
recounts, advances the epoch; wrapping clears all stamps before reuse. A negative
memo means only that this cached path does not prove liveness, never that the
node is unreachable. The ordinary collector still determines its ownership.

Every surviving unproved snapshot member adds its structural scan cost to the
saturating `_gc_proofs_debt` counter. `__rt_gc_node_scan_cost` charges one node plus
the child slots actually inspected: graph-array length, full hash capacity
(including holes and scalar/string entries), fixed and dynamic object properties,
and native retained-child slots. The native enumeration mode counts without
marking or acquiring owners. A request boundary rebuilds only after debt reaches
`max(256, _gc_proofs_cost)` of the preceding forest. Rebuilding resets the debt.
This amortizes reconstruction against candidate scan work and prevents each
small retained write from triggering a global walk and sort. Unproved values
still participate in the ordinary collector immediately; only proof-cache
maintenance waits for its budget. Newly retained large services can earn reusable
proofs through repeated closure work. A refresh can still cause an individual
request latency spike, and opaque ownership paths retain their conservative fallback.

Records use three machine words plus one parallel epoch word in reusable libc
storage, with geometric capacity. Within a preparation pass, lookup work depends
on the number of distinct validated prefixes and queries, each using logarithmic
record searches, instead of repeatedly walking shared paths. Rebuilding visits
and sorts the rooted graph. Diagnostic visited counts include structural slots,
validation, memo propagation and cache reconstruction. Native cache capacity
follows its largest recorded graph.

Anchor bit 20 is temporary and is removed before incoming-edge counting reuses that
field. References from excluded roots remain external to the candidate subset.
Destructor recounts retain previously pinned nodes even when resurrection gives them
a new global owner, so pin removal remains balanced. Only snapshot members suppress
child decrements during the final sweep; scalar/string array leaves always retire
through ordinary ownership.

Pending roots, anchors, and snapshot vectors use reusable libc buffers that own no PHP
references. Their counts are reset after use; capacities follow their largest observed
working sets. Surviving snapshot membership is cleared before a pending exception is
re-thrown. Candidate collection runs at the same safe points and request boundaries,
with the same destructor timing. Default request lifecycle retains physical heap scans.

### Performance characteristics

A loop such as `for ($i = 0; $i < 1000; $i++) { $s .= "x"; }` uses
[`__rt_str_append`](the-runtime.md#__rt_str_append-consuming-string-append) when
`$s` is an initialized plain string local and the right-hand side is a string
literal or an initialized plain string local. A borrowed parameter is allowed
on the right. The EIR admission rules for that shape, including `$s = $s . $rhs`,
are the **Consuming local string append** section of [The EIR Design](the-ir.md).
Parameters, captures, references, globals, extern or eval-visible storage, and
mixed or retyped slots stay on ordinary assignment: the previous value is
released and a new string is allocated.

For an admitted append, [`__rt_str_append`](the-runtime.md#__rt_str_append-consuming-string-append)
chooses between in-place reuse and a copy:

- A unique heap kind-1 string (refcount 1) whose right-hand side lies completely
  outside that allocation is extended in place when the new logical length fits
  in the header's payload capacity. The header size is the capacity. The
  returned pair's length word is the logical length. The helper writes only
  the new suffix and keeps the same owner.
- When that unique buffer is too small, growth is geometric: the helper doubles
  the previous capacity, then reserves at least the required length and at least
  16 bytes. A doubled size larger than `0xfffffff8` (the largest aligned payload
  the header can represent) is replaced by the exact required length. The new
  block is kind 1, the old prefix and the suffix are copied into it, and the
  previous kind-1 owner is released only after that copy finishes.
- Copy-on-write applies when the string is shared (refcount other than 1), when
  it is not kind 1 (a literal, a concat-scratch slice, or another non-owned
  view), or when the right-hand side overlaps the allocation, including a
  self-append. Those paths allocate independent storage and finish the copy
  before a kind-1 owner is retired. Shared and overlapping copies seed growth
  from the old logical length, so a retained snapshot does not keep doubling
  unused spare capacity. A non-kind-1 input has no capacity to double; its
  reservation is the required length, or 16 bytes when that is larger, and this
  helper leaves that input for its existing owner to release.
- Spare capacity is the trade-off. In the doubling case the new length still
  fits in twice the old capacity, so the live block keeps total capacity up to
  almost twice the logical length — and the existing prefix is copied only when
  the buffer grows. The copy volume of that loop stays linear in the final length.
  An append whose new length exceeds twice the old capacity reserves that exact
  length and copies the existing prefix into the new block.
  Shared and overlap copies seed that doubling from the logical length rather
  than from leftover capacity, and the same exact-length reservation applies
  when the new length exceeds twice that seed.
- An empty append returns the original owner and leaves its mbstring origin
  unchanged. A wrapping length, or a replacement the header cannot represent,
  reaches `__rt_alloc_overflow` before any payload byte is written.

The retained result is one block whose capacity follows the final length. Blocks
released by a growth copy return to the free list; the bump allocator supplies
a block when the free list cannot.

Native mbstring string identity belongs to the allocation containing the string
bytes. A kind-5 Mixed wrapper holding a string owns a separate byte buffer; the
identity belongs to that child, never to the wrapper. Its final heap release
skips the string-registry lookup; the child's release still retires its own
identity before allocator reuse. Eval array-reference observers retire before
this skip, including when a Mixed cell no longer contains its former array
value. Other allocation kinds retain their existing identity retirement path.

### Packed integer prefixes in JSON arrays

The structural JSON decoder constructs each fresh indexed array with signed-i64 slots. A bounded, allocation-free token probe accepts only strict JSON integers within PHP's integer range, followed by JSON whitespace and a comma or closing bracket. It appends the raw value with `__rt_array_push_int`, preserving the ordinary separator, depth and error cleanup paths. It records the same token-start diagnostic candidate as recursive decoding.

At the first other token, `__rt_array_to_mixed` converts the existing integer prefix once before recursive decoding continues. This covers floats, oversized integers (including `JSON_BIGINT_AS_STRING`), strings, booleans, null and containers. Later elements keep the generic path; the decoder never rereads the whole array to choose its storage. Fully integer arrays remain packed under their Mixed container box. Existing runtime-header-aware consumers handle reads, COW, references, mutation and encoding. Construction owners remain unique and unpublished until the existing owned-container boxing transfer. Partial errors release either representation through ordinary array cleanup.

Packed integer leaves remain outside the cycle collector's swept-node set. When an unreachable graph owner releases one, the uniform decref dispatcher therefore performs an ordinary array decrement even during the final sweep. A surviving native alias keeps the buffer alive; the final owner releases it. Graph-backed array storage keeps its existing reachability and sweep suppression.
