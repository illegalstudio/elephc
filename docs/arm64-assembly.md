# Introduction to ARM64 Assembly

[← Back to Wiki](README.md) | Previous: [What is a Compiler?](what-is-a-compiler.md) | Next: [ARM64 Instruction Reference →](arm64-instructions.md)

---

## What is assembly language?

Assembly is the thinnest possible layer above raw machine code. Each assembly instruction maps (almost) 1-to-1 to a single CPU operation. Where a high-level language says `$x = $a + $b`, assembly says:

```asm
ldr x1, [x29, #-16]     ; load $a from the stack into register x1
ldr x2, [x29, #-24]     ; load $b from the stack into register x2
add x0, x1, x2          ; add them, put the result in x0
str x0, [x29, #-32]     ; store the result back to the stack ($x)
```

Every line is one operation. The CPU reads them sequentially (unless a branch instruction says otherwise).

## What is ARM64?

ARM64 (also called AArch64) is the instruction set used by Apple Silicon chips (M1, M2, M3, M4) and most modern smartphones. It's a **RISC** (Reduced Instruction Set Computer) architecture — instructions are simple, fixed-size (4 bytes each), and uniform.

Compare with x86-64 (Intel/AMD), which is **CISC** (Complex Instruction Set) — variable-length instructions with many special cases. ARM64 is cleaner and easier to learn.

## Registers: the CPU's variables

A register is a tiny, ultra-fast storage location inside the CPU. ARM64 has 31 general-purpose registers, each 64 bits (8 bytes) wide:

### General-purpose registers

| Register | Convention | elephc usage |
|---|---|---|
| `x0`-`x7` | Function arguments and return values | Arguments passed to/from functions. `x0` = integer/bool result |
| `x8` | Indirect result | Scratch register |
| `x9`-`x15` | Temporary (caller-saved) | Scratch for intermediate computations |
| `x16`-`x17` | Intra-procedure scratch | `x16` = syscall number on macOS |
| `x29` | Frame pointer (FP) | Points to current function's stack frame |
| `x30` | Link register (LR) | Return address (where to go after `ret`) |
| `sp` | Stack pointer | Top of the stack (grows downward) |

You can also use `w0`-`w30` to access only the lower 32 bits of each register (useful for byte operations like `strb w12, [x9]`).

### Floating-point registers

| Register | Usage |
|---|---|
| `d0`-`d7` | Float arguments and return values |
| `d8`-`d15` | Callee-saved (preserved across function calls) |
| `d16`-`d31` | Temporary |

In elephc, `d0` holds float results. Float arguments to functions use `d0`-`d7`.

### How elephc uses registers

elephc follows a simple convention (see [The Code Generator](the-codegen.md) for details):

```
Integer/Bool result  → x0
Float result         → d0
String result        → x1 (pointer to bytes), x2 (length)
Array result         → x0 (pointer to heap-allocated header)
```

## The stack: function-local storage

The **stack** is a region of memory that grows downward (from high addresses to low addresses). Each function call creates a **stack frame** — a block of memory for that function's local variables.

```
High addresses
┌─────────────────────┐
│  caller's frame      │
├─────────────────────┤ ← x29 (frame pointer) points here
│  saved x29, x30      │  (16 bytes: frame pointer + return address)
├─────────────────────┤
│  local variable 1    │  [x29, #-8]
│  local variable 2    │  [x29, #-16]
│  local variable 3    │  [x29, #-24]
│  ...                 │
├─────────────────────┤ ← sp (stack pointer) points here
Low addresses
```

Key concepts:

- **`sp`** (stack pointer) marks the current top of the stack. You allocate space by subtracting from `sp`.
- **`x29`** (frame pointer) marks the base of the current frame. Local variables are accessed at negative offsets from `x29`.
- **`x30`** (link register) holds the return address — where the CPU should jump when the function finishes.

### Function prologue and epilogue

Every function starts with a **prologue** (set up the frame) and ends with an **epilogue** (tear it down):

```asm
; Prologue
sub sp, sp, #48          ; allocate 48 bytes on the stack
stp x29, x30, [sp, #32] ; save old frame pointer and return address
add x29, sp, #32        ; set new frame pointer

; ... function body ...

; Epilogue
ldp x29, x30, [sp, #32] ; restore frame pointer and return address
add sp, sp, #48          ; deallocate stack space
ret                      ; jump to address in x30
```

This is what elephc generates for every function. See [The Code Generator](the-codegen.md) for the full details.

## Memory: load and store

ARM64 is a **load/store architecture**. You can't operate directly on memory — you must load values into registers first, operate on them, then store results back:

```asm
ldr x0, [x29, #-8]      ; LOAD: read 8 bytes from stack into x0
add x0, x0, #1           ; OPERATE: add 1 to x0
str x0, [x29, #-8]       ; STORE: write x0 back to the stack
```

This is why `$i++` in PHP becomes at least 3 instructions in assembly.

### Addressing modes

| Syntax | Meaning | Example |
|---|---|---|
| `[x29, #-16]` | Base + offset | Load from 16 bytes below frame pointer |
| `[x1]` | Base only | Load from address in x1 |
| `[x0, x1, lsl #3]` | Base + shifted index | Array access: base + (index × 8) |

## System calls: talking to the OS

The CPU can't print to the screen or read files on its own — it needs to ask the operating system. On macOS ARM64, this is done with the `svc` (supervisor call) instruction:

```asm
mov x0, #1          ; file descriptor 1 = stdout
; x1 = pointer to string data (already set)
; x2 = string length (already set)
mov x16, #4         ; syscall number 4 = write
svc #0x80           ; invoke the kernel
```

This is how `echo` works in elephc — every `echo` ultimately becomes a `write` system call. See [The Runtime](the-runtime.md) for more details on how values are converted to strings before printing.

## Branches: control flow

The CPU executes instructions sequentially unless a **branch** changes the flow:

| Instruction | Meaning | Used for |
|---|---|---|
| `b label` | Unconditional jump | `else` blocks, loop back-edges |
| `b.eq label` | Branch if equal | After `cmp`, for `==` |
| `b.ne label` | Branch if not equal | For `!=` |
| `b.lt label` | Branch if less than | For `<` |
| `b.gt label` | Branch if greater than | For `>` |
| `b.le label` | Branch if less or equal | For `<=` |
| `b.ge label` | Branch if greater or equal | For `>=` |
| `cbz x0, label` | Branch if x0 is zero | `if` conditions (falsy check) |
| `cbnz x0, label` | Branch if x0 is not zero | Loop conditions |
| `bl label` | Branch with link (function call) | Saves return address in x30 |
| `ret` | Return from function | Jumps to address in x30 |

### How an `if` becomes assembly

```php
if ($x > 0) {
    echo "positive";
}
```

becomes (simplified):

```asm
ldr x0, [x29, #-8]      ; load $x
cmp x0, #0               ; compare $x with 0
b.le _end_if_1           ; if $x <= 0, skip the body
; ... emit "positive" ...
_end_if_1:
```

See [ARM64 Instruction Reference](arm64-instructions.md) for every instruction elephc uses, and [The Code Generator](the-codegen.md) for how each PHP construct maps to assembly.

## Labels: named positions

Labels are names for positions in the code. They don't generate instructions — they just mark addresses that branches can jump to:

```asm
_while_1:                ; ← this is a label
    ldr x0, [x29, #-8]
    cmp x0, #10
    b.ge _end_while_1   ; jump forward to end
    ; ... loop body ...
    b _while_1           ; jump back to start
_end_while_1:            ; ← another label
```

In elephc, labels are generated with a global counter to avoid collisions: `_while_1`, `_while_2`, `_if_3`, etc. See [The Code Generator](the-codegen.md) for how `Context::next_label()` works.

## Data section: constants

The assembly output has two sections:

- **`.text`** — executable code (instructions)
- **`.data`** — read-only data (string literals, float constants)

```asm
.data
_str_0: .ascii "Hello, world!\n"    ; 14 bytes
_float_0: .quad 0x400921FB54442D18  ; 3.14159... stored as raw bits

.text
; ... code that references _str_0 and _float_0 ...
```

String literals are embedded directly in the binary. To use them, you load their address with `adrp` + `add` (see [ARM64 Instruction Reference](arm64-instructions.md)).

---

Next: [ARM64 Instruction Reference →](arm64-instructions.md)
