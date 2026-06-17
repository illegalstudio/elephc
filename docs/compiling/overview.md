---
title: "Compiling Overview"
description: "How the elephc command-line compiler turns a PHP file into a standalone native binary, and a map of the compilation documentation."
sidebar:
  order: 1
---

elephc is an ahead-of-time compiler: it reads a `.php` file and produces a
standalone native binary with no interpreter, no virtual machine, and no runtime
dependencies. The generated program is plain machine code linked against a small
hand-written runtime that is baked into the executable.

## Basic invocation

```bash
elephc hello.php
./hello
```

The compiler writes the output binary next to the source file, using the source
name without its extension (`hello.php` → `hello`). Nothing else is produced by
default — no intermediate object files are left behind, no cache pollution in
your project directory.

```bash
elephc src/app.php     # produces ./src/app
./src/app
```

## What happens during a compile

A compile runs the source through a fixed sequence of phases — lexing, parsing,
name resolution, type checking, AST optimization, lowering to elephc's
intermediate representation (EIR), EIR optimization, native code generation, and
linking. Each phase is described in
[The compilation pipeline](compilation-pipeline.md), and you can see how long
each one takes with [`--timings`](output-and-diagnostics.md#--timings).

By default the compiler:

- targets the **host** platform (see [Targets and cross-compilation](targets.md)),
- uses the **EIR backend** with the **linear-scan register allocator** and the
  **EIR optimization passes** enabled (see [Optimization and codegen controls](optimization.md)),
- emits a native **executable** (see [Output formats and diagnostics](output-and-diagnostics.md)),
- and uses an **8 MB** heap (see [Linking, heap, and conditional compilation](linking-and-conditional-compilation.md#heap-size)).

Every one of these defaults can be changed with a command-line flag.

## No runtime, one file

The runtime routines the program needs (string formatting, the allocator,
reference-counting GC, hash tables, I/O, exception unwinding, and so on) are
emitted as assembly and linked directly into the binary. The result is a single
self-contained executable you can copy to another machine of the same target and
run. See [The Runtime](../internals/the-runtime.md) for what those routines are.

## Documentation map

| Page | Covers |
|---|---|
| [The compilation pipeline](compilation-pipeline.md) | Every phase from source text to binary, in order |
| [CLI reference](cli-reference.md) | The complete, authoritative list of every flag |
| [Targets and cross-compilation](targets.md) | The supported target matrix and `--target` |
| [Optimization and codegen controls](optimization.md) | `--regalloc`, `--ir-opt`, `--null-repr`, and what they do |
| [Output formats and diagnostics](output-and-diagnostics.md) | `--emit`, `--emit-asm`, `--emit-ir`, `--check`, `--timings`, `--source-map`, `--gc-stats`, `--heap-debug` |
| [Linking, heap, and conditional compilation](linking-and-conditional-compilation.md) | `--link`, `--link-path`, `--framework`, `--heap-size`, `--define` |

For the internals behind these phases, see
[The Pipeline](../internals/how-elephc-works.md),
[The Code Generator](../internals/the-codegen.md), and
[The EIR Design](../internals/the-ir.md).
