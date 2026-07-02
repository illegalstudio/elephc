---
title: "Targets and cross-compilation"
description: "The supported target matrix, how to select a target with --target, and the accepted target spellings."
sidebar:
  order: 4
---

elephc compiles to native machine code for a fixed set of first-class targets.
All native targets are equal: a feature is not considered done until it works on
every one of them. In addition to the native matrix, elephc can also target
`wasm32-wasi`, which compiles to a WebAssembly module rather than native machine
code; it is a non-native, growing-subset target documented separately below.

## Supported target matrix

| Target | Platform | Architecture |
|---|---|---|
| `macos-aarch64` | macOS | ARM64 (Apple Silicon) |
| `linux-aarch64` | Linux | ARM64 |
| `linux-x86_64` | Linux | x86-64 |
| `wasm32-wasi` | WebAssembly / WASI | wasm32 |

By default the compiler targets the **host** it runs on, detected automatically.
The native macOS/Linux targets are at full parity; `wasm32-wasi` supports a
growing subset of the language (see [WebAssembly partial parity](#webassembly-partial-parity)).

## Selecting a target

```bash
elephc --target linux-aarch64 hello.php
elephc --target linux-x86_64 hello.php
elephc --target=macos-aarch64 hello.php
```

Both the spaced (`--target VALUE`) and inline (`--target=VALUE`) forms work.

## Accepted spellings

Each target accepts several spellings, including the LLVM-style triple, so build
scripts written for other toolchains keep working:

| Canonical | Also accepted |
|---|---|
| `macos-aarch64` | `macos-arm64`, `aarch64-apple-darwin` |
| `linux-aarch64` | `linux-arm64`, `aarch64-unknown-linux-gnu` |
| `linux-x86_64` | `x86_64-unknown-linux-gnu` |
| `wasm32-wasi` | `wasm32-wasip1`, `wasm32-unknown-wasi`, `wasm` |

## WebAssembly partial parity

The `wasm32-wasi` target is a non-native target: instead of emitting native
assembly and invoking the system assembler and linker, it emits a WebAssembly
module (`.wat`/`.wasm`) through the dedicated `src/codegen_wasm` backend, which
consumes the same EIR the native backends use. It runs under any WASI host
(for example `wasmer` or `wasmtime`), and `--emit npm` packages the resulting
module as an NPM package.

Unlike the native macOS/Linux targets, `wasm32-wasi` is **not yet at full
parity**. It supports a growing subset of the language, and an EIR operation
that the WebAssembly backend does not yet implement aborts compilation of the
whole module rather than degrading a single function. As of this writing the
higher-order array builtins implemented for `wasm32-wasi` are `array_map`,
`array_filter`, `usort`, `uasort`, `uksort`, `array_reduce`, and `array_walk`,
together with `call_user_func` and `call_user_func_array`. This list reflects
current status and grows over time.

To select it:

```bash
elephc --target wasm32-wasi hello.php
elephc --target wasm32-wasi --emit npm hello.php
```

## Cross-compilation notes

Selecting a target different from the host produces assembly and an object file
for that target. Producing a final linked binary still depends on having a
linker and any target libraries available for that platform; the elephc test
suite uses the Docker scripts under `scripts/` to build and run the Linux
targets from a macOS host.

For the target-aware ABI and runtime details behind each platform, see
[Architecture](../internals/architecture.md) and
[The Code Generator](../internals/the-codegen.md).
