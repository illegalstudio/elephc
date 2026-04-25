---
title: "elephc Documentation"
description: "A PHP-to-native compiler. Compiles a static subset of PHP to native assembly and produces standalone binaries for supported targets."
sidebar:
  order: 0
---

elephc compiles PHP to native binaries for the supported targets — currently macOS ARM64, Linux ARM64, and Linux x86_64. No interpreter, no VM, no runtime dependencies. This documentation covers everything from PHP syntax support to compiler-specific extensions and internal architecture.

## Getting Started

- [Installation](getting-started/installation.md) — install elephc via Homebrew or from source
- [Your First Program](getting-started/your-first-program.md) — write, compile, and run your first PHP binary
- [Benchmark Suite](https://github.com/illegalstudio/elephc/blob/main/benchmarks/README.md) — compare elephc against PHP and equivalent C fixtures

## PHP Syntax

Standard PHP features supported by elephc. All PHP syntax is **100% compatible** with the PHP interpreter.

- [Types](php/types.md) — int, float, string, bool, array, null, mixed, callable, enum, union types, extension types, type casting
- [Operators](php/operators.md) — arithmetic, comparison, logical, bitwise, string, assignment, ternary, null coalescing
- [Control Structures](php/control-structures.md) — if/else, while, for, foreach, switch, match, try/catch/finally
- [Functions](php/functions.md) — declarations, closures, arrow functions, variadic, spread, pass-by-reference, static variables
- [Strings](php/strings.md) — escape sequences, interpolation, heredoc/nowdoc, 50+ built-in string functions
- [Arrays](php/arrays.md) — indexed, associative, copy-on-write, 45+ built-in array functions
- [Math](php/math.md) — abs, floor, ceil, round, trigonometry, logarithms, random, constants
- [Classes](php/classes.md) — inheritance, interfaces, abstract/final classes, typed/final/static properties and methods, traits, enums, magic methods
- [Namespaces](php/namespaces.md) — namespace, use, include/require, constants, superglobals
- [System & I/O](php/system-and-io.md) — file I/O, date/time, JSON, regex, exec, debugging

## Beyond PHP

Compiler-specific extensions that go beyond standard PHP. These features have no PHP equivalent and exist to enable use cases PHP was never designed for.

- [Pointers](beyond-php/pointers.md) — ptr(), ptr_get(), ptr_set(), pointer arithmetic, typed casting
- [Buffers](beyond-php/buffers.md) — buffer&lt;T&gt; for fixed-size contiguous arrays, hot-path data
- [Packed Classes](beyond-php/packed-classes.md) — flat POD records with compile-time field offsets
- [FFI & Extern](beyond-php/extern.md) — calling C libraries, extern functions/globals/classes, callbacks
- [Conditional Compilation](beyond-php/ifdef.md) — ifdef blocks, compile-time feature flags, CLI flags

## Compiler Internals

How elephc works under the hood — from lexing to code generation and runtime structure.

- [What is a Compiler?](internals/what-is-a-compiler.md) — the big picture of compilation
- [The Pipeline](internals/how-elephc-works.md) — from `<?php` to running binary
- [The Lexer](internals/the-lexer.md) — raw text to tokens
- [The Parser](internals/the-parser.md) — tokens to AST with Pratt parsing
- [The Type Checker](internals/the-type-checker.md) — compile-time type inference and validation
- [The Optimizer](internals/the-optimizer.md) — constant folding, constant propagation, purity / may-throw reasoning, control-flow pruning, normalization, and dead-code elimination on the AST
- [The Code Generator](internals/the-codegen.md) — optimized checked AST to target assembly (with an AArch64-focused walkthrough)
- [The Runtime](internals/the-runtime.md) — hand-written assembly routines
- [Memory Model](internals/memory-model.md) — stack frames, heap, reference counting
- [Architecture](internals/architecture.md) — module map, calling conventions
- [ARM64 Assembly](internals/arm64-assembly.md) — introduction to ARM64
- [ARM64 Instructions](internals/arm64-instructions.md) — instruction reference

For compile-time instrumentation and debug artifacts, the CLI also supports `--timings` to print per-phase compiler timings, including the optimizer phases, and `--source-map` to emit a sidecar `.map` file next to generated assembly.
