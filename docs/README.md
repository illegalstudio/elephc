---
title: "elephc Documentation"
description: "A PHP-to-native compiler producing standalone host binaries and cross-compiled iOS libraries."
sidebar:
  order: 0
---

elephc compiles PHP to native code for five supported targets without PHP, the Zend Engine, or an external VM. macOS ARM64, Linux ARM64, and Linux x86_64 are standalone-executable targets and the host triples for compiler release archives; from macOS, the compiler also cross-compiles libraries for iOS ARM64 devices and the iOS ARM64 Simulator. Ordinary source is AOT-compiled; experimental `eval()` may embed an optional interpreter bridge when a fragment requires runtime parsing. This documentation covers everything from PHP syntax support to compiler-specific extensions and internal architecture.

## Getting Started

- [Installation](getting-started/installation.md) — install and manage elephc versions with elvm, or use Homebrew, a source build, a release download, or an unsupported nightly
- [Your First Program](getting-started/your-first-program.md) — write, compile, and run your first PHP binary
- [Benchmark Suite](https://github.com/illegalstudio/elephc/blob/main/benchmarks/README.md) — compare elephc against PHP and equivalent C fixtures

## How-To

Task-oriented guides for building real programs with elephc.

- [Build a Fiber Web Server](how-to/fiber-web-server.md) — create a native HTTP server with non-blocking sockets, `poll()`, and one `Fiber` per connection

## Compiling

Everything about driving the compiler: the command-line flags and the full path from a `.php` or `.lfc` file to a native binary.

- [Compiling Overview](compiling/overview.md) — basic invocation, output naming, defaults, and a map of this section
- [The compilation pipeline](compiling/compilation-pipeline.md) — every phase from source text to binary, in order
- [CLI reference](compiling/cli-reference.md) — the complete, authoritative list of every flag, value, default, and env override
- [Targets and cross-compilation](compiling/targets.md) — the supported target matrix and `--target`
- [Native dependencies](compiling/native-dependencies.md) — declare, lock, build, cache, diagnose, and explicitly prune curated native packages with `elephc native`
- [Optimization and codegen controls](compiling/optimization.md) — `--ir-opt` (EIR identity, peephole, and dead-instruction passes), `--regalloc`, `--null-repr`
- [Output formats and diagnostics](compiling/output-and-diagnostics.md) — `--emit`, `--emit-asm`, `--emit-ir`, `--check`, `--timings`, `--source-map`, `--debug-info`, `--gc-stats`, `--heap-debug`
- [Source maps](compiling/source-maps.md) — the `--source-map` v2 JSON schema (function ranges, labels, opcode/origin-tagged mappings, inverse line index) and `--debug-info` DWARF lines
- [Linking, heap, and conditional compilation](compiling/linking-and-conditional-compilation.md) — `--link`/`-l`, `--link-path`/`-L`, `--framework`, `--heap-size`, `--define`

## PHP Syntax

Standard PHP features supported by elephc. Implemented PHP syntax is intended to match PHP behavior; known compatibility gaps are documented on the relevant reference pages and tracked in the [roadmap](../ROADMAP.md).

- [Types](php/types.md) — int, float, string, bool, array, null, mixed, callable, enum, union types, extension types, type casting
- [Built-in functions](php/builtins.md) — the generated index of every built-in: signature, availability (AOT / `eval()`), and implementation links
- [PHP compatibility](php/compatibility.md) — the generated builtin-coverage report against the pinned reference PHP version
- [Operators](php/operators.md) — arithmetic, comparison, `instanceof`, logical, bitwise, string, assignment, ternary, null coalescing, error control
- [Control Structures](php/control-structures.md) — if/else, while, for, foreach, switch, match, multi-level break/continue, try/catch/finally
- [Functions](php/functions.md) — declarations, closures, arrow functions, named arguments, variadic, spread, pass-by-reference, first-class callables, static variables
- [Eval](php/eval.md) — experimental literal-AOT and runtime PHP fragment evaluation, scope synchronization, dynamic declarations, safety, supported builtins, and limitations
- [Strings](php/strings.md) — escape sequences, interpolation, heredoc/nowdoc, 70+ built-in string functions
- [Regex](php/regex.md) — PCRE2-backed `preg_*` functions, SPL regex iterators, and the managed `pcre2` package
- [Arrays](php/arrays.md) — indexed, associative, copy-on-write, 50+ built-in array functions
- [Math](php/math.md) — abs, floor, ceil, round, trigonometry, logarithms, random, constants
- [BCMath](php/bcmath.md) — exact arbitrary-precision decimal arithmetic, scale, rounding, and errors
- [iconv](php/iconv.md) — character-set conversion, `//TRANSLIT`/`//IGNORE`, character-oriented string functions, RFC 2047 MIME headers, and the encoding trio
- [Classes](php/classes.md) — inheritance, interfaces, abstract/final classes, typed/final/static properties, static property redeclarations, constructor promotion, methods, traits, enums, magic methods
- [SPL](php/spl.md) — SPL interfaces, exceptions, autoload/introspection helpers, and runtime-backed containers
- [Namespaces](php/namespaces.md) — namespace, use, include/require/include_once/require_once, Composer/SPL autoloading, class introspection, constants, superglobals
- [System & I/O](php/system-and-io.md) — system functions, date/time, JSON, filesystem, exec, debugging
- [Streams](php/streams.md) — stream resources, wrappers, contexts, filters, sockets, TLS, process pipes
- [Sessions](php/sessions.md) — `session_start()`, `$_SESSION`, session ID and cookie management, file-based storage under `--web`
- [OPcache](php/opcache.md) — the observable Zend OPcache API over elephc's compile-time script manifest: `opcache_get_status()`/`opcache_get_configuration()`, the `opcache.*` directive matrix, `ini_get`/`ini_get_all`, `extension_loaded()`
- [Magic Constants](php/magic-constants.md) — `__DIR__`, `__FILE__`, `__LINE__`, `__FUNCTION__`, `__CLASS__`, `__METHOD__`, `__NAMESPACE__`, `__TRAIT__`
- [Fibers](php/fibers.md) — cooperative coroutines (PHP 8.1+ Fiber): start, suspend, resume, FiberError
- [Generators](php/generators.md) — `yield`, `yield from`, `Generator::send` / `throw` / `getReturn`, stackful coroutine codegen
- [PDO (Databases)](php/pdo.md) — PDO connections, prepared statements, fetch modes, transactions, and PDOException for SQLite, PostgreSQL, and MySQL/MariaDB drivers
- [mysqli (MySQL / MariaDB)](php/mysqli.md) — the mysqli subset over the same pure-Rust MySQL client: buffered results, prepared statements, multi_query, and mysqli_report error handling
- [Date and Time](php/datetime.md) — `DateTime`, `DateTimeImmutable`, `DateTimeZone`, `DateInterval`: construct, format, setters, `add`/`sub`, `diff`
- [Calendar](php/calendar.md) — `ext/calendar`: Julian Day conversions for the Gregorian, Julian, French Republican and Jewish calendars, Easter, day/month names, `cal_*` dispatch
- [Images](php/image.md) — GD image creation, I/O, color, drawing, text, transforms/filters, Exif/IPTC metadata, the Imagick (`Imagick`/`ImagickDraw`/`ImagickPixel`/`ImagickPixelIterator`/`ImagickKernel`) and Gmagick (`Gmagick`/`GmagickDraw`/`GmagickPixel`) object APIs, and Cairo 2D vector drawing (`CairoImageSurface`/`CairoContext`/`CairoMatrix`/patterns/gradients), plus `getimagesize`/`image_type_to_*`, backed by a pure-Rust codec/raster bridge (no system GD/ImageMagick/GraphicsMagick/cairo/libpng/libjpeg/libexif)
- [cURL](php/curl.md) — `ext/curl`'s complete function, class, and constant surface (easy, multi, share, `CURLFile`/`CURLStringFile` uploads, six libcurl callbacks) on a statically pinned libcurl 8.21.0 with OpenSSL 3.5.8 as its TLS backend and native Apple SecTrust verification on iOS, plus the protocol matrix, the option-rejection table, and every documented difference from PHP

## Beyond PHP

Compiler-specific extensions that go beyond standard PHP. These features have no PHP equivalent and exist to enable use cases PHP was never designed for.

- [LFC Source Files](beyond-php/lfc-source-files.md) — tagless source, mixed PHP/LFC projects, and per-file strict-mode behavior
- [Pointers](beyond-php/pointers.md) — ptr(), ptr_get(), ptr_set(), pointer arithmetic, typed casting
- [Buffers](beyond-php/buffers.md) — buffer&lt;T&gt; for fixed-size contiguous arrays, hot-path data
- [Packed Classes](beyond-php/packed-classes.md) — flat POD records with compile-time field offsets
- [FFI & Extern](beyond-php/extern.md) — calling C libraries, extern functions/globals/classes, callbacks
- [Conditional Compilation](beyond-php/ifdef.md) — ifdef blocks, compile-time feature flags, CLI flags
- [Shared Libraries (cdylib)](beyond-php/cdylib.md) — --emit cdylib, #[Export] C-ABI functions, dlopen lifecycle
- [Web Server (--web)](beyond-php/web.md) — compile a PHP file into a standalone HTTP server with worker, pool, or per-request process isolation
- [zval Bridge](beyond-php/zval-bridge.md) — zval_pack/unpack/type/free convert elephc values to/from PHP zval structs
- [Profiling](beyond-php/profiling.md) - PHP-level profiling with one command in every environment: a launched program reports exact wall time, allocations, retained objects, database and outgoing-network wait, SQL queries, network operations and calls; a running service is sampled by default and exact only for a requested completed request. Includes per-`--web`-route tags, `.elephc` performance budgets, automatic curl trace propagation, W3C distributed traces, and a self-contained interactive call-graph page

## Compiler Internals

How elephc works under the hood — from lexing to code generation and runtime structure.

- [What is a Compiler?](internals/what-is-a-compiler.md) — the big picture of compilation
- [The Pipeline](internals/how-elephc-works.md) — from `<?php` to running binary
- [The Lexer](internals/the-lexer.md) — raw text to tokens
- [The Parser](internals/the-parser.md) — tokens to AST with Pratt parsing
- [The Type Checker](internals/the-type-checker.md) — compile-time type inference and validation
- [The Optimizer](internals/the-optimizer.md) — constant folding, constant propagation, purity / may-throw reasoning, control-flow pruning, normalization, and dead-code elimination on the AST
- [The Code Generator](internals/the-codegen.md) — checked AST to EIR, then target assembly
- [The EIR Design](internals/the-ir.md) — PHP-shaped intermediate representation used by codegen and `--emit-ir`
- [The Runtime](internals/the-runtime.md) — hand-written assembly routines
- [Eval Runtime Architecture](internals/eval-runtime.md) — literal AOT planning, scope synchronization, Magician fallback, and bridge ABI
- [Memory Model](internals/memory-model.md) — stack frames, heap, reference counting
- [Architecture](internals/architecture.md) — module map, calling conventions
- [ARM64 Assembly](internals/arm64-assembly.md) — introduction to ARM64
- [ARM64 Instructions](internals/arm64-instructions.md) — instruction reference

For compile-time instrumentation and debug artifacts, the CLI also supports `--timings` to print per-phase compiler timings, including the optimizer phases, and `--source-map` to emit a sidecar `.map` file next to generated assembly.
