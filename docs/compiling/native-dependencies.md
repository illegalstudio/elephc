---
title: "Native dependencies"
description: "Declare, lock, install, inspect, and link curated native packages with elephc native."
sidebar:
  order: 9
---

Some generated programs call native C libraries. Elephc manages those libraries
as **curated native packages**: the project declares an exact version, the lock
records immutable catalog metadata, and `elephc native` builds verified static
archives into a target- and toolchain-specific cache.

PCRE2 10.47 is the first package. Programs using `preg_*`, `mb_ereg_match()`,
`RegexIterator`, or `RecursiveRegexIterator` require it at final link time.

## Quick start

From the project directory:

```bash
elephc native add pcre2
elephc main.php
./main
```

`add` declares the catalog default exact version, writes a deterministic lock,
downloads and verifies the source archive, then builds the selected host
artifact. Commit both `elephc.toml` and `elephc.lock`; the global artifact cache
does not belong in the repository.

For reproducible CI installation:

```bash
elephc native install --locked
```

Once the verified source and artifact are cached, the same check works without
network access:

```bash
elephc native install --locked --offline
```

## Command reference

```text
elephc native add <package>[@<exact-version>]
    [--target TARGET] [--offline] [--manifest-path FILE]

elephc native install
    [--target TARGET] [--locked] [--offline] [--manifest-path FILE]

elephc native update [<package>[@<exact-version>]]
    [--target TARGET] [--offline] [--manifest-path FILE]

elephc native remove <package>
    [--manifest-path FILE]

elephc native list
    [--target TARGET] [--manifest-path FILE]

elephc native doctor
    [--target TARGET] [--manifest-path FILE]
```

| Command | Effect |
|---|---|
| `add` | Add one exact catalog package and install it before publishing the manifest and lock. Re-adding the same exact version is idempotent; use `update` to change versions. |
| `install` | Reconcile the lock from the manifest when allowed, then materialize and verify selected target artifacts. |
| `update` | Refresh one package, or every package when no name is given, from the current built-in catalog. |
| `remove` | Remove the declaration and lock entry. Shared cached artifacts are retained. |
| `list` | Read-only status for each declared package: `installed`, `missing`, `corrupt`, `stale`, or `toolchain-error`. |
| `doctor` | Read-only project, lock, cache, toolchain, and receipt diagnostics. |

`--target` accepts the normal supported targets: `macos-aarch64`,
`linux-aarch64`, and `linux-x86_64`. It defaults to the host. GNU and musl are
cache ABI variants derived from the selected C compiler, not additional public
Elephc targets.

`--manifest-path` must name an `elephc.toml` file and disables ancestor
discovery. `--offline` guarantees that no downloader is invoked. `--locked` is
valid only for `install`; it requires an existing lock that exactly matches the
manifest and the catalog and never rewrites it.

`native list` and `native doctor` never use the network and never mutate the
project or cache. `elephc native --help` and each verb's `--help` work without a
project.

## Project discovery and files

Native commands search upward from the current directory for the nearest
`elephc.toml`. `native add` creates one in the current directory when none
exists. Compilation instead searches upward from the PHP source file's parent,
so a project selected by a recovery command must be an ancestor of that source.

A minimal manifest is:

```toml
[native]
schema = 1

[native.dependencies]
pcre2 = "10.47"
```

Manifest edits preserve comments, formatting, and unrelated top-level
sections. Native dependency values are exact catalog versions; version ranges,
arbitrary URLs, Git repositories, local paths, package-manager names, and
project-supplied build scripts are rejected.

`elephc.lock` expands each declaration to the immutable catalog source URL,
SHA-256, exact source size, recipe revision, provides set, dependencies, and
ordered link outputs for all supported targets. It contains no absolute cache
or compiler paths and is safe to commit. Do not edit it by hand.

## What happens during compilation

Ordinary compilation is read-only with respect to native packages. It never
downloads, extracts, configures, builds, or repairs an artifact, and it never
changes `elephc.toml` or `elephc.lock`. The final-link path resolves a program's
logical requirements against the nearest manifest, current lock, and a verified
cache receipt.

For PCRE2, the linker receives exact archive paths in this order:

```text
libelephc_pcre2_shim.a
libpcre2-posix.a
libpcre2-8.a
```

There is no production fallback to a system PCRE2 installation and raw
`--link pcre2-posix` flags do not satisfy the managed requirement. A program
that does not use regex does not link PCRE2 merely because the project declares
it. `--check`, `--emit-ir`, and `--emit-asm` do not perform the final link and
therefore do not require an installed artifact.

When state is missing, the diagnostic gives the recovery command. Typical
repairs are:

```bash
elephc native add pcre2
elephc native install
elephc native install --locked --target linux-x86_64
```

## Cache and integrity

The native cache root is selected in this order:

1. `ELEPHC_NATIVE_CACHE`;
2. `$XDG_CACHE_HOME/elephc/native`;
3. `$HOME/.cache/elephc/native`.

Source archives are content-addressed by SHA-256. Installed artifacts are keyed
by package/version/recipe/source, Elephc target, target C ABI, and a fingerprint
of the compiler, archiver, ranlib, and (on macOS) SDK. This prevents GNU, musl,
different architectures, or incompatible toolchains from sharing artifacts.

Downloads use HTTPS and are bounded and hashed before publication. Extraction
rejects path escapes, links, device entries, and oversized archives. Builds and
receipts are staged, verified, and atomically published under advisory locks, so
an interrupted or concurrent install cannot become a usable partial artifact.

An explicit `native add`, `install`, or `update` executes the verified upstream
source build with the user's permissions. V1 does not promise a portable OS
sandbox that blocks every filesystem or network access on macOS and Linux. Its
trust root is the catalog embedded in the installed Elephc binary, HTTPS PKI,
and the catalog's exact SHA-256; neither the manifest nor the lock can provide a
command, recipe, or replacement URL. Recipe processes receive a minimal
allowlisted environment and publish only reviewed catalog outputs.

## Build tools and cross targets

Installing PCRE2 from source requires a POSIX shell, Make, a target C compiler,
`ar`, and `ranlib`. Elephc does not install these tools. The recipe builds
static, position-independent PCRE2 8-bit, Unicode, and POSIX archives with JIT
and 16/32-bit libraries disabled.

Host builds use `cc`, `ar`, and `ranlib` by default. Override them with:

```text
ELEPHC_NATIVE_CC
ELEPHC_NATIVE_AR
ELEPHC_NATIVE_RANLIB
```

Target-specific overrides take precedence; replace `TARGET_ENV` with the
uppercase target and underscores, for example `LINUX_AARCH64`:

```text
ELEPHC_NATIVE_CC_<TARGET_ENV>
ELEPHC_NATIVE_AR_<TARGET_ENV>
ELEPHC_NATIVE_RANLIB_<TARGET_ENV>
```

All three commands are mandatory for a non-host target. Elephc validates the
compiler tuple and that the archive tools accept its objects before downloading
anything or changing project files.

## Four dependency mechanisms

These mechanisms solve different problems and are intentionally separate:

| Mechanism | Purpose | Managed by |
|---|---|---|
| Native packages | Curated external C/C++ source built into verified target-specific static archives | `elephc native` + `elephc.toml`/`elephc.lock` |
| Composer packages | PHP source discovered and inlined ahead of time | Composer metadata and Elephc's compile-time autoloader |
| Rust bridge crates | Optional Elephc workspace `staticlib` implementations such as `pdo`, `tls`, or `crypto` | Feature detection and `--with-<crate>` |
| Toolchains | Assemblers, linkers, C compilers, Make, SDKs, and cross tools | The user or operating system |

`elephc native` v1 manages only catalog packages. It is not a general package
manager and does not replace Composer, Cargo, Homebrew, apt, or a cross-toolchain
installer.
