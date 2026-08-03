---
title: "Targets and cross-compilation"
description: "The supported target matrix, how to select a target with --target, and the accepted target spellings."
sidebar:
  order: 4
---

elephc compiles to native machine code for a fixed set of first-class targets.
All native targets are equal: a feature is not considered done until it works on
every one of them. Elephc can also target experimental platforms outside this
matrix. `wasm32-wasi` is one such target: it compiles to a WebAssembly module
rather than native machine code and is documented separately below.

## Supported target matrix

| Target | Platform | Architecture |
|---|---|---|
| `macos-aarch64` | macOS | ARM64 (Apple Silicon) |
| `linux-aarch64` | Linux | ARM64 |
| `linux-x86_64` | Linux | x86-64 |

By default the compiler targets the **host** it runs on, detected automatically.
The native macOS/Linux targets are the first-class supported matrix.

## Experimental targets

| Target | Platform | Architecture | Status |
|---|---|---|---|
| `wasm32-wasi` | WebAssembly / WASI Preview 1 | wasm32 | Experimental; incomplete PHP/EIR parity and portability gates |

Experimental target availability is not a first-class support guarantee. See
[WebAssembly partial parity](#webassembly-partial-parity) for the currently
tested surface and open gates.

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
consumes the same EIR the native backends use. The target is experimental.
Production artifact generation performs in-process Core validation with
`wasmparser`. The dedicated host-portability job is green on Elephc
`0505b837ad`; it checksum-pins Wasmer 7.2.1, Wasmtime 47.0.2, `wasm-tools`
1.254.0, Node 26.3.0, and TypeScript 6.0.3. `--emit npm` packages the resulting
command module for Node.js 20 or newer.

Unlike the native macOS/Linux targets, `wasm32-wasi` is **not yet at full
parity**. It supports a growing subset of the language, and an EIR operation
that the WebAssembly backend does not yet implement aborts compilation of the
whole module rather than degrading a single function. The pre-emission
capability audit classifies every operation, runtime identity, and terminator,
then checks cross-cutting operand, result, immediate, representation,
ownership, callable, object/property, iterator, and control-flow shapes. Static
acceptance is followed by one exact in-memory planning pass; every fallible
lowering and identifier-consistency check must succeed before an accepted plan
exists. Artifact publication consumes that private plan without re-running
lowering, so a module accepted by the complete gate cannot later fail with a
backend `Unsupported`. Rejected source remains a target-capability gap even
though no artifact is published. The audited acceptance contract and remaining
gaps are tracked in
[WebAssembly and PHP Compliance](../specs/wasm-compliance.md).

The durable tested inventory currently includes:

- production in-process WAT assembly and Core 3.0 validation;
- focused artifact publication, shape-complete capability, and target-capability rejection
  tests;
- focused typed-transfer, `$argc`, void/Mixed result, block-argument, loop, and
  deterministic-identifier regressions;
- one shared artifact validated with `wasmparser`, `wasm-tools`, Wasmer,
  Wasmtime, and Node, then executed under all three hosts with exact output and
  `exit(7)`;
- independent-process WAT/WASM/npm/archive reproducibility, partial `fd_write`,
  repeated Node imports, npm contents, and strict TypeScript NodeNext checks.

It does not yet include a full php-src differential corpus or exhaustive EIR
shape, ownership, argument/environment/preopen, and process-status coverage.

To select it:

```bash
elephc --target wasm32-wasi hello.php
elephc --target wasm32-wasi --emit npm hello.php
node hello-npm/index.mjs
```

The NPM form writes `hello-npm/` with `module.wasm`, an ESM loader
(`index.mjs`), TypeScript declarations, package metadata, and a README. The
loader can also be imported:

```js
import { run } from "./hello-npm/index.mjs";

const exitCode = await run({
  args: ["hello", "first-argument"],
  env: process.env,
  preopens: { "/work": process.cwd() },
});
```

WASM output is currently a WASI command (`_start`); `--emit cdylib` reactors are
rejected with a focused diagnostic.

Native-only compiler options are also rejected with focused diagnostics instead
of being ignored: web-server mode, native source maps/DWARF, native heap and
register-allocation controls, native linker/framework flags, and bridge-crate
linking are not yet available on `wasm32-wasi`.

The parser also recognizes `macos-x86_64` / `x86_64-apple-darwin` and
`windows-x86_64` / `x86_64-pc-windows-msvc` /
`x86_64-pc-windows-gnu`. These spellings are groundwork for future backends,
not supported targets: compilation stops with an explicit unsupported-backend
diagnostic.

## Cross-compilation notes

Selecting a native target different from the host produces assembly and an
object file for that target. Producing a final linked binary still depends on
having a linker and any target libraries available for that platform; the
elephc test suite uses the Docker scripts under `scripts/` to build and run the
Linux targets from a macOS host. `wasm32-wasi` instead follows the artifact and
host workflow described above.

For the target-aware ABI and runtime details behind each platform, see
[Architecture](../internals/architecture.md) and
[The Code Generator](../internals/the-codegen.md).
