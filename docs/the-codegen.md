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
    pub deferred_closures: Vec<DeferredClosure>, // closures emitted after _main
    pub constants: HashMap<String, (ExprKind, PhpType)>, // compile-time constants
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
| `Object` | `x0` (heap pointer) |

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

### Function calls

```php
my_func($a, $b, $c)
```

1. Evaluate each argument and push results onto the stack
2. Pop arguments into the correct ABI registers (`x0`-`x7` for ints, `d0`-`d7` for floats, two registers per string)
3. `bl _fn_my_func` — branch with link (saves return address)
4. Result is in `x0`/`d0`/`x1`+`x2` depending on return type

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

Arrow functions (`fn($x) => ...`) automatically capture all referenced outer variables — no `use` clause needed.

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

Built-in functions like `array_map`, `array_filter`, `usort` etc. accept closures as callback arguments. The closure's function pointer is passed in a register (like any other `Callable` argument) and the runtime routine calls it via `blr`.

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
2. On assignment to a global variable: the codegen writes to the BSS storage (`_gvar_x`) via `adrp`/`add`/`str` instead of (or in addition to) the local stack slot.
3. In `_main`: when the main scope assigns to a variable that any function declares as `global`, the value is also written to `_gvar_NAME` so that functions can read it.

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

At function epilogue, variables marked as static are written back to their BSS storage.

### List unpacking

```php
[$a, $b, $c] = [10, 20, 30];
```

`ListUnpack` destructures an indexed array into individual variables. The codegen:

1. Evaluates the right-hand side expression (an array)
2. Saves the array pointer on the stack
3. For each variable in the list: loads the element at the corresponding index from the array, stores it into the variable's stack slot

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

1. **Calculate object size**: `8 + (num_properties × 16)` — 8 bytes for the class ID, 16 bytes per property
2. **Allocate heap memory**: call `__rt_heap_alloc` with the calculated size
3. **Zero-initialize**: clear all property slots to zero
4. **Store class ID**: write the class identifier at offset 0
5. **Apply defaults**: for properties with default values, evaluate and store them at their fixed offsets
6. **Call constructor**: if the class has a `__construct` method, pass the new object pointer as `x0` (`$this`) followed by the constructor arguments, then `bl _method_ClassName___construct`

The result is the object pointer in `x0`.

### Property access (`$obj->prop`)

Property access uses fixed offsets computed at compile time:

```asm
; $obj->prop where prop is property index 1
ldur x0, [x29, #-offset]            ; load object pointer
ldur x0, [x0, #24]                  ; load property at 8 + 1 * 16 = 24
```

For property assignment (`$obj->prop = value`), the value is evaluated first, then stored at the computed offset.

### Method call (`$obj->method(args)`)

1. Evaluate the object expression to get the pointer in `x0`
2. Push the object pointer onto the stack
3. Evaluate and push all arguments
4. Pop arguments into ABI registers, with the object pointer as the first argument (`x0`)
5. `bl _method_ClassName_methodName`
6. Result is in the standard registers based on return type

Inside the method body, `$this` is the first parameter and lives in the function's first stack slot.

### Static method call (`ClassName::method(args)`)

Static methods are called like regular functions, but with the label `_static_ClassName_methodName`. No object pointer is passed:

```asm
bl _static_Point_origin              ; call static method
; result in x0 (object pointer)
```

## The ABI module

**File:** `src/codegen/abi.rs`

Centralizes register conventions so they're consistent everywhere:

### Large offset addressing

ARM64's `stur`/`ldur` instructions only support 9-bit signed immediates (offsets up to 255). Functions with many local variables can exceed this limit. The ABI module handles this transparently via `store_at_offset()` and `load_at_offset()`:

- **Offsets <= 255**: single `stur`/`ldur` instruction (fast path)
- **Offsets 256-4095**: two-instruction sequence — `sub x9, x29, #offset` to compute the address in a scratch register, then `str`/`ldr` through that register

This means all codegen that accesses stack variables goes through the ABI helpers rather than emitting `stur`/`ldur` directly, so large stack frames work automatically.

### `emit_store(emitter, type, offset)`

Stores the current result to a stack variable. Uses `store_at_offset()` internally to handle large offsets:

| Type | What it stores |
|---|---|
| `Int` / `Bool` | `stur x0, [x29, #-offset]` (or 2-insn sequence for large offsets) |
| `Float` | `stur d0, [x29, #-offset]` |
| `Str` | `stur x1, [x29, #-offset]` + `stur x2, [x29, #-(offset-8)]` |
| `Array` | `stur x0, [x29, #-offset]` |
| `Object` | `stur x0, [x29, #-offset]` |

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
2. Builds a new indexed array for the variadic arguments by calling `__rt_array_new` and `__rt_array_push_int`/`__rt_array_push_str` for each extra argument
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

It walks `Assign`, `If`, `While`, `For`, `Foreach`, `DoWhile`, `Switch`, `ListUnpack`, `Global`, `StaticVar` nodes recursively, collecting variable names and inferring their types from the expressions assigned to them.

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
