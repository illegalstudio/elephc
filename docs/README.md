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

## How-To

Task-oriented guides for building real programs with elephc.

- [Build a Fiber Web Server](how-to/fiber-web-server.md) — create a native HTTP server with non-blocking sockets, `poll()`, and one `Fiber` per connection

## Compiling

Everything about driving the compiler: the command-line flags and the full path from a `.php` file to a native binary.

- [Compiling Overview](compiling/overview.md) — basic invocation, output naming, defaults, and a map of this section
- [The compilation pipeline](compiling/compilation-pipeline.md) — every phase from source text to binary, in order
- [CLI reference](compiling/cli-reference.md) — the complete, authoritative list of every flag, value, default, and env override
- [Targets and cross-compilation](compiling/targets.md) — the supported target matrix and `--target`
- [Optimization and codegen controls](compiling/optimization.md) — `--ir-opt` (EIR identity, peephole, and dead-instruction passes), `--regalloc`, `--null-repr`
- [Output formats and diagnostics](compiling/output-and-diagnostics.md) — `--emit`, `--emit-asm`, `--emit-ir`, `--check`, `--timings`, `--source-map`, `--gc-stats`, `--heap-debug`
- [Linking, heap, and conditional compilation](compiling/linking-and-conditional-compilation.md) — `--link`/`-l`, `--link-path`/`-L`, `--framework`, `--heap-size`, `--define`

## PHP Syntax

Standard PHP features supported by elephc. Implemented PHP syntax is intended to match PHP behavior; known compatibility gaps are documented on the relevant reference pages and tracked in the roadmap.

- [Types](php/types.md) — int, float, string, bool, array, null, mixed, callable, enum, union types, extension types, type casting
- [Operators](php/operators.md) — arithmetic, comparison, `instanceof`, logical, bitwise, string, assignment, ternary, null coalescing, error control
- [Control Structures](php/control-structures.md) — if/else, while, for, foreach, switch, match, multi-level break/continue, try/catch/finally
- [Functions](php/functions.md) — declarations, closures, arrow functions, named arguments, variadic, spread, pass-by-reference, first-class callables, static variables
- [Strings](php/strings.md) — escape sequences, interpolation, heredoc/nowdoc, 70+ built-in string functions
- [Regex](php/regex.md) — PCRE2-backed `preg_*` functions, SPL regex iterators, and native PCRE2 build requirements
- [Arrays](php/arrays.md) — indexed, associative, copy-on-write, 50+ built-in array functions
- [Math](php/math.md) — abs, floor, ceil, round, trigonometry, logarithms, random, constants
- [Classes](php/classes.md) — inheritance, interfaces, abstract/final classes, typed/final/static properties, static property redeclarations, constructor promotion, methods, traits, enums, magic methods
- [SPL](php/spl.md) — SPL interfaces, exceptions, autoload/introspection helpers, and runtime-backed containers
- [Namespaces](php/namespaces.md) — namespace, use, include/require/include_once/require_once, Composer/SPL autoloading, class introspection, constants, superglobals
- [System & I/O](php/system-and-io.md) — system functions, date/time, JSON, filesystem, exec, debugging
- [Streams](php/streams.md) — stream resources, wrappers, contexts, filters, sockets, TLS, process pipes
- [Magic Constants](php/magic-constants.md) — `__DIR__`, `__FILE__`, `__LINE__`, `__FUNCTION__`, `__CLASS__`, `__METHOD__`, `__NAMESPACE__`, `__TRAIT__`
- [Fibers](php/fibers.md) — cooperative coroutines (PHP 8.1+ Fiber): start, suspend, resume, FiberError
- [Generators](php/generators.md) — `yield`, `yield from`, `Generator::send` / `throw` / `getReturn`, state-machine codegen
- [PDO (Databases)](php/pdo.md) — PDO connections, prepared statements, fetch modes, transactions, and PDOException for SQLite, PostgreSQL, and MySQL/MariaDB drivers
- [Date and Time](php/datetime.md) — `DateTime`, `DateTimeImmutable`, `DateTimeZone`, `DateInterval`: construct, format, setters, `add`/`sub`, `diff`
- [Calendar](php/calendar.md) — `ext/calendar`: Julian Day conversions for the Gregorian, Julian, French Republican and Jewish calendars, Easter, day/month names, `cal_*` dispatch
- [Images](php/image.md) — GD image creation, I/O, color, drawing, text, transforms/filters, Exif/IPTC metadata, the Imagick (`Imagick`/`ImagickDraw`/`ImagickPixel`/`ImagickPixelIterator`/`ImagickKernel`) and Gmagick (`Gmagick`/`GmagickDraw`/`GmagickPixel`) object APIs, and Cairo 2D vector drawing (`CairoImageSurface`/`CairoContext`/`CairoMatrix`/patterns/gradients), plus `getimagesize`/`image_type_to_*`, backed by a pure-Rust codec/raster bridge (no system GD/ImageMagick/GraphicsMagick/cairo/libpng/libjpeg/libexif)

## Beyond PHP

Compiler-specific extensions that go beyond standard PHP. These features have no PHP equivalent and exist to enable use cases PHP was never designed for.

- [Pointers](beyond-php/pointers.md) — ptr(), ptr_get(), ptr_set(), pointer arithmetic, typed casting
- [Buffers](beyond-php/buffers.md) — buffer&lt;T&gt; for fixed-size contiguous arrays, hot-path data
- [Packed Classes](beyond-php/packed-classes.md) — flat POD records with compile-time field offsets
- [FFI & Extern](beyond-php/extern.md) — calling C libraries, extern functions/globals/classes, callbacks
- [Conditional Compilation](beyond-php/ifdef.md) — ifdef blocks, compile-time feature flags, CLI flags
- [Shared Libraries (cdylib)](beyond-php/cdylib.md) — --emit cdylib, #[Export] C-ABI functions, dlopen lifecycle

## Compiler Internals

How elephc works under the hood — from lexing to code generation and runtime structure.

- [What is a Compiler?](internals/what-is-a-compiler.md) — the big picture of compilation
- [The Pipeline](internals/how-elephc-works.md) — from `<?php` to running binary
- [The Lexer](internals/the-lexer.md) — raw text to tokens
- [The Parser](internals/the-parser.md) — tokens to AST with Pratt parsing
- [The Type Checker](internals/the-type-checker.md) — compile-time type inference and validation
- [The Optimizer](internals/the-optimizer.md) — constant folding, constant propagation, purity / may-throw reasoning, control-flow pruning, normalization, and dead-code elimination on the AST
- [The Code Generator](internals/the-codegen.md) — checked AST to EIR, then target assembly through the default backend
- [The EIR Design](internals/the-ir.md) — PHP-shaped intermediate representation used by the default backend and `--emit-ir`
- [The Runtime](internals/the-runtime.md) — hand-written assembly routines
- [Memory Model](internals/memory-model.md) — stack frames, heap, reference counting
- [Architecture](internals/architecture.md) — module map, calling conventions
- [ARM64 Assembly](internals/arm64-assembly.md) — introduction to ARM64
- [ARM64 Instructions](internals/arm64-instructions.md) — instruction reference

For compile-time instrumentation and debug artifacts, the CLI also supports `--timings` to print per-phase compiler timings, including the optimizer phases, and `--source-map` to emit a sidecar `.map` file next to generated assembly.
