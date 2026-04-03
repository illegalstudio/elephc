---
title: "Installation"
description: "How to install elephc on macOS with Homebrew or from source."
sidebar:
  order: 1
---

## Requirements

- macOS on Apple Silicon (ARM64)
- Xcode Command Line Tools

If you don't have Xcode Command Line Tools installed:

```bash
xcode-select --install
```

This provides the assembler (`as`) and linker (`ld`) that elephc uses to produce native binaries.

## Homebrew (recommended)

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

Pre-built binaries are available on the [releases page](https://github.com/illegalstudio/elephc/releases). Download the latest `elephc` binary for macOS ARM64, make it executable, and move it to your `PATH`:

```bash
chmod +x elephc
mv elephc /usr/local/bin/
```
