---
title: "Installation"
description: "How to install elephc on supported platforms."
sidebar:
  order: 1
---

## Requirements

- Rust toolchain (`cargo`) if building from source
- A native assembler and linker for your host platform

Projects that install a curated native package also need a POSIX shell, Make, a
target C compiler, `ar`, and `ranlib`. Elephc verifies and uses these tools but
does not install them. PCRE2 itself does **not** need to be installed from a
system package for production compilation; `elephc native add pcre2` downloads,
verifies, and builds the catalogued source into Elephc's cache.

On macOS, install Xcode Command Line Tools if you don't have them already:

```bash
xcode-select --install
```

This provides the assembler (`as`), linker (`ld`), C compiler, archive tools,
and Make used to produce binaries and managed native artifacts.

On Linux, install your distro's standard native toolchain so `as`, `ld`, `cc`,
`ar`, `ranlib`, Make, and the libc development files are available. For example,
Debian/Ubuntu's `build-essential` or Alpine's `build-base` plus `make` provide
the usual host tools. Cross-target artifacts require an explicit matching C
compiler, archiver, and ranlib; see [Native
dependencies](../compiling/native-dependencies.md#build-tools-and-cross-targets).

## Homebrew (macOS)

```bash
brew install illegalstudio/tap/elephc
```

Verify the installation by compiling a small program:

```bash
echo '<?php echo "ok\n";' > check.php
elephc check.php && ./check
```

This prints `ok` and confirms `elephc` can produce and run a native binary.

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
