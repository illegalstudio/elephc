---
title: "Installation"
description: "How to install elephc on supported platforms."
sidebar:
  order: 1
---

## Requirements

- A POSIX shell, `curl`, `tar`, and either `shasum` or `sha256sum` to install
  `elvm`
- Rust toolchain (`cargo`) if building from source
- A native assembler and linker for your host platform

Projects that install a curated native package also need a POSIX shell, Make, a
target C compiler, `ar`, and `ranlib`. Elephc verifies and uses these tools but
does not install them. Catalogued native sources do **not** need to be installed
from system packages; for example, `elephc native add pcre2`
downloads, verifies, and builds the exact source into Elephc's cache without a
system fallback.

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

## elvm (recommended)

[`elvm`](https://github.com/getelephc/elvm) is the recommended way to install
elephc. It keeps compiler releases and their bridge static libraries together,
supports multiple installed versions, and selects the active compiler through
the `elephc` command.

### Install elvm

```bash
curl -fsSL https://get.elephc.dev | sh
```

The installer downloads and verifies `elvm`, creates the `elephc` shim, and
prompts to add `~/.elvm/bin` to your `PATH`. It does not install a compiler
version. Start a new shell after accepting the prompt, then install and select
the latest published elephc release:

```bash
elvm install latest
elvm use latest --global
elvm current
```

`elvm install` verifies the release checksum and installs the compiler together
with its bridge static libraries. `elvm use latest --global` sets the highest
installed release as the default, and `elvm current` shows the resolved version
and where that selection came from.

### Pin a project version

Install a specific version, then run `elvm use` without `--global` in the
project directory:

```bash
elvm ls-remote
elvm install 0.26.5
elvm use 0.26.5
git add .elephc-version
```

Replace `0.26.5` with the release your project needs. Commit the generated
`.elephc-version` file so every contributor and CI job selects the same
compiler. In a checkout that already contains the file, install its requested
version with:

```bash
elvm install
```

When `elephc` runs, the first available selection wins:

1. `ELEPHC_VERSION`
2. the nearest `.elephc-version`, searching upward from the current directory
3. the global default set by `elvm use --global`

`latest` has two deliberately different meanings: `elvm install latest`
installs the newest published release, while `latest` in a version selection
resolves to the highest version already installed. Running `elephc` never
downloads a compiler automatically; if the selected version is missing, install
it with `elvm install`.

Use `elvm ls` to inspect installed versions, `elvm ls-remote` to inspect
published releases, and `elvm doctor` to diagnose `PATH`, shim, or installation
problems.

## Verify the installation

Compile a small program regardless of which installation method you used:

```bash
echo '<?php echo "ok\n";' > check.php
elephc build check.php && ./check
```

This prints `ok` and confirms `elephc` can produce and run a native binary.

## Homebrew (alternative, macOS)

```bash
brew install illegalstudio/tap/elephc
```

## mise (alternative)

[mise](https://mise.jdx.dev) can install elephc straight from the GitHub
release tarballs through its `github:` backend, with no plugin to add:

```bash
mise use -g github:illegalstudio/elephc@latest
```

Without `-g`, `mise use` writes the version into the project's `mise.toml`
instead, which pins the compiler per project:

```toml
[tools]
"github:illegalstudio/elephc" = "0.27.1"
```

mise picks the tarball for the current platform and keeps the bridge static
libraries next to the `elephc` binary, a layout elephc already searches, so no
further setup is needed. Only the platforms listed under
[GitHub releases](#from-github-releases-alternative) are available, and
`latest` resolves to the newest numbered release; nightly builds are not
installed through mise.

## From source (alternative)

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

## From GitHub releases (alternative)

Each release on the [releases page](https://github.com/illegalstudio/elephc/releases)
ships a tarball per supported platform:

- `elephc-<version>-aarch64-apple-darwin.tar.gz` — macOS ARM64
- `elephc-<version>-x86_64-unknown-linux-gnu.tar.gz` — Linux x86_64
- `elephc-<version>-aarch64-unknown-linux-gnu.tar.gz` — Linux ARM64

Each tarball contains the `elephc` binary plus the bridge staticlibs
(`libelephc_*.a`) that compiled programs link against. Install them in a layout
elephc searches: the staticlibs either in the same directory as the binary, or
in a `lib/` directory next to the `bin/` directory holding the binary — for
example `/usr/local/bin` and `/usr/local/lib`:

```bash
tar xzf elephc-<version>-<target>.tar.gz
sudo install -m 755 elephc /usr/local/bin/
sudo install -m 644 libelephc_*.a /usr/local/lib/
```

The Linux tarballs are built against glibc 2.35 and run on any distribution
with that glibc or newer (Ubuntu 22.04+, Debian 12+, RHEL 9+, ...).

## Nightly builds (unsupported)

Alongside the versioned releases, `main` is built every night and published as a
pre-release. Use it to try unreleased work or to confirm that a fix has landed —
not to run anything you depend on. Nightly artifacts carry no compatibility,
stability, or upgrade guarantees.

Each build is published under two tags:

- [`nightly`](https://github.com/illegalstudio/elephc/releases/tag/nightly) —
  a rolling tag that always points at the newest build. Its assets are replaced
  every night, so a URL under it downloads something different tomorrow.
- `nightly-YYYYMMDD` (the UTC build date, e.g. `nightly-20260901`) — immutable.
  Its assets are never replaced, so this is what to pin to. The **14** most
  recent are kept; older ones are deleted, tag included, and a pinned build that
  falls out of that window cannot be recovered. A second build on the same UTC
  day — which only happens when the workflow is run by hand after new commits —
  is published as `nightly-YYYYMMDD.2`.

Nothing is published on a day when `main` has not moved, so the newest nightly
can be older than today. Nightlies are only built from a commit whose CI run was
green.

The rolling download URLs are stable, so the newest tarball for a platform is
always at:

```bash
curl -fLO https://github.com/illegalstudio/elephc/releases/download/nightly/elephc-nightly-x86_64-unknown-linux-gnu.tar.gz
```

The other targets are `elephc-nightly-aarch64-apple-darwin.tar.gz` and
`elephc-nightly-aarch64-unknown-linux-gnu.tar.gz`. The asset names are the same
under a dated tag, which is what disambiguates the URL:

```bash
curl -fLO https://github.com/illegalstudio/elephc/releases/download/nightly-20260901/elephc-nightly-x86_64-unknown-linux-gnu.tar.gz
```

The contents and the install layout are identical to a release tarball, so the
same install steps apply.

A nightly identifies the commit it was built from in its version string:

```console
$ elephc --version
elephc 0.26.5-nightly.20260901+g6f7cd55de
```

The build number is the UTC date and the trailing `g<sha>` is the `main` commit.
Note that this string sorts *below* the release it derives from under semantic
versioning — `0.26.5-nightly.20260901` precedes `0.26.5` — even though the code
is newer, and that the `+g<sha>` part is build metadata, which semver ignores
when comparing. Identify a specific build by its commit or by the tarball
checksum rather than by ordering version strings.
