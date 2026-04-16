---
title: "The Code Generator"
description: "How AST nodes become ARM64 assembly."
sidebar:
  order: 6
---

**Source:** `src/codegen/` — `mod.rs`, `expr.rs`, `expr/`, `stmt.rs`, `stmt/`, `functions/`, `ffi.rs`, `abi/`, `context.rs`, `data_section.rs`, `emit.rs`

The code generator (codegen) is the heart of the compiler. It takes the typed AST and produces ARM64 assembly text — the actual instructions the CPU will execute. For an introduction to ARM64, see [Introduction to ARM64 Assembly](arm64-assembly.md).

## Overview

The codegen walks the AST and emits assembly for each node. The output is a single `.s` file with this structure:

```asm
.global _main
.align 2

; --- user-defined functions ---
_fn_factorial:
    ...
    ret

; --- class methods ---
_method_Point_move:
    ...
    ret

; --- main program ---
_main:
    ; prologue (stack frame setup)
    ; global argc/argv initialization
    ; program statements
    ; epilogue (exit syscall)

; --- deferred closures emitted after _main ---
_closure_1:
    ...
    ret

; --- runtime routines ---
__rt_throw_current:
    ...
__rt_itoa:
    ...
__rt_concat:
    ...

; --- data section ---
.data
_str_0: .ascii "hello"
_float_0: .quad 0x400921FB54442D18

; --- runtime data / BSS declarations ---
.comm _concat_buf, 65536, 3
.comm _heap_buf, 8388608, 3
```

Trait composition does not add a separate runtime dispatch layer. Traits are flattened into each concrete class during type checking, then inheritance metadata is layered on top. Codegen still emits `_method_Class_method` / `_static_Class_method` labels, but instance calls now use vtable slots keyed by `class_id` so child overrides work through inherited methods.

## The Emitter

**File:** `src/codegen/emit.rs`

The `Emitter` is a simple string buffer with helper methods:

| Method | Output |
|---|---|
| `instruction("mov x0, #42")` | `    mov x0, #42\n` (indented) |
| `label("_main")` | `_main:\n` |
| `comment("load variable")` | `    ; load variable\n` |
| `raw(".global _main")` | `.global _main\n` (no indent) |
| `blank()` | `\n` |

All assembly is built as text, then written to the `.s` file.

## The Context

**File:** `src/codegen/context.rs`

The `Context` tracks state during code generation:

```rust
pub struct Context {
    pub variables: HashMap<String, VarInfo>,  // variable → type + stack offset
    pub stack_offset: usize,                  // next available stack slot
    pub loop_stack: Vec<LoopLabels>,          // for break/continue
    pub return_label: Option<String>,         // for early returns
    pub functions: HashMap<String, FunctionSig>,
    pub deferred_closures: Vec<DeferredClosure>, // closures emitted after current function
    pub constants: HashMap<String, (ExprKind, PhpType)>, // compile-time constants
    pub global_vars: HashSet<String>,         // globals active in current scope
    pub static_vars: HashSet<String>,         // statics active in current scope
    pub ref_params: HashSet<String>,          // pass-by-reference params
    pub in_main: bool,                        // whether we're compiling top-level code
    pub all_global_var_names: HashSet<String>,
    pub all_static_vars: HashMap<(String, String), PhpType>,
    pub closure_sigs: HashMap<String, FunctionSig>,
    pub closure_captures: HashMap<String, Vec<(String, PhpType)>>,
    pub classes: HashMap<String, ClassInfo>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub packed_classes: HashMap<String, PackedClassInfo>,
    pub current_class: Option<String>,
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    pub extern_classes: HashMap<String, ExternClassInfo>,
    pub extern_globals: HashMap<String, PhpType>,
    pub return_type: PhpType,
    pub activation_prev_offset: Option<usize>,
    pub activation_cleanup_offset: Option<usize>,
    pub activation_frame_base_offset: Option<usize>,
    pub pending_action_offset: Option<usize>,
    pub pending_target_offset: Option<usize>,
    pub pending_return_value_offset: Option<usize>,
    pub try_slot_offsets: Vec<usize>,
    pub next_try_slot_idx: usize,
    pub finally_stack: Vec<FinallyContext>,
}
```

Each variable has a `VarInfo`:

```rust
pub struct VarInfo {
    pub ty: PhpType,                  // Int, Float, Str, etc.
    pub stack_offset: usize,          // offset from frame pointer (x29)
    pub ownership: HeapOwnership,     // NonHeap / Owned / Borrowed / MaybeOwned
    pub epilogue_cleanup_safe: bool,  // false for locals populated through still-ambiguous control-flow/alias paths
}
```

`HeapOwnership` is a codegen-only ownership lattice used for heap-backed values flowing through stack slots:

- `NonHeap` — integers, floats, bools, null, raw pointers
- `Owned` — this slot definitely owns the current heap-backed value
- `Borrowed` — this slot currently aliases heap storage owned elsewhere
- `MaybeOwned` — control flow merged heap-backed paths with different ownership states

The lattice is now threaded through the main local-variable paths. Function epilogues re-enable cleanup only for slots classified as `Owned` and still marked `epilogue_cleanup_safe`; locals coming from still-ambiguous control-flow or aliasing paths are intentionally skipped. Special aliases such as `$this`, by-reference params, globals, and statics are explicitly kept out of epilogue cleanup because the current frame does not own their storage. Builtins that duplicate containers now also dispatch to dedicated `_refcounted` runtime helpers when their element/value types are heap-backed, so nested array/hash/object/string payloads are retained before the new container becomes an owner.

The exception-related fields let codegen thread `try` / `catch` / `finally` through non-local control flow. Function and `_main` frames publish activation records into the runtime cleanup stack, pre-allocate handler slots for `setjmp` buffers, and use `finally_stack` plus the `pending_*` slots to defer `return`, `break`, and `continue` until the innermost `finally` body has run.

### Label generation

`ctx.next_label("while")` produces `_while_1`, `_while_2`, etc. A global atomic counter ensures labels never collide across functions or compilation units.

## The Data Section

**File:** `src/codegen/data_section.rs`

String literals and float constants are stored in the `.data` section:

```rust
pub struct DataSection {
    entries: Vec<(String, Vec<u8>)>,          // string label → bytes
    float_entries: Vec<(String, u64)>,        // float label → bit pattern
    counter: usize,                           // next unique label suffix
    dedup: HashMap<Vec<u8>, String>,          // avoid duplicate strings
    float_dedup: HashMap<u64, String>,        // avoid duplicate floats
}
```

When the codegen encounters `"hello"`, it calls `data.add_string(b"hello")` which returns a label (`_str_0`) and length (`5`). Identical strings are deduplicated — two `"hello"` literals share the same label.

Floats are stored as their raw 64-bit IEEE 754 bit patterns (`.quad` directive).

## Expression codegen

**Files:** `src/codegen/expr.rs`, `src/codegen/expr/`

`emit_expr()` takes an expression node and emits code that leaves the result in the standard registers. The top-level `expr.rs` file now mainly dispatches into focused helpers under `expr/` such as `scalars.rs`, `variables.rs`, `binops.rs`, `arrays.rs`, `compare.rs`, `calls/`, and `objects/`.

| Type | Result location |
|---|---|
| `Int` / `Bool` / `Void` | `x0` |
| `Float` | `d0` |
| `Str` | `x1` (pointer), `x2` (length) |
| `Array` / `AssocArray` | `x0` (heap pointer) |
| `Mixed` | `x0` (pointer to boxed mixed cell) |
| `Object` | `x0` (heap pointer) |
| `Callable` / `Pointer` | `x0` |
| `Buffer` / `Packed` | `x0` (heap pointer) |
| `Union` | `x0` (same as Mixed — boxed runtime-tagged payload) |

### Literals

```php
42        →  mov x0, #42
3.14      →  adrp x9, _float_0@PAGE  /  add x9, ...  /  ldr d0, [x9]
"hello"   →  adrp x1, _str_0@PAGE  /  add x1, ...  /  mov x2, #5
true      →  mov x0, #1
null      →  movz x0, #0xFFFE  /  movk x0, ...  (load null sentinel)
```

Large integers (> 65535 or negative) use `movz` + `movk` sequences. See [ARM64 Instruction Reference](arm64-instructions.md#loading-large-constants).

### The push/pop pattern for binary operations

Binary operations like `$a + $b` need both operands in registers simultaneously, but `emit_expr` uses the same registers for every expression. The solution: **push the left result onto the stack, evaluate the right, then pop the left back**.

```php
$a + $b
```

```asm
; Step 1: evaluate left ($a)
ldur x0, [x29, #-8]              ; x0 = $a

; Step 2: push left onto stack
str x0, [sp, #-16]!              ; save x0 to stack, decrement sp

; Step 3: evaluate right ($b)
ldur x0, [x29, #-16]             ; x0 = $b  (overwrites left!)

; Step 4: pop left back into a different register
ldr x1, [sp], #16                ; restore left into x1, increment sp

; Step 5: operate
add x0, x1, x0                   ; x0 = left + right
```

For strings (which use two registers), the push saves both `x1` and `x2`, and the pop restores them to `x3` and `x4`.

For floats, the push/pop uses `d0`/`d1`:

```asm
str d0, [sp, #-16]!              ; push left float
; ... evaluate right → d0 ...
ldr d1, [sp], #16                ; pop left float into d1
fadd d0, d1, d0                  ; d0 = left + right
```

### Comparison operators

Comparisons use `cmp` (integer) or `fcmp` (float) followed by `cset`:

```php
$x > 5
```

```asm
; ... push $x, evaluate 5 ...
cmp x1, x0                       ; compare left with right
cset x0, gt                      ; x0 = 1 if greater, 0 otherwise
```

The result is always `x0` with value 0 or 1 (`PhpType::Bool`).

### Short-circuit logical operators

`&&` and `||` use **short-circuit evaluation** — the right side isn't evaluated if the left determines the result:

```php
$a && $b
```

```asm
; evaluate $a
cmp x0, #0
b.eq _sc_end_1          ; if $a is falsy, skip $b entirely (result = 0)
; evaluate $b
cmp x0, #0
cset x0, ne             ; result = whether $b is truthy
_sc_end_1:
```

### String concatenation

The `.` operator calls the runtime's `__rt_concat`:

```php
"hello" . " world"
```

```asm
; push left string (x1, x2)
; evaluate right string → x1, x2
; pop left → x3, x4
; call concat
mov x3, ...              ; left ptr
mov x4, ...              ; left len
bl __rt_concat           ; result → x1 (ptr), x2 (len)
```

See [The Runtime](the-runtime.md) for how `__rt_concat` works.

### Bitwise operations

The bitwise operators (`&`, `|`, `^`, `~`, `<<`, `>>`) operate on integers and emit single ARM64 instructions:

```php
$a & $b    →  and x0, x1, x0     // bitwise AND
$a | $b    →  orr x0, x1, x0     // bitwise OR
$a ^ $b    →  eor x0, x1, x0     // bitwise XOR
$a << $b   →  lsl x0, x1, x0     // logical shift left
$a >> $b   →  asr x0, x1, x0     // arithmetic shift right (preserves sign)
~$a        →  mvn x0, x0         // bitwise complement (one's complement)
```

Like other binary operations, bitwise ops use the push/pop pattern — evaluate left, push, evaluate right, pop left, apply operation.

### Spaceship operator

The spaceship operator (`<=>`) returns -1, 0, or 1 depending on the comparison result. It uses conditional select instructions:

```php
$a <=> $b
```

```asm
; ... push $a, evaluate $b ...
cmp x1, x0                      ; compare left with right
cset x0, gt                     ; x0 = 1 if left > right, else 0
csinv x0, x0, xzr, ge           ; if left < right: x0 = ~0 = -1 (all ones)
```

`csinv` (conditional select invert) inverts `xzr` (the zero register) to produce -1 when the condition is not met.

For floats, `fcmp` replaces `cmp`, but the same `cset`/`csinv` pattern applies.

### Null coalescing operator

The `??` operator returns the left operand if it is non-null, otherwise the right:

```php
$x ?? "default"
```

```asm
; evaluate $x
; compare with null sentinel (0x7FFFFFFFFFFFFFFE)
b.ne _nc_done_1          ; if not null, keep left value
; evaluate "default"      ; otherwise, use right side
_nc_done_1:
```

The null check compares the value against the [null sentinel](memory-model.md). The operator is right-associative (`$a ?? $b ?? $c` = `$a ?? ($b ?? $c)`).

### Type coercions

When types need to match (e.g., int + float), the codegen inserts conversion instructions:

```asm
scvtf d0, x0             ; convert signed integer (x0) → double (d0)
fcvtzs x0, d0            ; convert double (d0) → signed integer (x0)
```

The `.` (concat) operator also coerces non-strings:
- `Int` → calls `__rt_itoa` to get a string
- `Float` → calls `__rt_ftoa`
- `Bool true` → string "1"
- `Bool false` / `Null` → empty string (length 0)

### Constant references

```php
const MAX = 100;
echo MAX;
```

Constants declared with `const` or `define()` are resolved at compile time. When the codegen encounters a `ConstRef`, it looks up the constant's value and emits it as a literal — `mov x0, #100` for an integer, or loads a string label from the data section. No runtime lookup is needed.

Enum cases reuse the same idea, but through enum metadata instead of scalar constants: `ExprKind::EnumCase` resolves to a canonical enum-case symbol emitted in runtime data, and helper builtins such as `Enum::from()` / `Enum::tryFrom()` lower through the checker/codegen enum tables carried in `Context`.

### Pointer values and casts

Pointer expressions are carried in `x0` as plain 64-bit addresses:

- `ptr($var)` computes the address of a stack or global slot and returns it in `x0`
- `ptr_null()` loads the zero address
- `ptr_cast<T>($p)` only changes the static type tag seen by the checker, so codegen emits the inner expression and leaves the address unchanged
- Pointer printing routes through `__rt_ptoa`, which formats the address as a `0x...` string before writing

### Buffer allocation and packed hot-path access

`buffer_new<T>(len)` lowers directly from `ExprKind::BufferNew`: codegen evaluates the element count, loads the checked element stride from the type metadata, and calls `__rt_buffer_new`. The resulting pointer in `x0` references a contiguous `[length][stride][payload...]` block rather than a PHP array/hash structure.

When `T` is a scalar POD type, reads and writes use direct address arithmetic from the buffer base plus `index * stride`. When `T` is a `packed class`, codegen combines the buffer element stride with the field offset from `packed_classes` metadata and emits direct typed loads/stores into the packed payload.

### Function calls

```php
my_func($a, $b, $c)
```

1. Evaluate each argument and push results onto the stack
2. Pop arguments into the correct ABI registers (`x0`-`x7` for ints, `d0`-`d7` for floats, two registers per string)
3. If a heap-backed argument is being borrowed from an existing owner (for example a local variable or container read), retain it before passing it to the callee
4. `bl _fn_my_func` — branch with link (saves return address)
5. Result is in `x0`/`d0`/`x1`+`x2` depending on return type

## Closure codegen

### Anonymous functions and arrow functions

Closures (`function($x) { ... }`) and arrow functions (`fn($x) => ...`) are compiled as separate labeled functions, similar to user-defined functions. The key difference is **deferred emission** — the closure body is not emitted inline. Instead:

1. **At the closure expression site**: the codegen generates a unique label (e.g., `_closure_1`) and loads its address into `x0` using `adrp` + `add`. The address is then stored in the variable's stack slot as a `Callable` (8-byte function pointer).

2. **The body is deferred**: the closure's parameter list, body statements, captured variables, and label are pushed onto `ctx.deferred_closures`. This avoids emitting function code in the middle of the current function's instruction stream.

3. **After `_main`**: all deferred closures are emitted as standalone labeled functions (prologue, body, epilogue), just like user-defined functions.

### `use` captures

Closures can capture variables from the enclosing scope via `use ($var1, $var2)`:

```php
$greeting = "Hello";
$fn = function($name) use ($greeting) {
    echo $greeting . " " . $name;
};
```

Only explicit `use (...)` captures are stored in the AST and forwarded as hidden closure arguments. Arrow functions are still parsed as closures, but they use `is_arrow = true` with an empty `captures` list.

The AST stores captured variable names in the `captures` field of the `Closure` expression. At the call site, captured variables are passed as **extra arguments** after the explicit arguments:

1. **At the closure expression site**: the captured variable names and types are recorded in `ctx.closure_captures` alongside the deferred closure.
2. **At the call site** (`$fn("World")`): the codegen looks up the captured variables, evaluates them from the caller's scope, and passes them as additional arguments after the explicit ones.
3. **In the closure body**: the captured values arrive as extra parameters and are stored in local stack slots, making them accessible like regular local variables.

This means captures are passed **by value** — modifying a captured variable inside the closure does not affect the outer scope (matching PHP semantics).

### Closure calls

When a closure variable is called (`$fn(1, 2)`), the codegen:

1. Evaluates each argument and pushes results onto the stack
2. Loads the closure function address from the variable's stack slot into `x9`
3. Pushes `x9` temporarily while popping arguments into ABI registers
4. Pops `x9` back and calls `blr x9` — an indirect branch through a register

`blr` (Branch with Link to Register) is like `bl` but the target address comes from a register rather than a label. This is what makes closures work — the compiler doesn't know at compile time which function will be called, so it uses an indirect jump.

### Closures as callback arguments

Built-in functions like `array_map`, `array_filter`, `array_reduce`, `array_walk`, and `usort` accept callback values. The callback function pointer is passed in a register (like any other `Callable` argument) and the runtime routine calls it via `blr`.

Closures that depend on hidden `use (...)` capture arguments still only work for direct `$fn(...)` calls today. Callback-style built-ins do not forward those hidden capture values, so captured closures are not yet valid drop-in callbacks there.

## Associative array codegen

Associative arrays use a hash table stored on the heap. The codegen differs from indexed arrays at every level:

### Literal creation

```php
$m = ["name" => "Alice", "age" => "30"];
```

1. Call `__rt_hash_new` with initial capacity and value type tag → `x0` = hash table pointer
2. For each key-value pair: evaluate key (string → `x1`/`x2`), evaluate value, call `__rt_hash_set`

### Access

```php
$m["name"]
```

1. Save hash table pointer on stack
2. Evaluate key expression → `x1`/`x2` (string)
3. Call `__rt_hash_get` → `x0` = found (0/1), `x1` = value_lo, `x2` = value_hi, `x3` = per-entry value tag
4. Move result to standard registers based on value type; if the static result is `Mixed`, box the payload into a heap cell first

### Functions on associative arrays

Builtin functions like `array_key_exists`, `in_array`, `array_keys`, `array_values` dispatch on the array type at compile time:
- `PhpType::Array` → use indexed runtime routines (e.g., bounds check, linear scan)
- `PhpType::AssocArray` → use hash table routines (e.g., `__rt_hash_get`, `__rt_hash_iter_next`)

### `foreach` over associative arrays

When `foreach` iterates a `PhpType::AssocArray`, the lowering differs from indexed arrays:

1. Save the hash pointer and an iteration cursor on the stack (`0` means "start from header.head")
2. Call `__rt_hash_iter_next`
3. If `x0 == -1`, exit the loop
4. Otherwise save the returned cursor, store `x1`/`x2` into the optional key variable, and store `x3`/`x4`/`x5` into the value variable according to the inferred element type; `Mixed` loop variables reuse or allocate boxed mixed cells as needed
5. Emit the loop body, then branch back to the iterator call

This preserves PHP-style insertion order because `__rt_hash_iter_next` walks the hash table's linked insertion-order chain rather than scanning physical buckets.

See [The Runtime](the-runtime.md) for details on hash table routines and [Memory Model](memory-model.md) for the hash table memory layout.

## String indexing codegen

The same `ArrayAccess` AST node also covers string indexing such as `$str[1]` or `$str[-1]`. In `src/codegen/expr/arrays.rs`, `emit_array_access()` checks for `PhpType::Str` and lowers the operation inline:

1. Save the string pointer/length while evaluating the index expression
2. Adjust negative indices relative to the end of the string
3. Clamp offsets below `-len` to the start and offsets past the end to the end
4. Advance the string pointer to the selected byte
5. Return either a one-character string (`x1` + `x2 = 1`) or an empty string when the offset is out of bounds

So the behavior is slice-like, but it does not call `substr()` or a dedicated runtime helper.

## Statement codegen

**Files:** `src/codegen/stmt.rs`, `src/codegen/stmt/`

`emit_stmt()` is similarly split across focused helpers under `stmt/`: assignment/storage logic, array statements, and control-flow lowering (`branching`, `foreach`, `loops`) now live outside the thin top-level dispatcher. Small shared statement-side policies such as borrowed-result retention, local-slot ownership updates, static-init guards, and indexed-array metadata stamping now sit in `stmt/helpers.rs` instead of bloating `stmt.rs` itself. Storage lowering is now split too: `stmt/storage.rs` is just a boundary, with `storage/locals.rs` handling ordinary global/static symbol access and `storage/extern_globals.rs` owning extern-global load/store conventions. Assignment lowering is also split one level deeper: `stmt/assignments/locals.rs` handles plain local/global/ref writes, while `stmt/assignments/properties.rs` now orchestrates property writes across `properties/target.rs`, `magic_set.rs`, and `storage.rs`. Array-index writes follow the same pattern now: `stmt/arrays/assign.rs` is just a dispatcher, while `stmt/arrays/assign/buffer.rs` and `assoc.rs` isolate the non-indexed-container paths, and `stmt/arrays/assign/indexed.rs` now orchestrates the indexed-array write across `indexed/prepare.rs`, `normalize.rs`, `store.rs`, and `extend.rs`. Branching lowering now follows that same shape too: `stmt/control_flow/branching.rs` is just a boundary, while `branching/if_stmt.rs` and `branching/switch_stmt.rs` own the distinct lowering paths. Exception lowering follows the same structure: `stmt/control_flow/exceptions.rs` orchestrates the high-level try/catch/finally flow, while `exceptions/handlers.rs`, `catches.rs`, and `finally.rs` own the lower-level handler stack, catch matching, and pending-action/finally dispatch mechanics. Loop lowering is split too: `stmt/control_flow/loops.rs` is now just a boundary, with `loops/iterative.rs` handling `for`/`while`/`do...while` and `loops/exits.rs` owning `break`/`continue`/`return`. `foreach` lowering now follows the same pattern: `stmt/control_flow/foreach.rs` dispatches between `stmt/control_flow/foreach/indexed.rs` and `stmt/control_flow/foreach/assoc.rs`.

### Echo

```php
echo $x;
```

1. Evaluate expression → result in registers
2. Check for null/false (skip printing if so — matches PHP behavior where `echo false` prints nothing)
3. Call `emit_write_stdout()` from the [ABI module](#the-abi-module)

### Assignment

```php
$x = expr;
```

1. Evaluate expression
2. If the result is a borrowed heap value, retain it before the local slot becomes a new owner
3. Release the previous owned heap value from `$x` when overwriting a heap-backed slot
4. `emit_store()` — write result to `$x`'s stack slot and classify the local slot as `Owned` for heap-backed types

Typed local declarations such as `int $x = 42;` or `buffer<int> $xs = buffer_new<int>(8);` share the same storage path after the checker has resolved `StmtKind::TypedAssign` into a concrete `PhpType`.

### Constant declaration

```php
const MAX = 100;
```

`ConstDecl` registers a compile-time constant. The value is stored in the codegen context and substituted directly wherever the constant is referenced via `ConstRef`. No runtime storage or stack allocation is needed.

### Global variables

```php
$x = 10;
function inc() {
    global $x;
    $x++;
}
```

The `global` statement inside a function declares that a variable refers to global storage rather than a local stack slot. The codegen uses BSS-allocated storage (`_gvar_NAME`, 16 bytes each) for global variables:

1. At `global $x;`: the variable is marked as global in the context. The current value is loaded from `_gvar_x` into the local stack slot.
   The local view is tracked as a borrowed alias of the BSS-backed owner.
2. On assignment to a global variable: the codegen writes to the BSS storage (`_gvar_x`) via `adrp`/`add`/`str` instead of (or in addition to) the local stack slot.
3. In `_main`: when the main scope assigns to a variable that any function declares as `global`, the value is also written to `_gvar_NAME` so that functions can read it.

### Extern declarations

`ExternFunctionDecl`, `ExternClassDecl`, and `ExternGlobalDecl` are registration-only statements during codegen. Their metadata has already been collected by the type checker and copied into `Context`, so `emit_stmt()` treats the declarations themselves as no-ops while later expression codegen uses the recorded FFI data.

Extern globals are loaded through GOT-relative addressing (`adrp ...@GOTPAGE` / `ldr ...@GOTPAGEOFF`) instead of ordinary stack or BSS slots.

### Static variables

```php
function counter() {
    static $count = 0;
    $count++;
    echo $count;
}
```

Static variables persist their value across function calls. Each static variable gets two BSS slots:

- `_static_FUNC_VAR` (16 bytes) — stores the persisted value
- `_static_FUNC_VAR_init` (8 bytes) — initialization flag (0 = not yet initialized)

The codegen for `static $count = 0;`:

1. Check the init flag — if already initialized, skip to loading the persisted value
2. If not initialized: evaluate the init expression, store to the BSS slot, set the init flag to 1
3. Load the persisted value into the local stack slot

That per-call local slot is tracked as `Borrowed`; the persisted static storage remains the long-lived owner.

At function epilogue, variables marked as static are written back to their BSS storage.

### List unpacking

```php
[$a, $b, $c] = [10, 20, 30];
```

`ListUnpack` destructures an indexed array into individual variables. The codegen:

1. Evaluates the right-hand side expression (an array)
2. Saves the array pointer on the stack
3. For each variable in the list: loads the element at the corresponding index from the array, stores it into the variable's stack slot, and marks heap-backed elements as borrowed aliases of the source container

### If / Elseif / Else

```php
if ($cond1) { body1 } elseif ($cond2) { body2 } else { body3 }
```

```asm
; evaluate $cond1
cmp x0, #0
b.eq _elseif_1           ; skip to next branch if falsy

; body1
b _end_if_1               ; done — skip all remaining branches

_elseif_1:
; evaluate $cond2
cmp x0, #0
b.eq _else_1

; body2
b _end_if_1

_else_1:
; body3

_end_if_1:
```

### While loop

```php
while ($cond) { body }
```

```asm
_while_1:                  ; ← continue jumps here
; evaluate $cond
cmp x0, #0
b.eq _end_while_1         ; exit if falsy ← break jumps here

; body
b _while_1                 ; loop back

_end_while_1:
```

### For loop

```php
for ($i = 0; $i < 10; $i++) { body }
```

```asm
; emit init ($i = 0)

_for_1:
; evaluate condition ($i < 10)
cmp x0, #0
b.eq _end_for_1

; body

_for_cont_1:               ; ← continue jumps here
; emit update ($i++)
b _for_1

_end_for_1:                 ; ← break jumps here
```

### Foreach

```php
foreach ($arr as $v) { body }
```

For indexed arrays:

1. Save array pointer, length, and index counter on the stack (3 × 16-byte slots)
2. Loop: load element at current index, store to `$v`, and classify heap-backed loop variables as borrowed aliases of the iterated container
3. Branch back to condition check
4. Cleanup: deallocate the 48 bytes

For associative arrays, see [Associative array codegen](#associative-array-codegen): the loop stores a hash pointer plus cursor, then advances with `__rt_hash_iter_next`.

### Break / Continue

`break` emits a `b` (unconditional jump) to the current loop's end label.
`continue` emits a `b` to the loop's continue label (the condition check for `while`, the update for `for`).

The `loop_stack` in the Context tracks which labels to jump to for nested loops. Each `LoopLabels` entry also carries an `sp_adjust` field so returns inside switch/loop-driven control flow can undo any temporary stack slots before jumping to the shared function epilogue.

### Exceptions and `finally`

Exception lowering lives in `src/codegen/stmt/control_flow/exceptions.rs`. The basic strategy is:

1. Evaluate the thrown object and publish it to `_exc_value`
2. Call `__rt_throw_current`, which unwinds activation records and `longjmp`s into the nearest handler
3. For `try`, emit a `_setjmp` resume point plus a linked handler record in `_exc_handler_top`
4. Test each catch target by class id or interface id through `__rt_exception_matches`
5. Route `return`, `break`, `continue`, and rethrow through `finally_stack` so every enclosing `finally` runs before control leaves the protected region

This means `finally` is part of ordinary control-flow lowering, not a separate runtime pass. The runtime only unwinds frames and chooses the landing pad; the compiler-generated labels still decide whether execution resumes in a matching `catch`, in a `finally`, or in an outer handler.

### Switch

```php
switch ($x) {
    case 1: echo "one"; break;
    case 2: echo "two"; break;
    default: echo "other"; break;
}
```

1. Evaluate the subject expression once and push the result onto the stack
2. For each case: pop subject, evaluate case value, compare (`cmp` + `b.ne` for integers, `bl __rt_str_eq` for strings)
3. If match: emit case body, which may contain `break` (jump to end label) or fall through to next case
4. Default case: emit body unconditionally
5. End label after all cases

The switch uses the loop stack so that `break` inside a case body jumps to the switch end label rather than an enclosing loop.

### Match expression

Match is an expression (returns a value), not a statement. It uses strict comparison (`===`) and has no fall-through:

```php
$result = match($x) {
    1 => "one",
    2 => "two",
    default => "other",
};
```

1. Evaluate subject, push onto stack
2. For each arm: compare subject with each pattern in the arm's pattern list
3. If any pattern matches: evaluate the arm's result expression, jump to end
4. Default arm: evaluate result unconditionally
5. Result is left in standard registers (`x0`, `d0`, or `x1`/`x2`)

## Class codegen

### Object allocation (`new ClassName(...)`)

When the codegen encounters a `NewObject` expression:

1. **Calculate object size**: `8 + (num_properties × 16)` — 8 bytes for the class ID, 16 bytes per property across the full inherited layout
2. **Allocate heap memory**: call `__rt_heap_alloc` with the calculated size
3. **Zero-initialize**: clear all property slots to zero
4. **Store class ID**: write the class identifier at offset 0
5. **Apply defaults**: for properties with default values, evaluate and store them at their fixed offsets
6. **Call constructor**: if the class exposes `__construct`, pass the new object pointer as `x0` (`$this`) followed by the constructor arguments, then branch to the implementation label recorded in class metadata (which may come from an inherited constructor)

The result is the object pointer in `x0`.

### Property access (`$obj->prop`)

Property access usually uses fixed offsets computed at compile time from `ClassInfo.property_offsets`:

```asm
; $obj->prop where prop resolved to offset 24
ldur x0, [x29, #-offset]            ; load object pointer
ldur x0, [x0, #24]                  ; load property at resolved inherited offset
```

If the property does not exist but the class exposes `__get($name)`, codegen materializes the property name as a string literal, pushes it as an argument, and dispatches the instance method through the normal object-call path. The returned value then flows back through the ordinary result registers based on the inferred return type.

For property assignment (`$obj->prop = value`), the value is evaluated first, then stored at the resolved inherited offset. If the property is missing but the class exposes `__set($name, $value)`, codegen boxes the value as `Mixed`, materializes the property name, and dispatches `__set` instead of emitting a direct store.

### Method call (`$obj->method(args)`)

1. Evaluate the object expression to get the pointer in `x0`
2. Push the object pointer onto the stack
3. Evaluate and push all arguments
4. Pop arguments into ABI registers, with the object pointer as the first argument (`x0`)
5. Load the object's `class_id`, fetch the class vtable pointer from `_class_vtable_ptrs`, load the method slot, and `blr` to the resolved implementation
6. Result is in the standard registers based on return type

Inside the method body, `$this` is the first parameter and lives in the function's first stack slot.

Private instance methods are the exception: they do not get vtable slots, so calls resolved to a private method of the current lexical class use a direct `_method_Class_method` branch instead of virtual dispatch.

### Static method call (`ClassName::method(args)`)

Static methods are called like regular functions, but with the label `_static_ClassName_methodName`. No object pointer is passed:

```asm
bl _static_Point_origin              ; call static method
; result in x0 (object pointer)
```

`self::method()` is handled as a direct call against the current lexical class. If it resolves to an instance method, codegen loads the implicit `$this` receiver and branches directly to the resolved `_method_Class_method` label. `parent::method()` works the same way against the immediate parent class. For static targets, codegen now also threads a hidden "called class id" argument through static method bodies: named `ClassName::method()` calls pin that id to the named class, while `self::` and `parent::` forward the current called class. `static::method()` then uses that forwarded class id to load the target from a per-class static-method table at runtime.

## The ABI module

**Files:** `src/codegen/abi/mod.rs`, `src/codegen/abi/`

Centralizes register conventions so they're consistent everywhere:

### Large offset addressing

ARM64's `stur`/`ldur` instructions only support 9-bit signed immediates (offsets up to 255). Functions with many local variables can exceed this limit. The ABI module handles this transparently via `store_at_offset()` and `load_at_offset()`:

- **Offsets <= 255**: single `stur`/`ldur` instruction (fast path)
- **Offsets 256-4095**: two-instruction sequence — `sub x9, x29, #offset` to compute the address in a scratch register, then `str`/`ldr` through that register

This means all codegen that accesses stack variables goes through the ABI helpers rather than emitting `stur`/`ldur` directly, so large stack frames work automatically. The same boundary now also owns indirect `[*ptr]` loads/stores used by by-reference params and mutation-heavy expression paths, so x86_64-specific memory syntax does not leak back into `expr.rs`.

`emit_frame_slot_address()` complements those helpers when codegen needs the address of a local slot itself rather than the value stored there. By-reference calls, `ptr($var)`, and exception-frame bookkeeping now all reuse that helper instead of open-coding frame-slot address math.

### Frame and return-value helpers

The `abi/` module now centralizes the frame-management primitives used by both `_main` and ordinary functions:

- `emit_frame_prologue()` / `emit_frame_restore()` — shared stack-frame setup and teardown
- `emit_cleanup_callback_prologue()` / `emit_cleanup_callback_epilogue()` — tiny helper frames used by exception cleanup callbacks
- `emit_preserve_return_value()` / `emit_restore_return_value()` — spill/reload of scalar, float, and string returns across epilogue side effects or `finally` dispatch

That moves prologue/epilogue mechanics out of the higher-level walkers and makes the ABI layer responsible for more than just local-slot addressing.

### Incoming argument lowering

Incoming parameter decoding now goes through `IncomingArgCursor` plus `emit_store_incoming_param()`.

The cursor tracks:

- current integer argument register index
- current floating-point argument register index
- when argument passing has overflowed to the caller stack
- the caller-stack byte offset for subsequent spilled parameters

Those helpers now understand both the existing AArch64 calling convention and the in-progress `linux-x86_64` SysV AMD64 target. Function codegen delegates incoming-parameter lowering to the ABI layer instead of open-coding register names or caller-stack offsets inline.

### Outgoing call argument lowering

Outgoing calls now use ABI-owned helpers as well:

- `build_outgoing_arg_assignments_for_target()` decides whether each argument lands in an integer register, a floating-point register, or overflows onto the caller-visible stack area for the selected target
- `materialize_outgoing_args()` rewrites the temporary pushed-argument stack into the final ABI layout expected at the call site

That logic is shared by ordinary function calls, indirect/callable dispatch, object/method calls, constructor/static dispatch, and helpers such as `call_user_func_array()`. The assignment/materialization rules now cover both AArch64 and the in-progress `linux-x86_64` SysV layout, so the call ABI policy lives in one place instead of being duplicated across several dispatch paths.

The same module now also owns a thin layer of call-site and temporary-stack primitives used by higher-level walkers:

- `emit_call_label()` / `emit_call_reg()` emit direct and indirect calls for the current target
- `emit_push_reg()`, `emit_pop_reg()`, `emit_push_float_reg()`, `emit_pop_float_reg()`, `emit_push_reg_pair()`, `emit_pop_reg_pair()`, and `emit_push_result_value()` manage the temporary argument stack without hardcoding ARM64 push/pop forms in each call path
- `emit_reserve_temporary_stack()`, `emit_temporary_stack_address()`, and `emit_load_temporary_stack_slot()` now also back the FFI extern-call path, where borrowed C-string temporaries are tracked and released after the foreign call returns
- `emit_release_temporary_stack()` and `emit_store_zero_to_local_slot()` centralize target-specific stack cleanup and zero-initialization details
- `emit_store_process_args_to_globals()`, `emit_enable_heap_debug_flag()`, `emit_copy_frame_pointer()`, and `emit_exit()` cover the `_main` bootstrap/teardown path without hardcoding process-entry registers or exit sequences in the higher-level driver

That keeps phase-3 `linux-x86_64` work focused inside `abi/` instead of scattering `call`, `blr`, `add sp`, `rsp`, or zero-register assumptions across function, closure, callable, and method dispatch code.

The same `abi/` layer now also owns symbol-slot plumbing for compiler-managed globals such as `_gvar_*`, `_static_*`, `_exc_*`, `_global_*`, and the high-frequency runtime symbols used by string builders, heap bookkeeping, and GC state such as `_concat_off`, `_heap_*`, and `_gc_*`: computing symbol addresses, moving result registers into symbol storage, loading symbol storage back into result registers, and copying local frame slots into symbol-backed storage during epilogues. Extern globals now use the same boundary too, so GOT/GOTPCREL address materialization lives in `abi/` instead of being open-coded separately in expression and statement lowering.

### `emit_store(emitter, type, offset)`

Stores the current result to a stack variable. Uses `store_at_offset()` internally to handle large offsets:

| Type | What it stores |
|---|---|
| `Int` / `Bool` | `stur x0, [x29, #-offset]` (or 2-insn sequence for large offsets) |
| `Float` | `stur d0, [x29, #-offset]` |
| `Str` | `bl __rt_str_persist`, then `stur x1, [x29, #-offset]` + `stur x2, [x29, #-(offset-8)]` |
| `Array` / `AssocArray` | `stur x0, [x29, #-offset]` |
| `Mixed` | `stur x0, [x29, #-offset]` |
| `Object` | `stur x0, [x29, #-offset]` |
| `Callable` / `Pointer` | `stur x0, [x29, #-offset]` |
| `Buffer` / `Packed` / `Union` | `stur x0, [x29, #-offset]` |

### `emit_load(emitter, type, offset)`

Loads a stack variable into result registers (inverse of store). Uses `load_at_offset()` internally.

### `emit_write_stdout(emitter, type)`

Emits code to print a value to stdout:

| Type | How it prints |
|---|---|
| `Str` | `mov x0, #1` / `mov x16, #4` / `svc #0x80` (direct syscall) |
| `Int` | `bl __rt_itoa` → then write |
| `Float` | `bl __rt_ftoa` → then write |
| `Bool` | `true` prints "1", `false` prints nothing |
| `Pointer` | `bl __rt_ptoa` → then write |
| `Mixed` | `bl __rt_mixed_write_stdout` → inspect boxed runtime tag, then write |
| `Void`/`Array`/`AssocArray`/`Callable`/`Object` | Prints nothing |

For the in-progress `linux-x86_64` backend, this write path is now the first real end-to-end vertical slice: string results are written with Linux SysV syscall registers (`rsi`/`rdx`/`edi`/`eax`), integer echo goes through the x86_64 `__rt_itoa` routine, float echo goes through the x86_64 `__rt_ftoa` routine backed by `snprintf`, and `_main` only initializes `$argc` / `$argv` when the program actually references them. The current minimal x86_64 runtime also emits `__rt_build_argv`, which materializes a tiny argv array header with `malloc` plus borrowed `ptr+len` entries so `count($argv)` and `$argv[0]` work before the full heap/array runtime is ported. String ownership is no longer modeled as a concat-buffer-only shortcut on x86_64: `emit_store()` now routes string locals through a real x86_64 `__rt_str_persist`, and the bootstrap runtime emits guarded `__rt_heap_alloc`, `__rt_heap_free`, and `__rt_heap_free_safe` wrappers that stamp a minimal uniform header in front of libc-backed allocations. That is enough to make string locals survive statement boundaries, `unset()` release owned strings, callable/mixed boxing retain persisted string payloads, enum singleton bootstrap stamp valid x86_64 heap headers, and function epilogues re-enable owned-string / owned-mixed cleanup on x86_64 without depending on the full ARM64 bump allocator. Arithmetic and comparison lowering now has an x86_64 slice for integer add/sub/mul/mod, `intdiv()`, string-to-int conversion through `intval()` / `__rt_atoi`, compile-time scalar predicates `is_bool()` / `is_int()` / `is_float()`, floating-point predicates `is_nan()` / `is_infinite()` / `is_finite()`, bitwise integer ops, arithmetic shifts, the `<=>` spaceship operator, PHP float division, float arithmetic/modulo, float comparisons, `floatval()`, `boolval()`, `is_numeric()` over integer/float/string inputs, `empty()`, `gettype()` over concrete and boxed mixed/union payloads, shared cast-to-bool truthiness coercion, float truthiness in `if`, SysV overflow float args, the mixed/union-backed `===` / `!==` helper path, null-coalescing over boxed mixed/union values, the current string-to-double cast path through `__rt_cstr` + `atof`, and the float builtins `floor()`, `ceil()`, `round()`, `fdiv()`, and `fmod()`. Control-flow lowering also has a basic x86_64 slice for `if`, ternary expressions, `while`, `for`, `switch`, `match`, `break`, `continue`, function-return jumps, indexed `foreach`, assoc-array `foreach`, and the full current exception surface (`try`, typed `catch`, multi-catch, `finally`, rethrow, uncaught-fatal reporting, and builtin `Exception` / `Throwable` catch dispatch). Direct function/callable lowering covers `function_exists()`, `call_user_func()`, `call_user_func_array()`, first-class callable wrappers, callable aliases that preserve by-reference params, callable-driven `array_map()`, `array_map()` with string-returning callbacks, `array_filter()`, `array_reduce()`, `array_walk()`, array spread in literals and variadics, `range()`, `array_fill()`, `array_fill_keys()`, `array_pop()`, `array_shift()`, `array_unshift()`, `array_slice()`, `array_splice()`, `array_chunk()`, `array_combine()`, `array_flip()`, `array_merge()`, `array_pad()`, `array_product()`, `array_rand()`, `array_reverse()`, `array_sum()`, `array_unique()`, `array_column()`, string and integer `implode()`, `explode()`, `trim()` whitespace mode and mask mode, `ltrim()` whitespace and mask modes, `rtrim()` whitespace and mask modes, `strtoupper()`, `ucfirst()`, `lcfirst()`, `ucwords()`, `str_repeat()`, `chr()`, `nl2br()`, `htmlspecialchars()`, `htmlentities()`, `html_entity_decode()`, `urlencode()`, `urldecode()`, `rawurlencode()`, `rawurldecode()`, `md5()`, `sha1()`, `hash()` over the current `md5`/`sha1`/`sha256` subset, `ctype_alpha()`, `ctype_digit()`, `ctype_alnum()`, `ctype_space()`, `bin2hex()`, `hex2bin()`, `base64_encode()`, `base64_decode()`, `date()`, `mktime()`, `strtotime()`, `getenv()`, `putenv()`, `exec()`, `shell_exec()`, `system()`, `passthru()`, `exit()` / `die()`, pure and backed enums (`Enum::cases()`, `Enum::from()`, `Enum::tryFrom()`, enum-case identity loads, and the fatal `from()` miss path), `sort()`, `rsort()`, `asort()`, `arsort()`, `shuffle()`, scalar `array_diff()` / `array_intersect()`, `array_diff_key()`, `array_intersect_key()`, `json_encode()` over scalars/arrays/assoc arrays, `json_decode()` over the full current string-representation contract for quoted strings, trimmed literal/array/object passthrough, surrogate-aware `\uXXXX` UTF-8 decoding, `file_put_contents()`, `file_get_contents()`, `file()`, `file_exists()`, `filesize()`, `filemtime()`, `is_file()`, `is_dir()`, `is_readable()`, `is_writable()`, `mkdir()`, `rmdir()`, `unlink()`, `rename()`, `copy()`, `getcwd()`, `chdir()`, `scandir()`, `glob()`, `tempnam()`, `fopen()`, `fwrite()`, `fclose()`, `fread()`, `fgets()`, `feof()`, `readline()`, `fseek()`, `ftell()`, `rewind()`, `fgetcsv()`, `fputcsv()`, FFI extern calls with string and pointer interop, GC-stats stderr reporting, simple `count(...)` over assoc-array literals, and the current buffer slice (`buffer_new<T>()`, `buffer_len()`, `buffer_free()`, direct scalar/pointer/float buffer reads and writes, bounds/use-after-free traps, and packed-buffer field access). The current x86_64 bootstrap runtime now also emits minimal `__rt_array_new`, `__rt_array_fill`, `__rt_array_fill_refcounted`, `__rt_array_push_int`, `__rt_array_push_refcounted`, `__rt_array_push_str`, `__rt_array_merge_into`, `__rt_array_merge_into_refcounted`, `__rt_array_merge`, `__rt_array_pad`, `__rt_array_product`, `__rt_array_rand`, `__rt_array_reverse`, `__rt_array_sum`, `__rt_array_unique`, `__rt_array_map_str`, `__rt_array_filter`, `__rt_array_filter_refcounted`, `__rt_array_reduce`, `__rt_array_walk`, `__rt_array_column`, `__rt_array_column_ref`, `__rt_array_column_str`, `__rt_random_u32`, `__rt_random_uniform`, `__rt_sort_int`, `__rt_rsort_int`, `__rt_shuffle`, `__rt_asort`, `__rt_arsort`, `__rt_range`, `__rt_array_diff`, `__rt_array_intersect`, `__rt_array_shift`, `__rt_array_slice`, `__rt_array_splice`, `__rt_array_unshift`, `__rt_array_chunk`, `__rt_array_combine`, `__rt_array_flip`, `__rt_hash_iter_next`, `__rt_hash_fnv1a`, `__rt_hash_new`, `__rt_hash_set`, `__rt_hash_get`, `__rt_array_fill_keys`, `__rt_array_fill_keys_refcounted`, `__rt_array_key_exists`, `__rt_array_search`, `__rt_implode`, `__rt_implode_int`, `__rt_explode`, `__rt_trim`, `__rt_ltrim`, `__rt_rtrim`, `__rt_strcopy`, `__rt_ltrim_mask`, `__rt_rtrim_mask`, `__rt_trim_mask`, `__rt_strtoupper`, `__rt_ucwords`, `__rt_str_repeat`, `__rt_chr`, `__rt_nl2br`, `__rt_htmlspecialchars`, `__rt_html_entity_decode`, `__rt_urlencode`, `__rt_urldecode`, `__rt_rawurlencode`, `__rt_md5`, `__rt_sha1`, `__rt_hash`, `__rt_atoi`, `__rt_bin2hex`, `__rt_hex2bin`, `__rt_base64_encode`, `__rt_base64_decode`, `__rt_date`, `__rt_mktime`, `__rt_strtotime`, `__rt_buffer_new`, `__rt_buffer_len`, `__rt_buffer_bounds_fail`, `__rt_buffer_use_after_free`, `__rt_cstr`, `__rt_getenv`, `__rt_shell_exec`, `__rt_ptr_check_nonnull`, `__rt_str_to_cstr`, `__rt_cstr_to_str`, `__rt_fopen`, `__rt_fgets`, `__rt_feof`, `__rt_fread`, `__rt_fgetcsv`, `__rt_fputcsv`, `__rt_file_get_contents`, `__rt_file_put_contents`, `__rt_file`, `__rt_file_exists`, `__rt_is_file`, `__rt_is_dir`, `__rt_is_readable`, `__rt_is_writable`, `__rt_filesize`, `__rt_filemtime`, `__rt_unlink`, `__rt_mkdir`, `__rt_rmdir`, `__rt_chdir`, `__rt_copy`, `__rt_getcwd`, `__rt_scandir`, `__rt_glob`, `__rt_tempnam`, `__rt_json_decode`, `__rt_json_encode_bool`, `__rt_json_encode_null`, `__rt_json_encode_str`, `__rt_json_encode_mixed`, `__rt_json_encode_array_int`, `__rt_json_encode_array_str`, `__rt_json_encode_array_dynamic`, `__rt_json_encode_assoc`, `__rt_exception_cleanup_frames`, `__rt_exception_matches`, `__rt_throw_current`, `__rt_rethrow_current`, `__rt_mixed_write_stdout`, `__rt_incref`, `__rt_decref_array`, `__rt_decref_hash`, `__rt_decref_mixed`, `__rt_decref_object`, `__rt_mixed_from_value`, `__rt_mixed_cast_bool`, `__rt_mixed_cast_int`, `__rt_mixed_free_deep`, `__rt_mixed_is_empty`, `__rt_mixed_unbox`, `__rt_mixed_cast_string`, `__rt_mixed_strict_eq`, `__rt_match_unhandled`, `__rt_enum_from_fail`, and `__rt_str_eq` helpers so that assoc-array lookup/update, callback-driven array mapping/filtering/reduction/walking, `range()`-driven spread construction, `array_column()`, `array_key_exists()`, `array_keys()`, `array_values()`, `array_search()`, `in_array()`, `array_fill_keys()`, `array_pop()`, `array_shift()`, `array_unshift()`, `array_slice()`, `array_splice()`, `array_chunk()`, `array_combine()`, `array_flip()`, `array_merge()`, `array_pad()`, `array_product()`, `array_rand()`, `array_reverse()`, `array_sum()`, `array_unique()`, `sort()`, `rsort()`, `asort()`, `arsort()`, `shuffle()`, scalar `array_diff()` / `array_intersect()`, `array_diff_key()`, `array_intersect_key()`, `implode()`, `explode()`, whitespace and mask trimming, `strcopy()`-backed first-character transforms, `ucwords()`, `strtoupper()`, `str_repeat()`, `chr()`, the current HTML/entity/URL string helpers, the current digest/hash helpers, the current `ctype_*` slice, `bin2hex()`, `hex2bin()`, `base64_encode()`, `base64_decode()`, the current `date()` / `mktime()` / `strtotime()` subset, the current enum bootstrap/lookup/identity slice, the current `getenv()` / `putenv()` / `exec()` / `shell_exec()` / `system()` / `passthru()` / `exit()` bridge, the current `empty()` / `gettype()` semantics for scalar, string, container, and boxed-mixed payloads, boxed-mixed cast-to-bool / cast-to-int coercion, mixed echo, spread-heavy indexed-array construction, computed-key loops, the current `switch` / `match` lowering, the current JSON encode/decode slice, the current filesystem stat/access/read/write/directory/working-directory/tempfile/stream/CSV/seek primitives, the current buffer slice, the current exception lowering/runtime slice, and the current extern-call string/pointer bridge can execute on x86_64 without pulling in the full ARM64 heap/runtime stack. That bootstrap deep-free path now also honors shared string payloads and nested indexed-array payloads when associative-array results borrow values from another owner, and the x86 array-literal fast path now stamps real heap/value-type metadata instead of raw `malloc`-only headers, so nested `json_encode()` over float/bool/object arrays can recurse without tripping over ARM64-only assumptions. The remaining gaps are no longer the basic assoc-array callback paths; they are the broader builtin/runtime families that still assume the ARM64-only heap / deep-free / GC stack in more advanced features, especially objects and the remaining string/formatting/system slices that still use bootstrap shortcuts on x86_64.

That same bootstrap system slice now also includes x86_64-native `time()` / `microtime(true)` through libc `gettimeofday()`, plus constant-string lowering for `phpversion()`, `php_uname()`, and `sys_get_temp_dir()` via the shared symbol-address ABI helpers instead of ARM64-only `adrp` / `add_lo12` sequences.

The x86_64 math surface is broader now too: the libc-backed float builtin family (`sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`, `exp`, `log`, `log2`, `log10`, `atan2`, `hypot`, `pow`) and the pure float helpers (`sqrt`, `pi`, `deg2rad`, `rad2deg`, `min`, `max`) all use SysV floating-point registers plus the shared temporary-stack ABI helpers instead of raw AArch64 `d0` / `scvtf` / `str d0` lowering. The same applies to the `**` operator in expression codegen, which now routes through the x86_64 `pow()` libc call path with the right floating argument order. The scalar random helpers (`rand()`, `mt_rand()`, `random_int()`) also live on that target-aware ABI path now, so their `[min, max]` range materialization no longer emits raw AArch64 stack spills on Linux x86_64.

## Function codegen

**Files:** `src/codegen/functions/mod.rs`, `src/codegen/functions/`

### `emit_function()`

Compiles a user-defined function:

1. **Collect local variables** — scan the function body to find all variables and their types
2. **Calculate stack frame size** — 16-byte aligned, includes space for all locals
3. **Emit prologue** — call the shared ABI frame helper
4. **Store parameters** — lower incoming arguments through the ABI helpers into stack slots, marking by-value heap params as `Owned` and by-reference params as borrowed aliases of the caller's storage
5. **Emit body** — all statements
6. **Emit epilogue** — preserve return registers, save static locals back to BSS through the shared ABI storage helpers, clean up only `Owned` + `epilogue_cleanup_safe` heap locals, then call the shared ABI frame-restore helper and `ret`

### Pass by reference

```php
function increment(&$val) {
    $val++;
}
```

When a parameter is declared with `&`, the codegen passes the **stack address** of the argument instead of its value:

1. At the call site: the address of the argument's stack slot is computed (`sub x_n, x29, #offset`) and passed in the argument register.
2. In the function prologue: the address is stored in the parameter's stack slot (it holds a pointer, not a value).
3. On reads: the codegen dereferences the pointer (`ldr x0, [x0]`) to get the actual value.
4. On writes: the codegen stores through the pointer (`str x0, [addr]`), modifying the caller's variable directly.

The context tracks which parameters are pass-by-reference via `ctx.ref_params`.

### Variadic parameters and spread operator

```php
function sum(...$nums) { /* $nums is an array */ }
sum(1, 2, 3);
sum(...$arr);  // spread
```

**Variadic functions**: The last parameter can be prefixed with `...` to collect all remaining arguments into an array. At the call site, the codegen:

1. Passes regular (non-variadic) arguments normally via registers
2. Uses the shared helpers in `src/codegen/expr/calls/args.rs` to prepare normalized/defaulted argument lists, lower pass-by-reference slots, handle spread-into-named parameters, and build the trailing variadic array when needed
3. Passes the array pointer as the last argument register

**Spread operator** (`...$arr`): When calling a function with `...$arr`, the array is passed directly as the variadic parameter without unpacking individual elements. In array literals, the spread operator uses `__rt_array_merge_into` to append all elements from the spread array into the target array.

### Default parameter values

Functions and closures support default parameter values:

```php
function greet($name, $greeting = "Hello") { ... }
```

When a call site omits an argument that has a default value, the codegen fills in the default. At the call site, the compiler checks how many arguments were actually passed and, for each missing parameter with a default, evaluates the default expression and places it in the appropriate argument register. This is handled at compile time — no runtime checks are needed.

### `collect_local_vars()`

Pre-scans the function body AST to find every variable that will be used. This is necessary because stack space must be allocated in the prologue, before any code runs.

It walks the statement tree before code emission and handles the major local-binding forms recursively (`Assign`, control-flow blocks, `For`/`Foreach`, `ListUnpack`, `Global`, `StaticVar`, and related cases). The exact match is implementation-driven in the `functions/` module, so this list is illustrative rather than exhaustive.

## Main program codegen

**File:** `src/codegen/mod.rs`

The `generate()` function orchestrates everything:

1. **Emit user functions** — scan AST for `FunctionDecl`, emit each one
2. **Emit class methods** — constructor, instance methods, and static methods use their own labels
3. **Emit `_main`**:
   - Prologue (stack frame for global variables)
   - Save `argc` and `argv` from OS (they arrive in `x0` and `x1`)
   - Build `$argv` array via `__rt_build_argv` runtime call
   - Register the main activation record so exceptions can unwind through top-level code too
   - Emit all non-function statements
   - Epilogue: clean up owned locals, unregister the activation record, then `exit(0)`
4. **Emit deferred closures** — closure bodies recorded during earlier expression codegen
5. **Emit runtime routines** — all `__rt_*` helper functions
6. **Emit data section** — string and float literals
7. **Emit runtime data / BSS** — global buffers, globals, statics, and lookup tables

On Linux x86_64, the current minimal runtime slice now also includes the refcounted indexed-array helper family used by GC-sensitive array transforms such as `array_merge()`, `array_slice()`, `array_splice()`, `array_pad()`, `array_chunk()`, `array_diff()`, `array_intersect()`, `array_combine()`, `array_reverse()`, and `array_unique()`. The simple sort family that can piggyback on those indexed-array helpers is on the same path now too: `asort()` / `arsort()`, `ksort()` / `krsort()`, and `natsort()` / `natcasesort()` all dispatch through target-aware x86_64 runtime labels instead of hard-coded ARM64 branches.

That x86_64 slice now also covers the copy-on-write and GC accounting paths for indexed and associative arrays: shallow clone / ensure-unique helpers, owned-hash insertion during clone, heap alloc/free GC counters, indexed-array deep-free, and the x86_64 header-stamping paths needed so nested array writes keep their runtime value-type tags intact.

The x86_64 runtime is no longer limited to the earlier `malloc` / `free` bootstrap wrappers in those paths. `__rt_heap_alloc` and `__rt_heap_free` now mirror the real heap model closely enough to reuse small bins, split and coalesce free-list blocks, trim the bump pointer when the heap tail becomes free again, and drive `_gc_live` / `_gc_peak` / `_gc_allocs` / `_gc_frees` accounting directly from the allocator. The minimal x86_64 runtime now also emits `__rt_gc_mark_reachable` and `__rt_gc_collect_cycles`, so retained arrays, hashes, objects, and boxed mixed values can participate in cycle collection instead of relying only on acyclic decref teardown. That allocator slice now includes the heap-debug/runtime-observability helpers too: `__rt_heap_debug_fail`, `__rt_heap_debug_check_live`, `__rt_heap_debug_validate_free_list`, `__rt_heap_debug_report`, `__rt_heap_kind`, and the x86_64 `__rt_hash_may_have_cyclic_values` path used to skip pointless collector runs for scalar-only hashes.

The same minimal x86_64 runtime now also carries the first string-search / compare slice: `__rt_strpos`, `__rt_strrpos`, `__rt_strcmp`, `__rt_strcasecmp`, `__rt_str_starts_with`, `__rt_str_ends_with`, `__rt_strtolower`, `__rt_strrev`, `__rt_wordwrap`, `__rt_str_split`, `__rt_str_pad`, `__rt_str_replace`, `__rt_str_ireplace`, `__rt_substr_replace`, `__rt_sprintf`, `__rt_number_format`, and `__rt_sscanf`, with the matching builtin lowering for `strpos()`, `strrpos()`, `strcmp()`, `strcasecmp()`, `str_contains()`, `str_starts_with()`, `str_ends_with()`, `strstr()`, `ord()`, `substr()`, `substr_replace()`, `strtolower()`, `strrev()`, `wordwrap()`, `str_split()`, `str_pad()`, `str_replace()`, `str_ireplace()`, `sprintf()`, `printf()`, `number_format()`, and `sscanf()`. That keeps this family on the SysV ABI path instead of falling back to ARM64-only `stp`/`ldp` lowering, and the same ABI conversion helpers now also cover `settype()` when it rewrites locals across `int` / `float` / `string` / `bool` on Linux x86_64.

The remaining inline array/string accessors are on that same path now too: x86_64 string indexing via `ArrayAccess` (`$str[$i]`, including negative offsets) and statement-side indexed-array list unpacking no longer emit raw AArch64 `ldr` / `stp` snippets. They now restore temporaries through the shared ABI helpers and use native SysV register pairs / stack slots instead.

---

Next: [The Runtime →](the-runtime.md)
