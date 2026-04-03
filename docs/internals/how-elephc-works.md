---
title: "The Pipeline"
description: "The full journey from PHP source to a running binary."
sidebar:
  order: 2
---

This page walks through the entire compilation process — from PHP source to running binary — using a concrete example.

## The example

```php
<?php
$x = 10;
if ($x > 5) {
    echo "big\n";
}
```

Let's follow this through every phase.

## Phase 1: Lexing

**File:** `src/lexer/` — See [The Lexer](the-lexer.md) for details.

The lexer reads the source character by character and produces a sequence of tokens:

```
OpenTag          <?php
Variable("x")   $x
Assign           =
IntLiteral(10)   10
Semicolon        ;
If               if
LParen           (
Variable("x")   $x
Greater          >
IntLiteral(5)    5
RParen           )
LBrace           {
Echo             echo
StringLiteral("big\n")  "big\n"
Semicolon        ;
RBrace           }
Eof
```

Each token also carries a **Span** — its line and column number — for error reporting.

## Phase 2: Parsing

**File:** `src/parser/` — See [The Parser](the-parser.md) for details.

The parser reads the token stream and builds an **Abstract Syntax Tree** (AST):

```
Program [
    Assign {
        name: "x",
        value: IntLiteral(10)
    },
    If {
        condition: BinaryOp {
            left: Variable("x"),
            op: Gt,
            right: IntLiteral(5)
        },
        then_body: [
            Echo(StringLiteral("big\n"))
        ],
        elseif_clauses: [],
        else_body: None
    }
]
```

The tree captures the **structure** — `IntLiteral(5)` is the right operand of `Gt`, and `Echo` is inside the `then_body` of the `If`. Token details like parentheses and braces are gone — they served their purpose during parsing.

## Phase 3: Conditional compilation

**File:** `src/conditional.rs`

If the program uses elephc-only `ifdef SYMBOL { ... } else { ... }` blocks, the conditional pass evaluates them against the active CLI `--define` symbols and removes the inactive branches from the AST before any include resolution or type checking happens.

In this example, there are no `ifdef` blocks, so the AST passes through unchanged.

## Phase 4: Resolving

**File:** `src/resolver.rs`

If the program had `include` or `require` statements, the resolver would parse those files and inline their ASTs. In this example, there's nothing to resolve — the AST passes through unchanged.

## Phase 5: Name resolution

**File:** `src/name_resolver.rs`

After includes are flattened, elephc resolves namespace-aware names. This pass applies the current `namespace`, any `use` / `use function` / `use const` imports, and rewrites references to their canonical fully-qualified names before semantic analysis.

In this example there are no namespaces or imports, so the AST still passes through unchanged.

## Phase 6: Type checking

**File:** `src/types/` — See [The Type Checker](the-type-checker.md) for details.

The type checker walks the AST and determines the type of every variable and expression:

```
$x = 10           →  $x: Int
$x > 5            →  Int > Int → Bool  ✓
echo "big\n"      →  Str  ✓
```

It builds a **type environment** — a map from variable names to their types:

```
{ "x" → Int, "argc" → Int, "argv" → Array(Str) }
```

If you tried `$x = "hello"` after `$x = 10`, the type checker would reject it — elephc doesn't allow variables to change type (except from `null`). The checker also resolves class/interface metadata for exception handling, so `throw` only accepts objects implementing `Throwable` and each `catch` target can be matched correctly later in codegen.

On successful type checking, elephc also runs a warning pass that reports issues such as unused variables and unreachable code. On failing compilations, the parser and checker both try to recover conservatively so they can often report more than one independent error in a single run.

## Phase 7: Code generation

**File:** `src/codegen/` — See [The Code Generator](the-codegen.md) for details.

The code generator walks the typed AST and emits ARM64 assembly. For ordinary control flow this is mostly straight-line branches and labels; for `try` / `catch` / `finally`, the compiler additionally emits handler records and resume labels around `_setjmp` / `_longjmp`-based exception unwinding. Here's what our simple example produces (simplified, with comments):

```asm
.global _main
.align 2

_main:
    ; -- prologue: set up stack frame --
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    add x29, sp, #16

    ; -- $x = 10 --
    mov x0, #10
    stur x0, [x29, #-8]

    ; -- if ($x > 5) --
    ldur x0, [x29, #-8]         ; load $x
    cmp x0, #5                   ; compare with 5
    b.le _end_if_1               ; skip body if $x <= 5

    ; -- echo "big\n" --
    adrp x1, _str_0@PAGE
    add x1, x1, _str_0@PAGEOFF
    mov x2, #4                   ; length = 4 ("big" + newline)
    mov x0, #1                   ; fd = stdout
    mov x16, #4                  ; syscall = write
    svc #0x80                    ; call kernel

_end_if_1:
    ; -- epilogue: exit(0) --
    mov x0, #0
    mov x16, #1
    svc #0x80

.data
_str_0: .ascii "big\n"
```

Key observations:

- `$x = 10` → `mov x0, #10` then `stur` to the stack at offset -8 from the frame pointer
- `if ($x > 5)` → `cmp` + `b.le` (branch if **not** greater — the condition is inverted)
- `echo "big\n"` → load string address + length, then `svc` to write to stdout
- The string literal lives in the `.data` section, referenced by label `_str_0`

## Phase 8: Assembly and linking

**Tools:** macOS `as` and `ld`

elephc writes the assembly to a `.s` file, then invokes the system tools:

```bash
as -arch arm64 -o file.o file.s     # text assembly → object file (binary)
ld -arch arm64 -e _main -o file file.o -lSystem -syslibroot /path/to/sdk
```

- **`as`** (assembler) converts text mnemonics into binary machine code, producing an object file (`.o`)
- **`ld`** (linker) resolves label addresses, links with system libraries (for `svc`), and produces the final **Mach-O binary**

The `.o` file is deleted after linking. The result is a standalone executable.

## Phase 9: Execution

```bash
./file
big
```

The binary runs directly on the CPU. There is no PHP interpreter or VM at runtime. The kernel loads the Mach-O file into memory, jumps to `_main`, and the CPU executes the instructions we generated. The binary still contains elephc's emitted helper routines and links `libSystem` for OS/libc services.

## The complete flow

```
"<?php $x = 10; if ($x > 5) { echo \"big\\n\"; }"
                    │
                    ▼ Lexer
    [OpenTag, Variable("x"), Assign, IntLiteral(10), ...]
                    │
                    ▼ Parser
    [Assign{x, 10}, If{Gt(Var(x), 5), [Echo("big\n")]}]
                    │
                    ▼ Conditional (ifdef no-op here)
                    │
                    ▼ Resolver (no-op here)
                    │
                    ▼ NameResolver (no-op here)
                    │
                    ▼ Type Checker
    { x: Int } — all types consistent ✓
                    │
                    ▼ Code Generator
    "sub sp, sp, #32 / stp x29, x30, ... / mov x0, #10 / ..."
                    │
                    ▼ as (assembler)
    file.o (machine code bytes)
                    │
                    ▼ ld (linker)
    file (Mach-O executable)
                    │
                    ▼ CPU
    "big\n"
```

---

Next: [The Lexer →](the-lexer.md)
