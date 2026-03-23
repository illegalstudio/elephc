# ARM64 Instruction Reference

[← Back to Wiki](README.md) | Previous: [Introduction to ARM64 Assembly](arm64-assembly.md) | Next: [The Pipeline →](how-elephc-works.md)

---

This is every ARM64 instruction elephc uses, organized by category. Each entry shows the instruction, what it does, and where elephc uses it.

## Arithmetic

| Instruction | Syntax | What it does |
|---|---|---|
| `add` | `add x0, x1, x2` | x0 = x1 + x2 |
| `sub` | `sub x0, x1, x2` | x0 = x1 - x2 |
| `mul` | `mul x0, x1, x2` | x0 = x1 × x2 |
| `udiv` | `udiv x0, x1, x2` | x0 = x1 ÷ x2 (unsigned) |
| `sdiv` | `sdiv x0, x1, x2` | x0 = x1 ÷ x2 (signed) |
| `msub` | `msub x0, x1, x2, x3` | x0 = x3 - (x1 × x2). Used for modulo: `a % b = a - (a/b)*b` |
| `neg` | `neg x0, x0` | x0 = -x0 (two's complement negation) |
| `madd` | `madd x0, x1, x2, x3` | x0 = x3 + (x1 × x2). Multiply-accumulate |

### With immediates

| Instruction | Syntax | What it does |
|---|---|---|
| `add` | `add x0, x0, #16` | x0 = x0 + 16 |
| `sub` | `sub sp, sp, #48` | Allocate 48 bytes on the stack |

### Floating-point arithmetic

| Instruction | Syntax | What it does |
|---|---|---|
| `fadd` | `fadd d0, d1, d0` | d0 = d1 + d0 |
| `fsub` | `fsub d0, d1, d0` | d0 = d1 - d0 |
| `fmul` | `fmul d0, d1, d0` | d0 = d1 × d0 |
| `fdiv` | `fdiv d0, d1, d0` | d0 = d1 ÷ d0 |
| `fneg` | `fneg d0, d0` | d0 = -d0 |
| `fsqrt` | `fsqrt d0, d0` | d0 = √d0 |
| `fabs` | `fabs d0, d0` | d0 = |d0| |
| `fmsub` | `fmsub d0, d1, d2, d3` | d0 = d3 - (d1 × d2). Float modulo |
| `frintm` | `frintm d0, d0` | d0 = floor(d0) |
| `frintp` | `frintp d0, d0` | d0 = ceil(d0) |
| `frintn` | `frintn d0, d0` | d0 = round(d0) |
| `fmax` | `fmax d0, d0, d1` | d0 = max(d0, d1) |
| `fmin` | `fmin d0, d0, d1` | d0 = min(d0, d1) |

## Load and store

These move data between registers and memory. See [Introduction to ARM64 Assembly](arm64-assembly.md) for addressing modes.

| Instruction | Syntax | What it does |
|---|---|---|
| `ldr` | `ldr x0, [x29, #-8]` | Load 8 bytes from memory into x0 |
| `ldur` | `ldur x0, [x29, #-8]` | Same as `ldr` but for unaligned/negative offsets |
| `str` | `str x0, [x29, #-8]` | Store x0 (8 bytes) to memory |
| `stur` | `stur x0, [x29, #-8]` | Same as `str` for unaligned/negative offsets |
| `ldrb` | `ldrb w0, [x1]` | Load 1 byte (zero-extended to 32 bits) |
| `ldrsb` | `ldrsb x0, [x1]` | Load 1 byte (sign-extended to 64 bits) |
| `strb` | `strb w0, [x1]` | Store lowest byte of w0 |
| `ldrh` | `ldrh w0, [x1]` | Load 2 bytes (halfword) |
| `strh` | `strh w0, [x1]` | Store 2 bytes |
| `stp` | `stp x29, x30, [sp, #16]` | Store pair of registers (16 bytes total) |
| `ldp` | `ldp x29, x30, [sp, #16]` | Load pair of registers |

### Floating-point load/store

| Instruction | Syntax | What it does |
|---|---|---|
| `ldr` | `ldr d0, [x9]` | Load 8-byte double into float register |
| `str` | `str d0, [x29, #-16]` | Store float register to memory |

### PC-relative addressing (for data section)

| Instruction | Syntax | What it does |
|---|---|---|
| `adrp` | `adrp x1, _str_0@PAGE` | Load the 4KB page address of a label |
| `add` | `add x1, x1, _str_0@PAGEOFF` | Add the offset within the page |

These two always appear together — `adrp` gets the page, `add` gets the exact address. Used for loading string literals and float constants from the [data section](arm64-assembly.md#data-section-constants).

## Move and immediate

| Instruction | Syntax | What it does |
|---|---|---|
| `mov` | `mov x0, x1` | Copy x1 to x0 |
| `mov` | `mov x0, #42` | Load small constant (0-65535) |
| `movz` | `movz x0, #0x1234` | Load 16-bit value, zero the rest |
| `movk` | `movk x0, #0x5678, lsl #16` | Insert 16-bit value at bit position, keep the rest |
| `movn` | `movn x0, #0` | Load bitwise NOT of immediate. `movn x0, #0` = -1 |

### Loading large constants

ARM64 instructions are fixed at 32 bits, so you can't load a 64-bit value in one instruction. Large numbers use `movz` + `movk`:

```asm
; Load 0x7FFFFFFFFFFFFFFE (the null sentinel) into x0
movz x0, #0xFFFE            ; bits  0-15
movk x0, #0xFFFF, lsl #16   ; bits 16-31
movk x0, #0xFFFF, lsl #32   ; bits 32-47
movk x0, #0x7FFF, lsl #48   ; bits 48-63
```

See [Memory Model](memory-model.md) for why this specific value is used as the null sentinel.

## Comparison and conditional

| Instruction | Syntax | What it does |
|---|---|---|
| `cmp` | `cmp x0, #0` | Compare x0 with 0 (sets condition flags) |
| `cmp` | `cmp x0, x1` | Compare x0 with x1 |
| `tst` | `tst x0, #1` | Bitwise AND, set flags (but discard result) |
| `cset` | `cset x0, eq` | x0 = 1 if equal flag set, 0 otherwise |
| `csel` | `csel x0, x1, x2, gt` | x0 = x1 if greater, x2 otherwise |
| `csinc` | `csinc x0, xzr, xzr, eq` | x0 = 1 if NOT equal (inverse cset pattern) |

### Floating-point comparison

| Instruction | Syntax | What it does |
|---|---|---|
| `fcmp` | `fcmp d0, d1` | Compare two doubles, set flags |
| `fcmp` | `fcmp d0, #0.0` | Compare double with zero |

### Condition codes

After `cmp`, these codes test the result:

| Code | Meaning | PHP operator |
|---|---|---|
| `eq` | Equal | `==` |
| `ne` | Not equal | `!=` |
| `lt` | Less than (signed) | `<` |
| `gt` | Greater than (signed) | `>` |
| `le` | Less or equal (signed) | `<=` |
| `ge` | Greater or equal (signed) | `>=` |
| `mi` | Minus (negative) | Used for sign checks |

## Branch (control flow)

| Instruction | Syntax | What it does |
|---|---|---|
| `b` | `b _label` | Unconditional jump |
| `b.eq` | `b.eq _label` | Jump if equal (after `cmp`) |
| `b.ne` | `b.ne _label` | Jump if not equal |
| `b.lt` | `b.lt _label` | Jump if less than |
| `b.gt` | `b.gt _label` | Jump if greater than |
| `b.le` | `b.le _label` | Jump if less or equal |
| `b.ge` | `b.ge _label` | Jump if greater or equal |
| `b.mi` | `b.mi _label` | Jump if negative |
| `cbz` | `cbz x0, _label` | Jump if x0 == 0 |
| `cbnz` | `cbnz x0, _label` | Jump if x0 != 0 |
| `bl` | `bl _fn_add` | Branch with link — call a function (saves return address in x30) |
| `ret` | `ret` | Return — jump to address in x30 |

### How branches map to PHP

See [The Code Generator](the-codegen.md) for full details.

```
if ($x > 0)     →  cmp x0, #0  /  b.le _else_1
while ($i < 10)  →  cmp x0, #10  /  b.ge _end_while_1
break            →  b _end_while_1
continue         →  b _while_1
```

## Type conversion

| Instruction | Syntax | What it does |
|---|---|---|
| `scvtf` | `scvtf d0, x0` | Signed integer → double float |
| `fcvtzs` | `fcvtzs x0, d0` | Double float → signed integer (truncate toward zero) |

Used for PHP type casting (`(int)3.14`, `(float)42`) and mixed arithmetic. See [The Type Checker](the-type-checker.md).

## Bitwise

| Instruction | Syntax | What it does |
|---|---|---|
| `and` | `and x0, x0, #0xFF` | Bitwise AND |
| `orr` | `orr x0, x0, x1` | Bitwise OR |
| `eor` | `eor x0, x0, x1` | Bitwise XOR |
| `lsr` | `lsr x0, x0, #4` | Logical shift right |
| `lsl` | `lsl x0, x0, #3` | Logical shift left |
| `asr` | `asr x0, x0, #63` | Arithmetic shift right (preserves sign) |

Used in [runtime routines](the-runtime.md) for things like hex conversion, hash algorithms, and base64 encoding.

## System

| Instruction | Syntax | What it does |
|---|---|---|
| `svc` | `svc #0x80` | Supervisor call — invoke the macOS kernel |

Before `svc`, set up: `x16` = syscall number, `x0`-`x5` = arguments.

| Syscall | x16 | Arguments | Used for |
|---|---|---|---|
| exit | `#1` | x0 = exit code | `exit()`, program termination |
| write | `#4` | x0 = fd, x1 = buf, x2 = len | `echo`, `print` |

## Assembly directives

These aren't CPU instructions — they're commands to the assembler:

| Directive | Example | What it does |
|---|---|---|
| `.global` | `.global _main` | Make label visible to the linker |
| `.align` | `.align 2` | Align next data to 2^2 = 4 bytes |
| `.data` | `.data` | Switch to data section |
| `.text` | `.text` | Switch to code section |
| `.ascii` | `.ascii "hello"` | Emit string bytes (no null terminator) |
| `.quad` | `.quad 0x4028000000000000` | Emit 8-byte value (used for float constants) |
| `.comm` | `.comm _heap_buf, 1048576` | Reserve uninitialized memory (BSS section) |

---

Next: [The Pipeline →](how-elephc-works.md)
