---
title: "CLI reference"
description: "The complete, authoritative list of every elephc command-line flag, its accepted values, default, and environment-variable override."
sidebar:
  order: 3
---

This page lists every flag the `elephc` command accepts. Topical pages
([optimization](optimization.md), [output](output-and-diagnostics.md),
[linking](linking-and-conditional-compilation.md)) explain the *why*; this page is
the exhaustive *what*.

## Synopsis

```text
elephc [OPTIONS] <source.php>
```

Exactly one positional argument is required: the path to the PHP source file. The
binary is written next to it, named after the source without its extension.

## Input and output

| Flag | Values | Default | Description |
|---|---|---|---|
| `<source.php>` | path | — | Required. The PHP file to compile. |
| `--emit KIND` / `--emit=KIND` | `executable` (`exe`, `bin`), `cdylib` (`dylib`, `shared`) | `executable` | Output artifact kind. `cdylib` builds a C-ABI shared library. |
| `--emit-asm` | — | off | Write generated assembly instead of a binary. |
| `--emit-ir` | — | off | Print the EIR textual form and stop. |
| `--check` | — | off | Run front-end checks only; write nothing. |
| `--source-map` | — | off | Emit a `.map` sidecar next to the assembly. |

`--emit-ir`, `--emit-asm`, and `--check` are mutually exclusive. See
[Output formats and diagnostics](output-and-diagnostics.md).

## Targets

| Flag | Values | Default | Description |
|---|---|---|---|
| `--target TARGET` / `--target=TARGET` | `macos-aarch64`, `linux-aarch64`, `linux-x86_64` (plus alias spellings) | host platform | Select the compilation target. |

See [Targets and cross-compilation](targets.md) for the full list of accepted
spellings.

## Optimization and code generation

| Flag | Values | Default | Env override | Description |
|---|---|---|---|---|
| `--ir-opt=on\|off` | `on`, `off` | `on` | `ELEPHC_IR_OPT` | Toggle the EIR optimization passes: identity folding, peepholes, constant folding, common-subexpression elimination, loop-invariant code motion, dead-instruction elimination, dead-store elimination, branch simplification, and the cross-function small-function inliner — run to a module-level fixed point. |
| `--no-ir-opt` | — | — | `ELEPHC_IR_OPT=off` | Shorthand for `--ir-opt=off`. |
| `--regalloc=linear\|stack` | `linear`, `stack` | `linear` | `ELEPHC_REGALLOC` | Register allocator: linear-scan, or stack-only fallback. |
| `--null-repr=sentinel\|tagged` | `sentinel`, `tagged` | `tagged` | `ELEPHC_NULL_REPR` | Representation for null-capable scalar slots. |
| `--ir-backend` | — | on | — | Force the EIR backend (already the default). |
| `--ast-backend` | — | off | — | **Deprecated.** Legacy direct AST backend; removal planned for v0.26.0. |

`--ir-backend` and `--ast-backend` cannot be combined. See
[Optimization and codegen controls](optimization.md).

## Linking and FFI

| Flag | Values | Default | Description |
|---|---|---|---|
| `--link LIB` / `-l LIB` / `-lLIB` | library name | — | Link an extra native library (repeatable). |
| `--link-path DIR` / `-L DIR` / `-LDIR` | directory | — | Add a library search path (repeatable). |
| `--framework NAME` | framework name | — | Link a macOS framework (repeatable). |

See [Linking, heap, and conditional compilation](linking-and-conditional-compilation.md).

## Memory and conditional compilation

| Flag | Values | Default | Description |
|---|---|---|---|
| `--heap-size=BYTES` | integer ≥ 65536 | `8388608` (8 MB) | Size of the program's runtime heap. |
| `--define SYMBOL` / `--define=SYMBOL` | symbol name | — | Define a compile-time symbol for `ifdef` (repeatable). |

## Diagnostics and debugging

| Flag | Values | Default | Description |
|---|---|---|---|
| `--timings` | — | off | Print per-phase compiler timings to stderr. |
| `--gc-stats` | — | off | Print allocation/free counters at exit. |
| `--heap-debug` | — | off | Enable runtime heap verification (double-free, bad refcount, free-list corruption). |

See [Output formats and diagnostics](output-and-diagnostics.md).

## Environment variables

Three environment variables provide defaults that the matching flag overrides.
They exist mainly so a whole test run or benchmark can flip a default without
changing every invocation:

| Variable | Values | Equivalent flag |
|---|---|---|
| `ELEPHC_IR_OPT` | `on`, `off` | `--ir-opt=` |
| `ELEPHC_REGALLOC` | `linear`, `stack` | `--regalloc=` |
| `ELEPHC_NULL_REPR` | `tagged`, `sentinel` | `--null-repr=` |
