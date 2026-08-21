---
title: "Linking, heap, and conditional compilation"
description: "Linking native libraries and frameworks for FFI, sizing the runtime heap, and defining compile-time symbols for ifdef branches."
sidebar:
  order: 8
---

These flags control how the binary is linked, how much heap the program gets, and
which compile-time branches are taken.

## Linking native libraries

When a program calls into C libraries through
[extern/FFI](../beyond-php/extern.md), those libraries must be linked into the
binary. Raw link flags, managed native packages, Composer source, Rust bridge
crates, runtime capabilities, and toolchains are distinct mechanisms:

| Mechanism | Use it for | Do not use it for |
|---|---|---|
| `elephc native` | Reviewed runtime/builtin-oriented C packages with exact source, lock, recipe, and cached static outputs | Arbitrary FFI libraries, PHP packages, Rust crates, or tool installation |
| Composer/autoload | PHP source dependencies resolved ahead of time | C archives or Rust bridges |
| Auto-detected bridge / `--with-NAME` | Optional Elephc Rust `staticlib` implementations and explicit runtime capabilities | Catalogued C sources themselves |
| User/OS toolchain | `cc`, `ar`, `ranlib`, assembler, linker, Make, SDK, and cross tools | Project dependency locking |

Raw `extern` linking is a separate user-supplied workflow layered onto the
linker. In particular, DOOM and the SDL examples require a user-installed SDL
plus `extern` and `--link`/`--link-path`; SDL is **not** installed or versioned
by `elephc native`.

### Managed native packages

Curated C/C++ dependencies are declared and installed with `elephc native`, not
with raw linker flags. During a final link, the compiler resolves logical
requirements against the nearest project's `elephc.toml`, deterministic
`elephc.lock`, and verified target/toolchain cache receipt. It passes exact
static archive paths to the linker; compilation never downloads or builds them.

The current catalog contains PCRE2 10.47 and zlib 1.3.2. Regex use links PCRE2's
managed archives in the fixed shim/POSIX/8-bit order and has no production
system-library fallback:

```bash
elephc native add pcre2
elephc app.php
```

Declaring PCRE2 does not force it into a program that does not use regex. Exact
managed archives remain compatible with Linux's static-link preference. zlib is
the second exact pure-C recipe; it is available for curated runtime/builtin
integration, not an automatic replacement for arbitrary `extern "z"` and `-lz`
workflows. See [Native dependencies](native-dependencies.md) for the full
workflow.

### `--link` / `-l`

Links an extra native library. Accepts the spaced form, the short flag, and the
attached form; repeat it for multiple libraries.

```bash
elephc app.php --link sqlite3
elephc app.php -l sqlite3
elephc app.php -lsqlite3
```

### `--link-path` / `-L`

Adds a directory to the library search path. Repeatable.

```bash
elephc app.php -l sqlite3 -L /opt/homebrew/lib
elephc app.php --link-path /usr/local/lib
```

### `--framework`

Links a macOS framework. Repeatable.

```bash
elephc app.php --framework Cocoa --framework Metal
```

`extern "libname" { ... }` blocks in source add their own `-l` flags
automatically; the flags above are for libraries not already named in the source.
They do not override or satisfy a missing managed-package requirement such as
PCRE2.
See [FFI & Extern](../beyond-php/extern.md).

## Bridge crates and `--with-NAME`

Some optional features are implemented as Rust *bridge crates* (`staticlib`
archives) that elephc links into the program: `pdo` (database access), `tls`
(`https://`/`ftps://` streams), `crypto` (the `hash()`/`md5()`/`sha1()` family),
`bcmath` (exact arbitrary-precision decimal arithmetic),
`phar` (Phar archives), `tz` (timezone introspection), `image` (GD/Imagick image
processing), `pcntl` (Unix process control and signals), `eval` (the Magician
interpreter fallback for dynamic `eval()`), and `web` (the `--web` server).

By default a bridge is linked **only when the program uses it** — using a hash
function pulls in `crypto`, opening an `https://` stream pulls in `tls`,
calling a `bc*` function pulls in `bcmath`, referencing `PDO` pulls in `pdo`,
and so on. An `eval()` call pulls in Magician
only when it needs runtime parsing: eligible literal fragments can be parsed at
compile time and lowered to native EIR without the interpreter bridge. Programs
that do not need a feature never link its crate, so binaries stay small.

`--with-CRATE` force-enables a bridge regardless of that auto-detection. It
force-links the staticlib (whole-archived, so it is retained even if no symbol
references it) and, for crates whose PHP surface comes from an injected prelude
(`pdo`, `tz`, `image`), force-injects that prelude so the classes/functions are
available. This is useful when a program reaches a feature through indirection
that detection cannot see. The flag is repeatable:

```bash
elephc app.php --with-pdo
elephc app.php --with-crypto --with-tls
elephc app.php --with-bcmath
elephc app.php --with-pcntl
elephc app.php --with-eval
```

`--with-pcntl` force-links the process-control bridge for indirect or opaque
runtime calls. Statically visible `pcntl_*` calls auto-link it. See
[PCNTL](../php/pcntl.md) for target availability, fork/signal semantics, and
documented limits.

`--with-eval` force-links `elephc_magician`; it does not enable new syntax or
change which fragments are eligible for AOT lowering. Normal eval usage is
detected automatically. See [Eval](../php/eval.md) for language semantics and
[Eval Runtime Architecture](../internals/eval-runtime.md) for the AOT/fallback
decision and scope ABI.

`--with-regex` is a runtime-capability flag rather than a Rust bridge flag.
Dynamic eval source cannot be inspected for feature use, so the flag requests
the ordinary regex runtime and managed PCRE2 archives, then registers that
provider with Magician:

```bash
elephc native add pcre2
elephc --with-regex app.php
```

Without it, dynamic eval still compiles and runs non-regex code, but `preg_*`
names are unavailable there and calls fail at runtime. A statically visible
regex use enables the same provider automatically. Declaring PCRE2 without
either trigger does not link it.

`--with-web` is an alias for [`--web`](../beyond-php/web.md) (the full server
mode, which owns the program entry point). An unknown capability name is
rejected with the list of valid names. Forcing a bridge increases binary size,
since the whole archive is included.

Bridge crates are Elephc's optional Rust workspace components. They are not
installed or versioned by `elephc native`. Runtime-capability flags may require
a separately declared managed package, as `--with-regex` requires `pcre2`;
the flag itself does not install it. Composer dependencies are PHP source
handled by the compile-time autoload pipeline and remain separate.

## Heap size

The compiled program uses a fixed-size runtime heap, **8 MB** by default. Programs
that allocate a lot of arrays, strings, or objects may need more.

### `--heap-size`

Sets the heap size in bytes. The minimum is `65536` (64 KB).

```bash
elephc --heap-size=16777216 heavy.php   # 16 MB
```

If a program exhausts its heap it aborts with a fatal "heap memory exhausted"
error; raising `--heap-size` is the fix. See [Memory Model](../internals/memory-model.md).

## Runtime dead stripping

The compiler ships a single runtime with helpers for every supported builtin, but
a given program only uses a few of them. When linking an **executable**, the
linker keeps only the runtime helpers reachable from the program and drops the
rest, so a small program does not carry the whole runtime. This is automatic —
there is no flag — and never changes behavior, only binary size.

It works the same on every supported target, using each platform's native
mechanism:

- **Linux** emits each runtime helper into its own section and links with
  `--gc-sections`.
- **macOS** emits the runtime object with `.subsections_via_symbols` so each
  helper is a separately collectable atom, and links with `-dead_strip`.

Shared libraries (`--emit cdylib`) keep the full runtime, since any exported
symbol may be reached by a host the linker cannot see.

## Symbol stripping

Dead stripping removes unreachable *code*. Stripping removes the *names* of the
code that stays. The two are independent: the first changes what runs, the
second changes only what the file says about itself.

A linked **executable** is stripped of its symbol table. Nothing in a compiled
program reads those names — `Throwable::getTrace()` and `getTraceAsString()` are
not implemented, and the uncaught-exception report prints no stack trace — so
they are dead weight at run time, and they are roughly a quarter of the file:

| program | linked | stripped |
|---|--:|--:|
| `<?php echo 1;` | 182 680 B | 132 760 B (−27%) |
| a realistic program | 213 528 B | 152 152 B (−29%) |

The share grows with the program rather than shrinking, because the symbol table
scales with the number of declarations while the text section does not.

Two flags keep the names, for the two reasons to want them:

- `--debug-info` already means "I am going to debug this", so the pipeline does
  not invoke `strip` in this mode. On macOS, `dsymutil` still bakes the dSYM and
  the linked executable keeps its symbol table too.
- `--keep-symbols` is for profilers, which read the symbol table and have no
  other source of names.

**Shared libraries are never stripped.** Their exported symbols are their
interface: a host resolving one with `dlsym` would get a null it may well treat
as "feature absent" rather than as an error.

If the `strip` tool is missing, or cannot read the target's object format, the
compiler warns and keeps the larger binary. A failed build would be the worse
outcome for what is only a size optimization.

## Binary hardening

Compiled binaries are hardened by default. There is no flag: the options below
are always applied and cannot be turned off.

On **Linux**, every executable and shared library is linked with:

| Option | Effect |
|---|---|
| `-z noexecstack` | Marks the stack non-executable (`PT_GNU_STACK` `RW`). elephc assembles its objects with `as`, which emits no `.note.GNU-stack` section, so without this GNU ld infers an **executable** stack and warns. Nothing elephc produces needs one: there is no JIT, and Fiber stacks are mapped read/write with a guard page. |
| `-z relro` | Maps the relocated head of the data segment read-only once startup relocation is done. |
| `-z now` | Resolves all relocations eagerly at load time, so `relro` can cover the GOT (full RELRO). |

Whether the executable is also position-independent is decided by the system
toolchain, not by elephc: Linux executables are linked `-static` whenever the
program needs no dynamic library, and a driver configured with default-PIE (for
example Alpine/musl) turns that into a **static PIE**, while a driver without it
(for example Debian/Ubuntu glibc) produces a classic non-PIE static executable.
elephc does not force `-static-pie`, because it requires a libc built with
static-PIE support (`rcrt1.o`) that many distributions do not ship, and a
missing one is a hard link failure.

On **macOS** these options do not apply: `ld64` does not accept `-z`, binaries
are position-independent by default, and the stack is non-executable at the
platform level.

## Conditional compilation

elephc supports compile-time feature branches with `ifdef`. Symbols are defined
on the command line and the branches are resolved before optimization and code
generation, so unused branches are never compiled.

### `--define` / `--define=`

Defines a compile-time symbol. Repeatable. It may be combined with
[`--strict-php`](cli-reference.md#strict-php-mode), but strict auditing still
rejects every `ifdef` in physical PHP source. The combination exists for mixed
projects and LFC source: LFC `ifdef` consumes the symbol while PHP source
remains audited.

```bash
elephc --define DEBUG app.php
elephc --define=DEBUG --define=METAL app.php
elephc --strict-php --define DEBUG app.lfc
```

```php
ifdef (DEBUG) {
    echo "debug build\n";
}
```

See [Conditional Compilation](../beyond-php/ifdef.md) for the full `ifdef` syntax
and semantics.
