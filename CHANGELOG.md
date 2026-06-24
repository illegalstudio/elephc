# Changelog

All notable changes to elephc, a PHP-to-native compiler written in Rust.
Releases are listed newest first.

## [Unreleased]
- EIR small-function inliner: splices small (≤24-instruction), non-recursive user functions into their callers — covering scalar, string, and array/value helpers — with copy-on-write and reference-counting semantics preserved, gated by `--ir-opt`. Recursive (direct or mutual), generator/fiber, exception-handling, object/closure/resource/by-reference, and argument-coercing call sites are left as ordinary calls.
- EIR optimization pipeline now runs to a module-level fixed point: the small-function inliner and the per-function passes are interleaved and repeated until neither changes anything, so optimization and inlining feed each other (e.g. a function inlined once its callees fold below the size threshold). Behavior is unchanged with `--ir-opt` on vs off; only the generated code gets tighter.
- EIR common-subexpression elimination now unifies constant operands by value, so repeated computations built on constants are deduplicated too — `($n + 1) * ($n + 1)` computes `$n + 1` once. Gated by `--ir-opt`; output is unchanged, only the generated code is tighter.
- Fixed an EIR optimizer miscompile: `$x / 1` (PHP's float-producing division) was folded to its integer operand, so the result's bits were misread as a float — `var_dump($x / 1)` and float arithmetic on it produced wrong values with the optimizer on. Identity folding now only folds genuine integer division.

## [0.25.1] - 2026-06-22
- Image support (EIR backend), pure-Rust with no runtime dependency (`elephc-image` crate): GD raster I/O (PNG/JPEG/GIF/BMP/WebP/TGA), drawing primitives, bitmap text, transforms/filters/copy, and color handling; Exif and IPTC metadata; Imagick and Gmagick OOP with the full method surface callable (operations with no pure-Rust equivalent throw `*Exception("... is not supported in elephc")` at runtime, matching PHP); and a Cairo (tiny-skia) procedural + OOP subset.
- Loose equality (`==`/`!=`) and `switch` compare a float against an int numerically (`1.5 == 1` is `false`, `1.0 == 1` is `true`, `switch (1.5)` matches `case 1.5`) instead of failing to compile or truncating the float subject to int.
- EIR dead store elimination over PHP local slots: a CFG-liveness pass that drops `store_local` writes to scalar locals that are never read before being overwritten or the function exits, gated by `--ir-opt`. Refcounted and by-reference-aliased slots are left untouched to preserve ownership and aliasing semantics.
- EIR branch simplification: folds constant-condition `cond_br`/`switch` terminators to unconditional branches (e.g. `while (true)` loops), threads predecessors through empty forwarding blocks, and neutralizes unreachable blocks, gated by `--ir-opt`.
- EIR per-block constant folding: folds pure operations whose operands are all compile-time constants (integer/float arithmetic and bitwise ops, in-range shifts, comparisons, `is_null`/`is_truthy`) into a single constant, gated by `--ir-opt`. Composed with the peephole's scalar load/store forwarding, it propagates constants through EIR value ids and local slots.
- EIR dominator-tree and natural-loop analyses: read-only sidecar analyses (Cooper–Harvey–Kennedy dominators; a back-edge/natural-loop forest with nesting and preheader detection) that underpin the cross-block optimizations below.
- EIR common-subexpression elimination: a dominator-tree value-numbering pass that removes a pure computation when an identical one already dominates it (per-block and cross-block), gated by `--ir-opt`.
- EIR loop-invariant code motion: hoists pure loop-invariant computations out of loop bodies into loop preheaders, gated by `--ir-opt`.

## [0.25.0] - 2026-06-19
- EIR dead instruction elimination over CFG liveness, registered after identity and peephole passes and gated by `--ir-opt`.
- PHP date/time, timezone, and calendar parity (EIR backend): `DateTime`, `DateTimeImmutable`, `DateTimeInterface`, `DateTimeZone`, `DateInterval`, and `DatePeriod`, the PHP 8.3 date exception hierarchy, plus the procedural `getdate`/`localtime`/`mktime`/`gmmktime`/`checkdate`/`microtime`/`hrtime`/`strtotime`, `date`/`gmdate` format tokens, solar functions (`date_sun_info`/`date_sunrise`/`date_sunset`), and `ext/calendar` functions — backed by a bundled IANA timezone database (`elephc-tz` crate).
- Date/time correctness fixes: `getdate()`/`localtime()` default to UTC like PHP, `gmdate("T")` reports `GMT` on every target, `createFromTimestamp()` keeps fractional-second microseconds, `DateInterval` requires a leading `P`, `diff()` and `DatePeriod::createFromISO8601String()` match PHP signatures (`$targetObject`, `$specification` + `$options`).
- General fixes surfaced by the date/time work: `var_dump` renders heterogeneous indexed-array bodies, `array_fill` terminates on a negative count on ARM64, and user-declared functions/classes are no longer hijacked when their name collides with a procedural date alias.

## [0.24.3] - 2026-06-17
- EIR peephole optimization pass: box/unbox cancellation, scalar load/store forwarding, paired acquire/release cancellation, string-literal concat folding, and redundant move/borrow cleanup.

## [0.24.2] - 2026-06-17
- Add a fixed-point EIR optimization pass driver with identity arithmetic folding.
- Document the pass driver and add the Compiling docs section.

## [0.24.1] - 2026-06-16
- Register allocator: caller-saved reuse and use-weighted spilling.
- Add benchmark time-series history and dashboard.

## [0.24.0] - 2026-06-16
- Linear-scan register allocator for the EIR backend (liveness, live intervals, int/float pools, spilling) behind `--regalloc`.

## [0.23.14] - 2026-06-16
- Fix empty-array string-grow corruption in the EIR backend.

## [0.23.13] - 2026-06-15
- Property hooks (get/set bodies), anonymous classes, intersection types (`A&B`), asymmetric visibility (`private(set)`).
- Enum methods/constants/implements, dynamic method and static calls, expression-position require/include, typed variadic parameters.

## [0.23.12] - 2026-06-15
- PDO: persistent connection pooling, class fetch modes, binary columns.
- Dynamic property reads/writes, including on Mixed values.

## [0.23.11] - 2026-06-13
- Phar streams: tar/zip/bzip2 read & write, OOP iteration, ArrayAccess, metadata/stubs, compression controls.
- Symlink ownership builtins and user stream filter params.

## [0.23.10] - 2026-06-12
- EIR is now the only active codegen backend. The legacy AST backend is frozen: `--ast-backend` is retained solely as a diagnostic fallback and is not a feature or parity target.
- Broad EIR parity hardening across arrays, mixed values, callables, fibers, SPL, generators, streams, and ownership/GC paths.

## [0.23.9] - 2026-06-11
- `--emit cdylib` produces loadable shared libraries (PHP → C dlopen).
- Tagged null representation as the default scalar/null encoding; PIC global access via GOT.

## [0.23.8] - 2026-06-09
- elephc-crypto crate: full `hash()` family, HMAC, incremental HashContext, checksums, timing-safe `hash_equals`; phar SHA1 signing migrated.
- Lexer/runtime correctness: UTF-8 identifiers, heredoc closers, string interpolation, string-to-int precision, Mixed-arg coercions.

## [0.23.7] - 2026-06-07
- Flow-sensitive type narrowing for `is_*`/`instanceof` guards and `if`/`elseif`/`never` divergence.
- Infer union types for untyped params called with heterogeneous arguments.

## [0.23.6] - 2026-06-05
- PDO: SQLite, PostgreSQL, and MySQL/MariaDB drivers via a driver-agnostic bridge; binding, fetch modes, attributes, `quote()`, Traversable statements.
- `__destruct` object destructor support.

## [0.23.5] - 2026-06-04
- Stream URL reads: http/https/ftp/ftps in `file_get_contents()`.
- `$length`/`$offset` for `stream_get_contents()` and `stream_copy_to_stream()`; real TLS teardown.

## [0.23.4] - 2026-06-03
- Streams subsystem: core resource model, sockets (TCP/UDP/Unix, IPv6, DNS), TLS, wrappers (data/http/ftp + zlib/bzip2), filters, contexts, userspace wrappers, phar read/write.

## [0.23.3] - 2026-06-02
- Generator fixes around yield-from echoes and send values; pipe callable ownership; string return persistence.

## [0.23.2] - 2026-06-01
- New builtins: `clamp`, `grapheme_strrev`, `SortDirection` enum, `json_decode` error locations, `array_filter` mode constants.
- Many PHP-parity fixes: finally semantics, float array keys, enum `from` errors, arrow-fn captures, parser edge cases.

## [0.23.1] - 2026-05-29
- PCRE2-backed SPL regex and filesystem iterators; PCRE2 regex requirements documented.

## [0.23.0] - 2026-05-28
- Phase 6 SPL containers.

## [0.22.5] - 2026-05-28
- Universal callable descriptors: unified runtime dispatch for string/array/closure/method callables across call_user_func, array_map/filter/reduce, sort, iterators, fibers, and externs.
- SPL iterator family expansion (recursive, caching, filter, multi-source) and stateful FFI callback trampolines.

## [0.22.4] - 2026-05-21
- Async HTTP server showcase; raw pointer memory builtins.
- Ownership/lifetime hotfixes across fibers, concat, by-ref foreach, and externs.

## [0.22.3] - 2026-05-19
- Reject by-reference foreach over iterators; isolate by-ref fallback flags.

## [0.22.2] - 2026-05-19
- Nested/mixed array assignment fixes, foreach reference aliasing, dynamic spreads after named args, built-in `Error` type.

## [0.22.1] - 2026-05-18
- Many parity fixes: by-reference foreach values, by-ref closure captures, multi-argument `isset`/`unset`/`echo`, unicode regex escapes, `preg_replace_callback` captures, PHP string escapes.

## [0.22.0] - 2026-05-18
- `ArrayAccess` subscript syntax.

## [0.21.16] - 2026-05-16
- OOP property parity v2; avoid clobbering by-ref scalar slots.

## [0.21.15] - 2026-05-16
- Runtime compatibility v2: loose comparisons, integer overflow promotion, uninitialized typed property reads, mixed value coercions.

## [0.21.14] - 2026-05-16
- Case-insensitive user function string lookup and callable fallbacks.

## [0.21.13] - 2026-05-16
- `is_callable` runtime fallback.

## [0.21.12] - 2026-05-16
- `preg_replace` backreferences and `strtotime` article offsets.

## [0.21.11] - 2026-05-16
- Callable parity follow-up: captured callable forwarding and signature validation.

## [0.21.10] - 2026-05-16
- JSON performance: inline pretty-print, fused decode validation, list-shape encoder optimization.

## [0.21.9] - 2026-05-15
- Abstract properties (redeclaration, readonly-static); expanded `strtotime` relative formats; macOS time/localtime crash fix.

## [0.21.8] - 2026-05-14
- SPL foundation: autoloader and core SPL infrastructure; hardened class introspection builtins.

## [0.21.7] - 2026-05-14
- Runtime attribute reflection: metadata tracking and emission.

## [0.21.6] - 2026-05-13
- PHP 8.5 pipe operator (`|>`) with first-class-callable short-circuit optimizations.

## [0.21.5] - 2026-05-13
- Filesystem builtins: symbolic-link (Phase 5) and stream-extension (Phase 4).

## [0.21.4] - 2026-05-13
- v0.21 JSON parity; `ReflectionAttribute` and class attribute reflection builtins.

## [0.21.3] - 2026-05-12
- Mixed array union support.

## [0.21.2] - 2026-05-12
- PHP generators: `yield`, `send`, `throw`, `yield from`, `getReturn`; complete backend support.

## [0.21.1] - 2026-05-11
- Parse PHP attributes; observe `Override`/`Deprecated`.

## [0.21.0] - 2026-05-11
- Heterogeneous indexed arrays (boxed/widened element types).
- Mandatory Rust module preambles; large-scale module split across compiler, runtime, and tests.

## [0.20.12] - 2026-05-09
- Method first-class callables; forward captured method callables in callbacks.

## [0.20.11] - 2026-05-09
- Forward captured closures through callbacks; string array callbacks.

## [0.20.10] - 2026-05-08
- PHP Fibers: context switch, captures, exception propagation; Linux x86_64 support.

## [0.20.9] - 2026-05-07
- Named arguments for builtins and externs; shared call-argument planning and spread normalization.

## [0.20.8] - 2026-05-06
- Full list destructuring.

## [0.20.7] - 2026-05-05
- Dynamic `instanceof` targets.

## [0.20.6] - 2026-05-05
- Mixed nullsafe chains.

## [0.20.5] - 2026-05-05
- Path-sensitive include graph declaration discovery and conditional include function variants.
- Large module split across resolver, type checker, lexer, and codegen.

## [0.20.4] - 2026-05-04
- Runtime include-once guards.

## [0.20.3] - 2026-05-04
- Reject dynamic include paths.

## [0.20.2] - 2026-05-04
- Model PHP stream resources; `fopen` failure parity.

## [0.20.1] - 2026-05-03
- Dynamic `pathinfo` flags.

## [0.20.0] - 2026-05-03
- `fnmatch` flag support.

## [0.19.14] - 2026-05-03
- Filesystem coverage: path manipulation (Phase 1), extended stat (Phase 2), modification builtins (Phase 3).
- PHP case-insensitive symbol lookup; built-in `Iterator`/`IteratorAggregate` with foreach.

## [0.19.13] - 2026-05-01
- `iterable` type, assignment expressions, multilevel break/continue, closure return types, `print` expression.

## [0.19.12] - 2026-04-29
- `never` and literal types, nullsafe operator, `instanceof`, magic constants in includes, short ternary, word-form logical operators, PHP array unions, error-control parity.

## [0.19.11] - 2026-04-27
- Typed/static/promoted/constructor-promoted properties, null-coalescing assignment, compound assignment operators, `php_uname`/`PHP_OS`.

## [0.19.10] - 2026-04-24
- `final` OOP members; benchmark suite machine-readable output and CI integration.

## [0.19.9] - 2026-04-24
- Constant propagation v3: propagate constants through known loop exits.

## [0.19.8] - 2026-04-23
- Constant folding of array/assoc-array literal access and known match expressions.

## [0.19.7] - 2026-04-23
- DCE v3: CFG-lite path analysis and guard inference to prune impossible switch/if branches.

## [0.19.6] - 2026-04-22
- DCE v2: scalar guard tracking, shadowed-arm dropping, try/switch path unification; optimizer split into modules.

## [0.19.5] - 2026-04-21
- Control-flow normalization pass: canonicalize elseif chains, merge identical branches/catches, normalize switches.

## [0.19.4] - 2026-04-20
- Local constant propagation pass with loop/branch/try-aware invalidation.

## [0.19.3] - 2026-04-20
- Purity and may-throw effect analysis for functions, methods, and closures.

## [0.19.2] - 2026-04-18
- Dead code elimination pass; control-flow simplification.

## [0.19.1] - 2026-04-18
- Constant folding pass and constant control-flow pruning; pure-subexpression pruning.

## [0.19.0] - 2026-04-17
- Compiler tooling: source maps, benchmark harness, timing output, runtime object caching.

## [0.18.5] - 2026-04-17
- Preserve `require_once` error locations.

## [0.18.4] - 2026-04-17
- By-ref array method fix; sync `Cargo.lock` in release workflow.

## [0.18.3] - 2026-04-17
- Specialize generic array hints.

## [0.18.2] - 2026-04-17
- Deep mixed postfix chains; property indexed/array-push assignments.

## [0.18.1] - 2026-04-17
- CLI `check` and `emit-asm` modes.

## [0.18.0] - 2026-04-16
- Linux support: full port to Linux x86_64 and Linux ARM64 with target-aware ABI.
- DOOM showcase; target model plumbing and ABI centralization; large codegen modularization.

## [0.17.9] - 2026-04-03
- Docs restructured into Astro-compatible sections; CI auto-sets `Cargo.toml` version from tag.
- DOOM showcase rendering work (BSP, sectors, fog, doors, HUD).

## [0.17.8] - 2026-04-02
- Error recovery and warnings.

## [0.17.7] - 2026-04-02
- Named arguments.

## [0.17.6] - 2026-04-02
- Type annotations on function params and return types, with enforcement.

## [0.17.5] - 2026-04-01
- Enum support; object-typed locals.

## [0.17.4] - 2026-04-01
- Union and nullable typed locals.

## [0.17.3] - 2026-04-01
- Split generated assembly into user + runtime objects with global runtime labels; cache pre-assembled runtime in test harness.

## [0.17.2] - 2026-04-01
- `readonly` classes, first-class callables, match fatal path, variadic methods/callables.

## [0.17.1] - 2026-04-01
- Packed buffers for hot-path data; manual `buffer_free`.

## [0.17.0] - 2026-03-31
- Namespace resolution across the compiler pipeline.

## [0.16.8] - 2026-03-31
- Build-time `ifdef` conditional compilation.

## [0.16.7] - 2026-03-31
- Magic methods for string conversion and dynamic properties.

## [0.16.6] - 2026-03-31
- String indexing syntax.

## [0.16.5] - 2026-03-31
- Exception handling: throw/catch (incl. multi-catch and variable-less), built-in `Exception`/`Throwable`.

## [0.16.4] - 2026-03-30
- Mixed-type associative arrays via runtime element tags.

## [0.16.3] - 2026-03-30
- Preserve associative array insertion order.

## [0.16.2] - 2026-03-30
- Interfaces and abstract class checks; common-parent object arrays; codegen modularization.

## [0.16.1] - 2026-03-30
- Single inheritance dispatch, late static binding, `self` receiver.

## [0.16.0] - 2026-03-30
- Inheritance groundwork release.

## [0.15.3] - 2026-03-30
- PHP-like trait composition.

## [0.15.2] - 2026-03-30
- Copy-on-write arrays.

## [0.15.1] - 2026-03-30
- Targeted cycle collection; small-bin heap size classes; call-scoped extern string args.

## [0.15.0] - 2026-03-29
- Heap ownership lattice, heap debug verification, free-block coalescing/splitting; cycle collection strategy.

## [0.14.0] - 2026-03-28
- FFI: `extern` keyword, C types, callbacks, extern globals, linker flags; SDL2 examples.

## [0.13.0] - 2026-03-28
- Pointer type, `ptr_cast<T>()`, pointer builtins and runtime.

## [0.12.1] - 2026-03-27
- Class visibility and checker coverage.

## [0.12.0] - 2026-03-27
- Trigonometry, logarithms, math constants; extensive type-inference fixes.

## [0.11.0] - 2026-03-27
- Reference-counting garbage collector with scope-based local cleanup.

## [0.10.0] - 2026-03-27
- Basic PHP classes: properties, constructors, methods, static members.

## [0.9.1] - 2026-03-26
- Bump-reset optimization and string dedup.

## [0.9.0] - 2026-03-26
- Free-list heap allocator with bounds checking, copy-on-store strings, dynamic array/hash growth, `--heap-size` flag.

## [0.8.2] - 2026-03-26
- `use ($var)` closure captures; `round()` precision, char-mask trims, variadic `min`/`max`; many correctness fixes.

## [0.8.1] - 2026-03-26
- Large batch of correctness fixes closing ~30 issues (cross-type `==`, `sprintf` modifiers, IIFE, spread into named params, heredoc interpolation, and more).

## [0.8.0] - 2026-03-25
- Date/time, JSON, and regex functions.

## [0.7.2] - 2026-03-25
- Global/static variables, pass by reference, variadic functions, spread operator; v0.8 system functions.

## [0.7.1] - 2026-03-25
- `define()`/`const` constants, list unpacking, `call_user_func_array`; CI and release workflow.

## [0.7.0] - 2026-03-24
- Default params, null coalescing, bitwise operators, spaceship, heredoc/nowdoc.

## [0.6.0] - 2026-03-23
- Associative arrays, `switch`/`match`, multi-dimensional arrays, ~30 array functions.
- Anonymous and arrow functions; callback functions (`array_map`/`filter`/`reduce`, `usort`, etc.); I/O and filesystem.

## [0.4.0] - 2026-03-23
- String functions: `substr`, `strpos`, `explode`/`implode`, `sprintf`/`printf`, string interpolation, HTML/URL/base64/ctype helpers, `hash`, `sscanf`.

## [0.3.0] - 2026-03-22
- Float type with FP registers and math builtins, proper Bool/null types, type casting, `===`/`!==`, `include`/`require`, `**`, constants.

## [0.2.0] - 2026-03-22
- Indexed arrays with heap allocator, `foreach`, array functions; ternary, do-while, `$argc`/`$argv`, `strlen`/`intval`.

## [0.1.0] - 2026-03-22
- Initial compiler: echo, variables, integers, arithmetic and string concatenation, comparison operators, control flow (`if`/`while`/`for`/`break`/`continue`), functions, logical/assignment/increment operators.

[Unreleased]: https://github.com/illegalstudio/elephc/compare/v0.25.1...HEAD
[0.25.1]: https://github.com/illegalstudio/elephc/compare/v0.25.0...v0.25.1
[0.25.0]: https://github.com/illegalstudio/elephc/compare/v0.24.3...v0.25.0
[0.24.3]: https://github.com/illegalstudio/elephc/compare/v0.24.2...v0.24.3
[0.24.2]: https://github.com/illegalstudio/elephc/compare/v0.24.1...v0.24.2
[0.24.1]: https://github.com/illegalstudio/elephc/compare/v0.24.0...v0.24.1
[0.24.0]: https://github.com/illegalstudio/elephc/compare/v0.23.14...v0.24.0
[0.23.14]: https://github.com/illegalstudio/elephc/compare/v0.23.13...v0.23.14
[0.23.13]: https://github.com/illegalstudio/elephc/compare/v0.23.12...v0.23.13
[0.23.12]: https://github.com/illegalstudio/elephc/compare/v0.23.11...v0.23.12
[0.23.11]: https://github.com/illegalstudio/elephc/compare/v0.23.10...v0.23.11
[0.23.10]: https://github.com/illegalstudio/elephc/compare/v0.23.9...v0.23.10
[0.23.9]: https://github.com/illegalstudio/elephc/compare/v0.23.8...v0.23.9
[0.23.8]: https://github.com/illegalstudio/elephc/compare/v0.23.7...v0.23.8
[0.23.7]: https://github.com/illegalstudio/elephc/compare/v0.23.6...v0.23.7
[0.23.6]: https://github.com/illegalstudio/elephc/compare/v0.23.5...v0.23.6
[0.23.5]: https://github.com/illegalstudio/elephc/compare/v0.23.4...v0.23.5
[0.23.4]: https://github.com/illegalstudio/elephc/compare/v0.23.3...v0.23.4
[0.23.3]: https://github.com/illegalstudio/elephc/compare/v0.23.2...v0.23.3
[0.23.2]: https://github.com/illegalstudio/elephc/compare/v0.23.1...v0.23.2
[0.23.1]: https://github.com/illegalstudio/elephc/compare/v0.23.0...v0.23.1
[0.23.0]: https://github.com/illegalstudio/elephc/compare/v0.22.5...v0.23.0
[0.22.5]: https://github.com/illegalstudio/elephc/compare/v0.22.4...v0.22.5
[0.22.4]: https://github.com/illegalstudio/elephc/compare/v0.22.3...v0.22.4
[0.22.3]: https://github.com/illegalstudio/elephc/compare/v0.22.2...v0.22.3
[0.22.2]: https://github.com/illegalstudio/elephc/compare/v0.22.1...v0.22.2
[0.22.1]: https://github.com/illegalstudio/elephc/compare/v0.22.0...v0.22.1
[0.22.0]: https://github.com/illegalstudio/elephc/compare/v0.21.16...v0.22.0
[0.21.16]: https://github.com/illegalstudio/elephc/compare/v0.21.15...v0.21.16
[0.21.15]: https://github.com/illegalstudio/elephc/compare/v0.21.14...v0.21.15
[0.21.14]: https://github.com/illegalstudio/elephc/compare/v0.21.13...v0.21.14
[0.21.13]: https://github.com/illegalstudio/elephc/compare/v0.21.12...v0.21.13
[0.21.12]: https://github.com/illegalstudio/elephc/compare/v0.21.11...v0.21.12
[0.21.11]: https://github.com/illegalstudio/elephc/compare/v0.21.10...v0.21.11
[0.21.10]: https://github.com/illegalstudio/elephc/compare/v0.21.9...v0.21.10
[0.21.9]: https://github.com/illegalstudio/elephc/compare/v0.21.8...v0.21.9
[0.21.8]: https://github.com/illegalstudio/elephc/compare/v0.21.7...v0.21.8
[0.21.7]: https://github.com/illegalstudio/elephc/compare/v0.21.6...v0.21.7
[0.21.6]: https://github.com/illegalstudio/elephc/compare/v0.21.5...v0.21.6
[0.21.5]: https://github.com/illegalstudio/elephc/compare/v0.21.4...v0.21.5
[0.21.4]: https://github.com/illegalstudio/elephc/compare/v0.21.3...v0.21.4
[0.21.3]: https://github.com/illegalstudio/elephc/compare/v0.21.2...v0.21.3
[0.21.2]: https://github.com/illegalstudio/elephc/compare/v0.21.1...v0.21.2
[0.21.1]: https://github.com/illegalstudio/elephc/compare/v0.21.0...v0.21.1
[0.21.0]: https://github.com/illegalstudio/elephc/compare/v0.20.12...v0.21.0
[0.20.12]: https://github.com/illegalstudio/elephc/compare/v0.20.11...v0.20.12
[0.20.11]: https://github.com/illegalstudio/elephc/compare/v0.20.10...v0.20.11
[0.20.10]: https://github.com/illegalstudio/elephc/compare/v0.20.9...v0.20.10
[0.20.9]: https://github.com/illegalstudio/elephc/compare/v0.20.8...v0.20.9
[0.20.8]: https://github.com/illegalstudio/elephc/compare/v0.20.7...v0.20.8
[0.20.7]: https://github.com/illegalstudio/elephc/compare/v0.20.6...v0.20.7
[0.20.6]: https://github.com/illegalstudio/elephc/compare/v0.20.5...v0.20.6
[0.20.5]: https://github.com/illegalstudio/elephc/compare/v0.20.4...v0.20.5
[0.20.4]: https://github.com/illegalstudio/elephc/compare/v0.20.3...v0.20.4
[0.20.3]: https://github.com/illegalstudio/elephc/compare/v0.20.2...v0.20.3
[0.20.2]: https://github.com/illegalstudio/elephc/compare/v0.20.1...v0.20.2
[0.20.1]: https://github.com/illegalstudio/elephc/compare/v0.20.0...v0.20.1
[0.20.0]: https://github.com/illegalstudio/elephc/compare/v0.19.14...v0.20.0
[0.19.14]: https://github.com/illegalstudio/elephc/compare/v0.19.13...v0.19.14
[0.19.13]: https://github.com/illegalstudio/elephc/compare/v0.19.12...v0.19.13
[0.19.12]: https://github.com/illegalstudio/elephc/compare/v0.19.11...v0.19.12
[0.19.11]: https://github.com/illegalstudio/elephc/compare/v0.19.10...v0.19.11
[0.19.10]: https://github.com/illegalstudio/elephc/compare/v0.19.9...v0.19.10
[0.19.9]: https://github.com/illegalstudio/elephc/compare/v0.19.8...v0.19.9
[0.19.8]: https://github.com/illegalstudio/elephc/compare/v0.19.7...v0.19.8
[0.19.7]: https://github.com/illegalstudio/elephc/compare/v0.19.6...v0.19.7
[0.19.6]: https://github.com/illegalstudio/elephc/compare/v0.19.5...v0.19.6
[0.19.5]: https://github.com/illegalstudio/elephc/compare/v0.19.4...v0.19.5
[0.19.4]: https://github.com/illegalstudio/elephc/compare/v0.19.3...v0.19.4
[0.19.3]: https://github.com/illegalstudio/elephc/compare/v0.19.2...v0.19.3
[0.19.2]: https://github.com/illegalstudio/elephc/compare/v0.19.1...v0.19.2
[0.19.1]: https://github.com/illegalstudio/elephc/compare/v0.19.0...v0.19.1
[0.19.0]: https://github.com/illegalstudio/elephc/compare/v0.18.5...v0.19.0
[0.18.5]: https://github.com/illegalstudio/elephc/compare/v0.18.4...v0.18.5
[0.18.4]: https://github.com/illegalstudio/elephc/compare/v0.18.3...v0.18.4
[0.18.3]: https://github.com/illegalstudio/elephc/compare/v0.18.2...v0.18.3
[0.18.2]: https://github.com/illegalstudio/elephc/compare/v0.18.1...v0.18.2
[0.18.1]: https://github.com/illegalstudio/elephc/compare/v0.18.0...v0.18.1
[0.18.0]: https://github.com/illegalstudio/elephc/compare/v0.17.9...v0.18.0
[0.17.9]: https://github.com/illegalstudio/elephc/compare/v0.17.8...v0.17.9
[0.17.8]: https://github.com/illegalstudio/elephc/compare/v0.17.7...v0.17.8
[0.17.7]: https://github.com/illegalstudio/elephc/compare/v0.17.6...v0.17.7
[0.17.6]: https://github.com/illegalstudio/elephc/compare/v0.17.5...v0.17.6
[0.17.5]: https://github.com/illegalstudio/elephc/compare/v0.17.4...v0.17.5
[0.17.4]: https://github.com/illegalstudio/elephc/compare/v0.17.3...v0.17.4
[0.17.3]: https://github.com/illegalstudio/elephc/compare/v0.17.2...v0.17.3
[0.17.2]: https://github.com/illegalstudio/elephc/compare/v0.17.1...v0.17.2
[0.17.1]: https://github.com/illegalstudio/elephc/compare/v0.17.0...v0.17.1
[0.17.0]: https://github.com/illegalstudio/elephc/compare/v0.16.8...v0.17.0
[0.16.8]: https://github.com/illegalstudio/elephc/compare/v0.16.7...v0.16.8
[0.16.7]: https://github.com/illegalstudio/elephc/compare/v0.16.6...v0.16.7
[0.16.6]: https://github.com/illegalstudio/elephc/compare/v0.16.5...v0.16.6
[0.16.5]: https://github.com/illegalstudio/elephc/compare/v0.16.4...v0.16.5
[0.16.4]: https://github.com/illegalstudio/elephc/compare/v0.16.3...v0.16.4
[0.16.3]: https://github.com/illegalstudio/elephc/compare/v0.16.2...v0.16.3
[0.16.2]: https://github.com/illegalstudio/elephc/compare/v0.16.1...v0.16.2
[0.16.1]: https://github.com/illegalstudio/elephc/compare/v0.16.0...v0.16.1
[0.16.0]: https://github.com/illegalstudio/elephc/compare/v0.15.3...v0.16.0
[0.15.3]: https://github.com/illegalstudio/elephc/compare/v0.15.2...v0.15.3
[0.15.2]: https://github.com/illegalstudio/elephc/compare/v0.15.1...v0.15.2
[0.15.1]: https://github.com/illegalstudio/elephc/compare/v0.15.0...v0.15.1
[0.15.0]: https://github.com/illegalstudio/elephc/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/illegalstudio/elephc/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/illegalstudio/elephc/compare/v0.12.1...v0.13.0
[0.12.1]: https://github.com/illegalstudio/elephc/compare/v0.12.0...v0.12.1
[0.12.0]: https://github.com/illegalstudio/elephc/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/illegalstudio/elephc/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/illegalstudio/elephc/compare/v0.9.1...v0.10.0
[0.9.1]: https://github.com/illegalstudio/elephc/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/illegalstudio/elephc/compare/v0.8.2...v0.9.0
[0.8.2]: https://github.com/illegalstudio/elephc/compare/v0.8.1...v0.8.2
[0.8.1]: https://github.com/illegalstudio/elephc/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/illegalstudio/elephc/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/illegalstudio/elephc/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/illegalstudio/elephc/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/illegalstudio/elephc/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/illegalstudio/elephc/compare/v0.4.0...v0.6.0
[0.4.0]: https://github.com/illegalstudio/elephc/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/illegalstudio/elephc/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/illegalstudio/elephc/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/illegalstudio/elephc/releases/tag/v0.1.0
