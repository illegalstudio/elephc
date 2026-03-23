# The Code Generator

[← Back to Wiki](README.md) | Previous: [The Type Checker](the-type-checker.md) | Next: [The Runtime →](the-runtime.md)

---

**Source:** `src/codegen/` — `mod.rs`, `expr.rs`, `stmt.rs`, `functions.rs`, `abi.rs`, `context.rs`, `data_section.rs`, `emit.rs`

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

; --- main program ---
_main:
    ; prologue (stack frame setup)
    ; global argc/argv initialization
    ; program statements
    ; epilogue (exit syscall)

; --- runtime routines ---
__rt_itoa:
    ...
__rt_concat:
    ...

; --- data section ---
.data
_str_0: .ascii "hello"
_float_0: .quad 0x400921FB54442D18
```

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
}
```

Each variable has a `VarInfo`:

```rust
pub struct VarInfo {
    pub ty: PhpType,         // Int, Float, Str, etc.
    pub stack_offset: usize, // offset from frame pointer (x29)
}
```

### Label generation

`ctx.next_label("while")` produces `_while_1`, `_while_2`, etc. A global atomic counter ensures labels never collide across functions or compilation units.

## The Data Section

**File:** `src/codegen/data_section.rs`

String literals and float constants are stored in the `.data` section:

```rust
pub struct DataSection {
    entries: Vec<(String, Vec<u8>)>,          // string label → bytes
    float_entries: Vec<(String, u64)>,        // float label → bit pattern
    dedup: HashMap<Vec<u8>, String>,          // avoid duplicate strings
    float_dedup: HashMap<u64, String>,        // avoid duplicate floats
}
```

When the codegen encounters `"hello"`, it calls `data.add_string(b"hello")` which returns a label (`_str_0`) and length (`5`). Identical strings are deduplicated — two `"hello"` literals share the same label.

Floats are stored as their raw 64-bit IEEE 754 bit patterns (`.quad` directive).

## Expression codegen

**File:** `src/codegen/expr.rs`

`emit_expr()` takes an expression node and emits code that leaves the result in the standard registers:

| Type | Result location |
|---|---|
| `Int` / `Bool` | `x0` |
| `Float` | `d0` |
| `Str` | `x1` (pointer), `x2` (length) |
| `Array` | `x0` (heap pointer) |

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

### Function calls

```php
my_func($a, $b, $c)
```

1. Evaluate each argument and push results onto the stack
2. Pop arguments into the correct ABI registers (`x0`-`x7` for ints, `d0`-`d7` for floats, two registers per string)
3. `bl _fn_my_func` — branch with link (saves return address)
4. Result is in `x0`/`d0`/`x1`+`x2` depending on return type

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
3. Call `__rt_hash_get` → `x0` = found (0/1), `x1` = value_lo, `x2` = value_hi
4. Move result to standard registers based on value type

### Functions on associative arrays

Builtin functions like `array_key_exists`, `in_array`, `array_keys`, `array_values` dispatch on the array type at compile time:
- `PhpType::Array` → use indexed runtime routines (e.g., bounds check, linear scan)
- `PhpType::AssocArray` → use hash table routines (e.g., `__rt_hash_get`, `__rt_hash_iter_next`)

See [The Runtime](the-runtime.md) for details on hash table routines and [Memory Model](memory-model.md) for the hash table memory layout.

## Statement codegen

**File:** `src/codegen/stmt.rs`

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
2. `emit_store()` — write result to `$x`'s stack slot

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

1. Save array pointer, length, and index counter on the stack (3 × 16-byte slots)
2. Loop: load element at current index, store to `$v`, emit body, increment index
3. Branch back to condition check
4. Cleanup: deallocate the 48 bytes

### Break / Continue

`break` emits a `b` (unconditional jump) to the current loop's end label.
`continue` emits a `b` to the loop's continue label (the condition check for `while`, the update for `for`).

The `loop_stack` in the Context tracks which labels to jump to for nested loops.

## The ABI module

**File:** `src/codegen/abi.rs`

Centralizes register conventions so they're consistent everywhere:

### `emit_store(emitter, type, offset)`

Stores the current result to a stack variable:

| Type | What it stores |
|---|---|
| `Int` / `Bool` | `stur x0, [x29, #-offset]` |
| `Float` | `stur d0, [x29, #-offset]` |
| `Str` | `stur x1, [x29, #-offset]` + `stur x2, [x29, #-(offset-8)]` |
| `Array` | `stur x0, [x29, #-offset]` |

### `emit_load(emitter, type, offset)`

Loads a stack variable into result registers (inverse of store).

### `emit_write_stdout(emitter, type)`

Emits code to print a value to stdout:

| Type | How it prints |
|---|---|
| `Str` | `mov x0, #1` / `mov x16, #4` / `svc #0x80` (direct syscall) |
| `Int` | `bl __rt_itoa` → then write |
| `Float` | `bl __rt_ftoa` → then write |
| `Bool` | `true` prints "1", `false` prints nothing |
| `Void`/`Array` | Prints nothing |

## Function codegen

**File:** `src/codegen/functions.rs`

### `emit_function()`

Compiles a user-defined function:

1. **Collect local variables** — scan the function body to find all variables and their types
2. **Calculate stack frame size** — 16-byte aligned, includes space for all locals
3. **Emit prologue** — `sub sp`, `stp x29, x30`, `add x29`
4. **Store parameters** — move from argument registers to stack slots
5. **Emit body** — all statements
6. **Emit epilogue** — `ldp x29, x30`, `add sp`, `ret`

### `collect_local_vars()`

Pre-scans the function body AST to find every variable that will be used. This is necessary because stack space must be allocated in the prologue, before any code runs.

It walks `Assign`, `If`, `While`, `For`, `Foreach`, `DoWhile` nodes recursively, collecting variable names and inferring their types from the expressions assigned to them.

## Main program codegen

**File:** `src/codegen/mod.rs`

The `generate()` function orchestrates everything:

1. **Emit user functions** — scan AST for `FunctionDecl`, emit each one
2. **Emit `_main`**:
   - Prologue (stack frame for global variables)
   - Save `argc` and `argv` from OS (they arrive in `x0` and `x1`)
   - Build `$argv` array via `__rt_build_argv` runtime call
   - Emit all non-function statements
   - Epilogue: `exit(0)` syscall
3. **Emit runtime routines** — all `__rt_*` helper functions
4. **Emit data section** — string and float literals

---

Next: [The Runtime →](the-runtime.md)
