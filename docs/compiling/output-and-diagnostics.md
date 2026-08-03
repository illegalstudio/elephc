---
title: "Output formats and diagnostics"
description: "Choosing what the compiler produces (executable, cdylib, WebAssembly/NPM, assembly, IR) and the flags that inspect or instrument a compile."
sidebar:
  order: 6
---

By default `elephc` produces a native executable. These flags change the output
artifact or stop the pipeline early to inspect an intermediate stage, plus the
diagnostics that instrument a compile or the resulting program.

## Output artifacts

### `--emit`

Selects the kind of artifact to produce.

```bash
elephc --emit executable app.php   # default: a native binary
elephc --emit cdylib lib.php       # a C-ABI shared library
elephc --target wasm32-wasi --emit npm app.php  # app-npm/ Node.js package
```

Accepted values and aliases:

| Value | Aliases | Produces |
|---|---|---|
| `executable` | `exe`, `bin` | A standalone native binary, or a WASI `.wasm` command for `wasm32-wasi`. |
| `cdylib` | `dylib`, `shared` | A C-ABI shared library (`.dylib`/`.so`). |
| `npm` | `npm-package` | A Node.js 20+ ESM/WASI package in `<stem>-npm/`; requires `--target wasm32-wasi`. |

The inline form `--emit=cdylib` also works. For exporting C-ABI functions from a
`cdylib`, see [Shared Libraries (cdylib)](../beyond-php/cdylib.md).
WASM cdylib/reactor output is not implemented. A generated NPM package contains
`module.wasm`, `index.mjs`, `index.d.ts`, `package.json`, and `README.md`; run it
with `node <stem>-npm/index.mjs` or import its asynchronous `run()` function.

### `--emit-asm`

Writes the generated native assembly next to the source instead of assembling
and linking a binary. For `wasm32-wasi`, it writes the readable `.wat` module
instead of encoding `.wasm`. Useful for inspecting exactly what the backend
produced.

```bash
elephc --emit-asm hello.php
```

### `--emit-ir`

Prints the EIR (elephc's intermediate representation) textual form to stdout and
stops before code generation. Because it runs after the
[EIR optimization passes](optimization.md#eir-optimization-passes), it reflects
the optimized IR; combine with [`--no-ir-opt`](optimization.md#eir-optimization-passes)
to see the unoptimized form.

```bash
elephc --emit-ir hello.php
elephc --emit-ir --no-ir-opt hello.php
```

See [The EIR Design](../internals/the-ir.md) for how to read the output.

### `--check`

Runs the front end — lexing, parsing, name resolution, type checking — and
reports errors and warnings without writing any assembly or binary. This is the
fastest way to validate a file.

```bash
elephc --check hello.php
```

`--emit-ir`, `--emit-asm`, and `--check` are mutually exclusive.

### `--source-map`

Emits a `.map` sidecar file next to the generated assembly, mapping assembly back
to PHP source positions. The sidecar is a versioned JSON document with function
ranges, assembly labels, opcode-tagged line mappings, and a PHP-line inverse
index — see [Source maps](source-maps.md) for the schema contract.

This is currently a native-only output; `wasm32-wasi` rejects the option.

```bash
elephc --emit-asm --source-map hello.php
```

### `--debug-info`

Embeds DWARF debug information in the generated assembly — a line table and one
`DW_TAG_subprogram` per PHP function, derived from the same source markers that
drive `--source-map`. Standard debuggers (lldb, gdb) and profilers then map
compiled code back to PHP lines without any custom tooling. On macOS a `.dSYM`
bundle is produced next to the binary:

```bash
elephc --debug-info hello.php
lldb ./hello   # breakpoints and backtraces resolve to hello.php lines
```

`--debug-info` and `--source-map` compose: the first serves standard DWARF
consumers, the second serves tools that want the richer JSON schema.
Both options are currently native-only.

## Compile-time diagnostics

On an interactive terminal, each compilation phase starts as a spinner. When
the phase finishes, elephc keeps its action-oriented label with a checkmark and
elapsed time, then starts the next phase on a new line. Non-interactive output
and `--quiet` keep the compact plain output without progress lines.

### `--timings`

Prints a bordered timing table to stderr in addition to the interactive
completed-phase lines. It uses friendly phase labels, adaptive millisecond or
second durations, percentage shares, and a total row. Interactive terminals
use Unicode box drawing; non-interactive output and `--quiet` use ASCII borders
without styling.

```bash
elephc --timings hello.php
```

```text
Compiler timings
┌────────────────────────────────┬───────────┬────────┐
│ Phase                          │  Duration │  Share │
├────────────────────────────────┼───────────┼────────┤
│ Reading source                 │   0.54 ms │   0.0% │
│ Checking types                 │ 155.70 ms │   1.4% │
│ ...                            │       ... │    ... │
│ Optimizing EIR                 │    2.78 s │  25.1% │
│ Generating native code         │    6.66 s │  60.1% │
├────────────────────────────────┼───────────┼────────┤
│ Total                          │   11.08 s │ 100.0% │
└────────────────────────────────┴───────────┴────────┘
```

## Runtime diagnostics

These flags instrument the **compiled program**, not the compiler.
They are currently implemented by the native runtime and are rejected for
`wasm32-wasi`.

### `--gc-stats`

Compiles the program so it prints allocation and free counters to stderr when it
exits — useful when debugging reference-counting and ownership behavior.

```bash
elephc --gc-stats heavy.php
./heavy
```

Combined with `--web`, the server never reaches the process-exit report, so the
counters are printed to stderr after every handled request instead — a growing
`allocs - frees` gap across requests indicates a per-request leak.

### `--heap-debug`

Enables runtime heap verification in the compiled program: double-free
detection, bad-refcount checks, and free-list corruption checks. Slower, but
invaluable when chasing memory bugs.

```bash
elephc --heap-debug heavy.php
./heavy
```

See [Memory Model](../internals/memory-model.md) and
[The Runtime](../internals/the-runtime.md) for what these report on.
