---
title: "Output formats and diagnostics"
description: "Choosing what the compiler produces (executable, cdylib, assembly, IR) and the flags that inspect or instrument a compile."
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
```

Accepted values and aliases:

| Value | Aliases | Produces |
|---|---|---|
| `executable` | `exe`, `bin` | A standalone native binary. |
| `cdylib` | `dylib`, `shared` | A C-ABI shared library (`.dylib`/`.so`). |

The inline form `--emit=cdylib` also works. For exporting C-ABI functions from a
`cdylib`, see [Shared Libraries (cdylib)](../beyond-php/cdylib.md).

### `--emit-asm`

Writes the generated assembly next to the source instead of assembling and
linking a binary. Useful for inspecting exactly what the backend produced.

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

## Compile-time diagnostics

### `--timings`

Prints how long each compilation phase took, to stderr. The labels match the
[pipeline phases](compilation-pipeline.md).

```bash
elephc --timings hello.php
```

## Runtime diagnostics

These flags instrument the **compiled program**, not the compiler.

### `--gc-stats`

Compiles the program so it prints allocation and free counters to stderr when it
exits — useful when debugging reference-counting and ownership behavior.

```bash
elephc --gc-stats heavy.php
./heavy
```

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
