---
title: "What is a Compiler?"
description: "What happens when source code becomes a program."
sidebar:
  order: 1
---

## The problem

You write code in a language humans can read — PHP, Python, Rust, C. But the CPU doesn't understand any of that. A CPU only understands **machine code**: sequences of bytes that encode specific operations like "add these two numbers" or "jump to this address".

Something has to bridge the gap between human-readable source code and machine-executable bytes. That something is either a **compiler** or an **interpreter**.

## Compiler vs. interpreter

An **interpreter** reads your source code and executes it on the fly, line by line. PHP normally works this way — `php script.php` reads the file, figures out what each line means, and does it immediately. There's no separate "compiled" file.

A **compiler** reads your source code and translates it into a different form — usually machine code — *before* anything runs. The output is a standalone program (a binary) that the CPU can execute directly. C and Rust work this way.

```
Interpreter:  source code → [interpreter reads + executes] → output
Compiler:     source code → [compiler translates] → binary → [CPU executes] → output
```

The key difference: an interpreter is always present at runtime, translating as it goes. A compiled binary runs on its own — no translator needed.

## What elephc does

elephc is a compiler. It takes PHP source code and produces a **native ARM64 binary** for macOS. No PHP interpreter involved. The output is a standalone executable, just like a C program compiled with `gcc`.

```
hello.php → elephc → hello (Mach-O binary) → runs directly on CPU
```

The resulting binary has no dependency on PHP and no interpreter or VM. It includes elephc's emitted helper routines and links `libSystem` for OS and libc services.

## The phases of compilation

Every compiler, no matter how simple or complex, follows a similar pipeline. Each phase transforms the program into a representation that's closer to machine code:

```
Source text    "if ($x > 0) { echo $x; }"
     │
     ▼
Tokens         [If, LParen, Variable("x"), Greater, Int(0), RParen, LBrace, Echo, Variable("x"), ...]
     │
     ▼
AST            If { condition: BinaryOp(Gt, Var("x"), Int(0)), body: [Echo(Var("x"))] }
     │
     ▼
Typed AST      Same tree, but now we know $x is an Int
     │
     ▼
Assembly       cmp x0, #0 / b.le _else_1 / ... / _else_1:
     │
     ▼
Machine code   Binary executable (Mach-O format on macOS)
```

Each phase has a clear job:

| Phase | Input | Output | Job |
|---|---|---|---|
| [Lexer](the-lexer.md) | Source text | Tokens | Break text into meaningful words |
| [Parser](the-parser.md) | Tokens | AST | Understand the structure (what's nested inside what) |
| [Type Checker](the-type-checker.md) | AST | Typed AST | Figure out and validate data types |
| [Code Generator](the-codegen.md) | Typed AST | Assembly | Translate each construct into CPU instructions |
| Assembler (`as`) | Assembly text | Object file | Convert text mnemonics to binary opcodes |
| Linker (`ld`) | Object file | Executable | Resolve addresses, produce final binary |

elephc handles the first four phases. The last two (assembler and linker) are delegated to macOS system tools.

## Why compile PHP?

PHP is normally interpreted, and that's fine for web servers. So why compile it?

elephc isn't trying to replace PHP. It's an **educational project** — a way to understand how compilers work by building one for a language many people already know. PHP's syntax is simple enough to be tractable but rich enough to be interesting (strings, arrays, functions, control flow, type coercion).

The fact that the output is *real ARM64 assembly* means you can see exactly what the CPU does for every PHP construct. `echo 1 + 2` isn't magic — it's a `mov`, an `add`, a `bl` to a conversion routine, and a `svc` system call. You can trace every step.

---

Next: [Introduction to ARM64 Assembly →](arm64-assembly.md)
