---
title: "Targets and cross-compilation"
description: "The supported target matrix, how to select a target with --target, and the accepted target spellings."
sidebar:
  order: 4
---

elephc compiles to native machine code for a fixed set of first-class targets.
All supported targets are equal: a feature is not considered done until it works
on every one of them.

## Supported target matrix

| Target | Platform | Architecture |
|---|---|---|
| `macos-aarch64` | macOS | ARM64 (Apple Silicon) |
| `ios-arm64` | iOS device | ARM64 |
| `ios-sim-arm64` | iOS Simulator | ARM64 |
| `linux-aarch64` | Linux | ARM64 |
| `linux-x86_64` | Linux | x86-64 |

By default the compiler targets the **host** it runs on, detected automatically.

## Selecting a target

```bash
elephc build --target linux-aarch64 hello.php
elephc build --target linux-x86_64 hello.php
elephc build --target=macos-aarch64 hello.php
elephc build --target ios-arm64 --emit staticlib module.php
elephc build --target ios-sim-arm64 --emit staticlib module.php
```

Both the spaced (`--target VALUE`) and inline (`--target=VALUE`) forms work.

## Accepted spellings

Each target accepts several spellings, including the LLVM-style triple, so build
scripts written for other toolchains keep working:

| Canonical | Also accepted |
|---|---|
| `macos-aarch64` | `macos-arm64`, `aarch64-apple-darwin` |
| `ios-arm64` | `ios-aarch64`, `aarch64-apple-ios` |
| `ios-sim-arm64` | `ios-simulator-arm64`, `aarch64-apple-ios-simulator` |
| `linux-aarch64` | `linux-arm64`, `aarch64-unknown-linux-gnu` |
| `linux-x86_64` | `x86_64-unknown-linux-gnu` |

iOS targets are ARM64-only. `ios-x86_64`, `ios-sim-x86_64`, and their
x86_64 Apple-triple spellings are rejected during target parsing with an
arm64-only diagnostic.

The parser also recognizes `macos-x86_64` / `x86_64-apple-darwin` and
`windows-x86_64` / `x86_64-pc-windows-msvc` /
`x86_64-pc-windows-gnu`. These spellings are groundwork for future backends,
not supported targets: compilation stops with an explicit unsupported-backend
diagnostic.

## Cross-compilation notes

Selecting a target different from the host produces assembly and an object file
for that target. Producing a final linked binary still depends on having a
linker and any target libraries available for that platform; the elephc test
suite uses the Docker scripts under `scripts/` to build and run the Linux
targets from a macOS host.

iOS output is a native library consumed by an application host, not a complete
signed `.app` bundle. Select `--emit staticlib` (the usual Xcode delivery form)
or `--emit cdylib`; standalone `--emit executable` output is rejected for both
iOS device and Simulator targets. A linked iOS library requires macOS with the
matching Xcode SDK. `--emit-asm` can stop before assembling, but it must still be
paired with a library emit kind so the assembly contains the public library ABI.

For the target-aware ABI and runtime details behind each platform, see
[Architecture](../internals/architecture.md) and
[The Code Generator](../internals/the-codegen.md).
