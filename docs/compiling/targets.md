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
elephc --target linux-aarch64 hello.php
elephc --target linux-x86_64 hello.php
elephc --target=macos-aarch64 hello.php
elephc --target ios-arm64 --emit staticlib module.php
elephc --target ios-sim-arm64 --emit staticlib module.php
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

The parser also recognizes `macos-x86_64` / `x86_64-apple-darwin` as groundwork
for a future backend; compilation stops with an explicit unsupported-backend
diagnostic. `windows-x86_64` and `x86_64-pc-windows-gnu` select the
experimental GNU/MinGW-ABI PE32+ backend. The
`x86_64-pc-windows-msvc` spelling is rejected explicitly because elephc does
not emit the MSVC ABI.

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

## Windows codegen gate

`windows-x86_64` is an experimental target, not yet a first-class supported
target. CI nevertheless treats its codegen suite as a strict native gate: every
runnable `ci`-profile codegen fixture is compiled and executed directly on
Windows Server 2025.

### How the gate works

The `windows-codegen-native` job splits the runnable inventory into 8
deterministic hash partitions, each running two nextest workers. Each shard runs
through `cargo nextest` with no failure baseline: one failing fixture makes that
shard fail. There is no allow-list or known-failures mechanism in this gate.

Every shard uploads its JUnit report, and shard 1 also uploads the complete
runnable inventory. The aggregating `windows-codegen-gate` job then verifies
that:

1. all 8 reports are present;
2. every runnable fixture appears exactly once across those reports;
3. no reported test failed; and
4. all native shard jobs succeeded.

This prevents both test failures and missing or truncated partitions from being
accepted as a green Windows result. The aggregate gate is required by the
top-level `test` job.

## Windows bridge verification

CI builds and tests the image, PDO, Phar, crypto, timezone, TLS, and web bridges
on a native Windows host using Rust's MSVC host toolchain. It also builds their
GNU/COFF static archives for `x86_64-pc-windows-gnu` and verifies the exported C
ABI against every `#[no_mangle] pub extern "C"` entry point declared in the
bridge sources.

The native job also runs the complete dedicated `windows_pe` integration module
and the bridge unit tests. TLS attachment uses full-width Winsock `SOCKET`
values and duplicates the live socket before rustls adopts it; the native bridge
tests verify that ownership boundary. Magician is built with the native MinGW
PCRE2 sysroot and exercised by the eval fixtures in the strict codegen shards.
The web bridge's PE tests cover its exported C ABI, successful and bad-request
HTTP responses, and clean `--max-requests` shutdown.

These checks are necessary but are not by themselves a promotion signal.
Windows stays experimental until the first complete strict native run is green
and the target-policy review confirms that no Windows-only reduced semantics
remain.

### Promotion checklist

Promotion is a deliberate documentation and CI change after native validation,
not an automatic consequence of one green shard run. It requires all of the
following from the same revision:

1. all 8 strict native shards pass and the aggregate coverage check accounts
   for every runnable `ci`-profile codegen fixture exactly once;
2. the native Windows PE, bridge, eval, and web suites pass;
3. bridge export verification and GNU plus LLVM/LLD toolchain probes pass; and
4. the target-policy review confirms no Windows-only reduced semantics remain.

Until then, keep Windows labeled experimental in the target table, README, CLI
reference, architecture guide, developer policy, changelog, and CI job comments.
