---
title: "Native dependencies"
description: "Declare, lock, install, inspect, prune, and link curated native packages with elephc native."
sidebar:
  order: 9
---

Some generated programs call native C libraries. Elephc manages those libraries
as **curated native packages**: the project declares an exact version, the lock
records immutable catalog metadata, and `elephc native` builds verified static
archives into a target- and toolchain-specific cache.

The catalog contains PCRE2 10.47, zlib 1.3.2, OpenSSL 3.5.8, nghttp2 1.70.0,
libssh2 1.11.1, curl 8.21.0, Oniguruma 6.9.10, and libxml2 2.15.3.
Programs using `preg_*`, `RegexIterator`, or `RecursiveRegexIterator` require
PCRE2 at final link time. Mbstring and opaque eval also require PCRE2 for the
shared output MIME-pattern provider, including with default startup settings.
This dependency does not enable `preg_*` inside opaque eval. zlib is the second
pure-C recipe and proves the manager is not PCRE2-specific; declaring it makes
its verified static artifact available for future runtime/builtin integrations
but does not by itself add `libz.a` to every program. curl has the largest
dependency closure in the catalog: `elephc native add curl` also declares
libssh2 (SCP/SFTP), nghttp2 (HTTP/2), OpenSSL (curl's TLS backend, and
libssh2's crypto backend) and zlib, and `--with-curl` or ordinary detected
`curl_*` usage requires all six archives at final link time.
Dependency declaration order is link order: the resolver walks it depth-first
and splices each package's own dependencies in behind it, so `libssh2.a`
precedes the OpenSSL and zlib archives that satisfy it. See
[Linking and conditional compilation](linking-and-conditional-compilation.md)
for the `--with-curl` flag itself.
libxml2 is the parser behind the `xml` bridge: `--with-xml`, or any program
whose link plan pulls in `elephc_xml`, requires the Elephc-owned
`libelephc_libxml2_shim.a` followed by `libxml2.a` at final link time. It is
built with the platform's iconv (built into glibc; `-liconv` on Apple targets)
and without zlib, ICU, Python, readline, or dynamic modules, so it has no
catalog dependencies of its own. Its complete public header set is retained
so later XML extensions can compile against the same artifact.

`elephc native add oniguruma` installs the pinned Oniguruma library and the
versioned mbregex provider, with the provider archive before `libonig.a` in link
order. The package supports all five compiler targets. The shared mbregex engine
uses this provider for `mb_ereg*` and `mb_split`, including `mb_ereg_match()`.
`--with-mbstring` enables this regex capability for opaque eval and therefore
requires both Oniguruma and PCRE2. Declaring Oniguruma alone does not change a
program's selected runtime.

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

elephc native prune
    [--target TARGET]
```

| Command | Effect |
|---|---|
| `add` | Add one exact catalog package and install it before publishing the manifest and lock. Re-adding the same exact version is idempotent; use `update` to change versions. |
| `install` | Reconcile the lock from the manifest when allowed, then materialize and verify selected target artifacts. |
| `update` | Refresh one package, or every package when no name is given, from the current built-in catalog. |
| `remove` | Remove the declaration and lock entry. Shared cached artifacts are retained. |
| `list` | Read-only status for each declared package: `installed`, `missing`, `corrupt`, `stale`, or `toolchain-error`. |
| `doctor` | Read-only project, lock, cache-size, stale-staging, toolchain, and receipt diagnostics. |
| `prune` | Explicitly remove abandoned staging, catalog-orphan artifacts, and old toolchain fingerprints for the selected target/ABI. |

`--target` accepts the normal supported targets: `macos-aarch64`, `ios-arm64`,
`ios-sim-arm64`, `linux-aarch64`, and `linux-x86_64`. It defaults to the host.
GNU and musl are cache ABI variants derived from the selected C compiler, not
additional public Elephc targets.

`--manifest-path` must name an `elephc.toml` file and disables ancestor
discovery. `--offline` guarantees that no downloader is invoked. `--locked` is
valid only for `install`; it requires an existing lock that exactly matches the
manifest and the catalog and never rewrites it.

`native list` and `native doctor` never use the network and never mutate the
project or cache. `native prune` is the one explicit global-cache cleanup
command; it never changes `elephc.toml` or `elephc.lock`. `native remove` remains
project-only and never silently deletes shared cached data. `elephc native
--help` and each verb's `--help` work without a project.

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

`native add` and `native update` declare a package's catalogued transitive
dependencies directly in the manifest before locking and materializing them.
Lock generation still fails closed with `elephc native add <dependency>` when a
hand-edited manifest omits a required dependency, instead of producing an
incomplete installation.

## Multi-target locks and caches

One committed lock describes **all five supported targets**, but installed
artifacts are keyed and cached **per target, ABI, and toolchain fingerprint**.
Installing on the developer's macOS host therefore does not install either iOS
or either Linux artifact. A compile with only the macOS artifact cached fails
for any of those targets and prints the exact target recovery command.

Install each target artifact explicitly:

```bash
elephc native install --locked --target macos-aarch64
elephc native install --locked --target ios-arm64
elephc native install --locked --target ios-sim-arm64
elephc native install --locked --target linux-aarch64
elephc native install --locked --target linux-x86_64
```

On a non-host target, set all three target C-tool overrides before the command.
The iOS entries require SDK-aware compiler and archive-tool wrappers on a macOS
runner. A minimal GitHub Actions shape for the native-runner subset is:

```yaml
strategy:
  matrix:
    include:
      - { runner: macos-14, target: macos-aarch64 }
      - { runner: ubuntu-24.04-arm, target: linux-aarch64 }
      - { runner: ubuntu-24.04, target: linux-x86_64 }
env:
  ELEPHC_NATIVE_CACHE: ${{ runner.temp }}/elephc-native
steps:
  - run: elephc native install --locked --target "${{ matrix.target }}"
  - run: elephc --target "${{ matrix.target }}" main.php
```

This example deliberately shows the three targets with native CI runners. Add
the two iOS rows on a macOS runner after providing wrappers for the selected
Xcode device or Simulator SDK. Whenever one runner cross-installs another
target, configure `ELEPHC_NATIVE_CC_<TARGET_ENV>`,
`ELEPHC_NATIVE_AR_<TARGET_ENV>`, and
`ELEPHC_NATIVE_RANLIB_<TARGET_ENV>` first.

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
without regex, mbstring, or eval does not link PCRE2 merely because the project
declares it. `--check`, `--emit-ir`, and `--emit-asm` do not perform the final link and
therefore do not require an installed artifact.

Every missing/stale/corrupt state uses the same diagnostic tail. It reports the
discovered project and a command that can be pasted from any directory:

```text
project: /work/app
recovery: cd -- '/work/app' && elephc native install --locked --target linux-x86_64
```

Typical raw recovery actions are:

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

`elephc native doctor` reports the cache's approximate regular-file size plus a
count/size summary and paths for staging or quarantine leftovers. Run:

```bash
elephc native prune
elephc native prune --target linux-x86_64
```

to remove abandoned publication siblings older than 24 hours, artifacts whose
catalog identity no longer exists, and older compiler fingerprints for the
selected target and ABI. Cleanup takes the same per-artifact locks as
installation, so it does not race publication of the same cache key; the
currently selected fingerprint is retained. Source archives remain
content-addressed and reusable.

Downloads use HTTPS and are bounded and hashed before publication. Sources are
gzip- or xz-compressed tarballs; the container format is fixed by the catalog
URL, and xz sources (libxml2) are inflated by a pure-Rust decoder under the
same expanded-size bound before their tar entries are checked. Extraction
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

### Toolchain fingerprint

The fingerprint hashes every effective input that can change produced objects
or archives:

- public Elephc target, compiler-reported tuple, and normalized GNU/musl/macOS ABI;
- selected `cc`, `ar`, and `ranlib` command identities and normalized version
  output (or the executable hash when a tool has no usable version output);
- the selected macOS SDK version from `xcrun`;
- fixed recipe flags such as `CFLAGS=-fPIC`; and
- the allowlisted tool-launch environment (`PATH`, temporary-directory
  variables, `SYSTEMROOT`, and fixed C locale).

Changing Clang/GCC, binutils, Xcode/SDK, a command path, or one of those
allowlisted values intentionally selects a different cache directory. The old
artifact is not ABI-assumed compatible and can later be removed with `native
prune`.

For CI, cache the native root with at least the target and runner/toolchain image
in the outer cache key:

```yaml
- uses: actions/cache@v4
  with:
    path: ${{ runner.temp }}/elephc-native
    key: native-${{ matrix.target }}-${{ runner.os }}-${{ hashFiles('elephc.lock') }}
```

The internal fingerprint still prevents an unsafe hit after a compiler or SDK
bump; the outer key controls restore efficiency rather than compatibility.

## Five dependency mechanisms

These mechanisms solve different problems and are intentionally separate:

| Mechanism | Purpose | Managed by |
|---|---|---|
| Native packages | Curated external C/C++ source built into verified target-specific static archives | `elephc native` + `elephc.toml`/`elephc.lock` |
| Composer packages | PHP source discovered and inlined ahead of time | Composer metadata and Elephc's compile-time autoloader |
| Rust bridge crates | Optional Elephc workspace `staticlib` implementations such as `pdo`, `tls`, `crypto`, or `bcmath` | Feature detection and `--with-<crate>` |
| Runtime capabilities | Optional helpers hidden behind opaque dynamic source, such as regex inside `eval()` | Feature detection and explicit flags such as `--with-regex`; managed packages remain separately declared |
| Toolchains | Assemblers, linkers, C compilers, Make, SDKs, and cross tools | The user or operating system |

The v1 catalog is deliberately runtime/builtin-oriented. It is not a general
system or FFI package manager and does not replace Composer, Cargo, Homebrew,
apt, or a cross-toolchain installer.

In particular, the DOOM renderer, SDL framebuffer, SDL audio, and similar
examples declare C functions with `extern` and link user-installed libraries
with `--link`, `--link-path`, and sometimes `--framework`. Those are **not**
`elephc native` packages. Adding zlib to the curated catalog does not turn
arbitrary `extern "z"` declarations into managed requirements or make raw link
flags satisfy a managed PCRE2 requirement.
