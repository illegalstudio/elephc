---
title: "Installation"
description: "How to install elephc on supported platforms."
sidebar:
  order: 1
---

## Requirements

- Rust toolchain (`cargo`) if building from source
- A native assembler and linker for your host platform

On macOS, install Xcode Command Line Tools if you don't have them already:

```bash
xcode-select --install
```

This provides the assembler (`as`) and linker (`ld`) that elephc uses to produce native binaries.

On Linux, install your distro's standard native toolchain so `as`, `ld`, and the libc development files are available.

## Homebrew (macOS)

```bash
brew install illegalstudio/tap/elephc
```

Verify the installation:

```bash
elephc --version
```

## From source

If you prefer to build from source, you'll also need the Rust toolchain (`cargo`).

```bash
git clone https://github.com/illegalstudio/elephc.git
cd elephc
cargo build --release
```

The binary is at `./target/release/elephc`. You can copy it to a directory in your `PATH`:

```bash
cp target/release/elephc /usr/local/bin/
```

## From GitHub releases

Pre-built binaries may be available on the [releases page](https://github.com/illegalstudio/elephc/releases). Download the artifact for your platform, make it executable if needed, and move it to your `PATH`:

```bash
chmod +x elephc
mv elephc /usr/local/bin/
```
