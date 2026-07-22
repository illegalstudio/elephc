# Elephc Native Dependencies v1

- **Status:** Frozen after GLM 5.2 and Kimi K2.7 `ACCEPT` verdicts
- **Reviewed payload:** 1,063 lines, 47,778 bytes, SHA-256 `f9a6dd6610f4886c07c3372ce32b4f6510ae10f567371b3a95be89956c7b882d`
- **Owner:** primary agent
- **Initial package:** PCRE2 10.47
- **Supported targets:** `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## 1. Decision summary

This RFC introduces a curated native-dependency workflow under `elephc native`.
It deliberately separates three operations that are currently conflated:

1. declaring a native package in a project;
2. locking its exact source and recipe;
3. materializing target-specific static artifacts in a local cache.

The frozen user model is:

```text
elephc native add <package>[@<exact-version>]
elephc native install
elephc native update [<package>[@<exact-version>]]
elephc native remove <package>
elephc native list
elephc native doctor
```

`add` declares and installs. `install` materializes target artifacts described
by the project files. Without `--locked` it may derive or repair the lock from
the manifest; with `--locked` it requires an existing lock that exactly matches
the manifest and current catalog and never rewrites it. Ordinary PHP compilation
never downloads or builds a native package and never mutates either project
file.

The implementation is generic at its package/catalog/resolution/link boundary,
but v1 ships exactly one recipe: PCRE2. Composer packages, Rust bridge crates,
toolchains, arbitrary user recipes, arbitrary URLs and prebuilt artifact
distribution are not part of v1.

PCRE2 is source-first. Elephc downloads the exact upstream release archive,
checks its SHA-256, builds static archives for the selected target, and builds an
Elephc-owned opaque C shim against the same headers. The final link uses exact
archive paths in this mandatory order:

```text
libelephc_pcre2_shim.a
libpcre2-posix.a
libpcre2-8.a
```

Generated regex code calls only the versioned shim ABI. It never allocates or
reads `regex_t`, `regmatch_t`, `regoff_t`, or another PCRE2-owned layout.

## 2. Goals

- Give projects a reproducible, explicit way to obtain external C/C++
  dependencies used by generated Elephc programs.
- Make `elephc native add pcre2` sufficient to declare, lock, fetch, verify,
  build and cache PCRE2 for the host target.
- Make `elephc native install --locked` suitable for CI and offline reuse.
- Keep project locks portable while preventing cache reuse across incompatible
  target/libc/toolchain identities.
- Link managed static archives by exact path and preserve the existing Linux
  static-link preference when no genuinely dynamic input requires otherwise.
- Remove Elephc's hard-coded PCRE2 POSIX struct-layout dependency.
- Support every current target in the same change, including the existing
  ARM64 and x86_64 runtime lowering paths.
- Establish a curated catalog/provider boundary that can later host packages
  such as zlib, bzip2, iconv, SQLite or SDL without redesigning the CLI or
  linker.
- Produce actionable, deterministic diagnostics for every missing, stale,
  offline, integrity, toolchain and build state.

## 3. Non-goals

- PHP/Composer dependency management.
- Cargo workspace or `elephc-*` bridge-crate management. Existing
  `--with-<crate>` behavior remains separate.
- Installing compilers, linkers, SDKs, CMake, Make or other build tools.
- Executing recipes supplied by a project, manifest, lockfile or remote index.
- Arbitrary Git repositories, URLs, local paths, package-manager formulas or
  system-library declarations.
- Semver constraints or dependency solving. V1 accepts catalogued exact
  upstream versions only.
- Signed Elephc prebuilt artifacts. The catalog/provider model must leave room
  for them, but v1 builds verified upstream sources locally.
- Byte-for-byte reproducible C archives across different compilers. V1 locks
  source and recipe inputs and records the local toolchain/output identities.
- Fixing unrelated regex semantics or changing embedded-NUL behavior.
- Refactoring the existing Rust bridge auto-build/network behavior. The
  no-network compile guarantee in this RFC applies to native packages; the
  existing bridge behavior is documented as an independent legacy exception.
- Supporting a PCRE2 system fallback. A safe opaque shim and its PCRE2 headers
  and archives are one indivisible artifact in v1.

## 4. Terminology and invariants

- **manifest**: project-owned `elephc.toml`, containing desired exact native
  dependencies.
- **lock**: committed `elephc.lock`, containing catalog-expanded immutable
  source, checksum, recipe and logical link outputs.
- **catalog**: trusted package/version data compiled into the Elephc binary.
- **artifact**: target-specific installed headers/static archives in the global
  native cache.
- **receipt**: local cache metadata binding an artifact to source, recipe,
  target ABI, toolchain and output checksums.
- **requirement**: a logical feature need emitted by the compiler, such as
  `NativePackage("pcre2")`.
- **link item**: a typed exact archive, named system library, search path or
  framework consumed by the linker.

Hard invariants:

1. Compilation never downloads, extracts, configures, builds or publishes a
   native artifact.
2. Compilation never writes `elephc.toml`, `elephc.lock` or the native cache.
3. `--check`, `--emit-ir` and `--emit-asm` never require installed native
   artifacts because they do not perform the final link.
4. A required managed package without a manifest declaration, current lock and
   valid receipt is a hard error with an exact recovery command.
5. No managed package silently falls back to `-l<name>` or a system path.
6. Manifest and lock data can select only recipes embedded in the trusted
   catalog; they cannot inject commands or URLs.
7. A source archive is verified before extraction, and artifact outputs are
   verified before publication and again before use.
8. Cache identity distinguishes macOS from Linux, architecture, GNU from musl,
   and the selected target C toolchain fingerprint.
9. A failed or interrupted install never publishes a partial final artifact.
10. Static link order is data, not an incidental sort; the PCRE2 shim precedes
    the POSIX archive, which precedes the 8-bit archive.
11. Generated regex assembly knows only Elephc's fixed shim ABI and fixed
    `[start, end]` pairs of signed 64-bit integers.
12. Existing user `--link`, `--link-path`, `--framework` and bridge semantics
    remain available and distinct from managed packages.

## 5. CLI contract

### 5.1 Top-level dispatch

The parser returns:

```rust
enum Command {
    Compile(CliConfig),
    Native(NativeCommand),
}
```

Only an exact `args[1] == "native"` selects the native command family.
Otherwise the existing compilation syntax and parser behavior are unchanged.
Flags before `native` are not supported. The only compatibility break is that a
source file literally named `native` must be passed with an explicit relative or
absolute path, for example `./native`.

### 5.2 Commands and flags

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

Rules:

- `TARGET` uses the existing target parser and accepts only a currently
  supported backend. It defaults to the detected host target.
- `--manifest-path` is a path to an `elephc.toml` file, not a directory.
- `--offline` guarantees that the downloader is never invoked. A verified
  source archive already in cache may still be extracted and built.
- `--locked` is valid only for `install`; it rejects an absent lock or any
  manifest/lock mismatch and never rewrites the lock.
- Package names are lowercase ASCII catalog identifiers. Unknown names fail and
  list the known packages.
- Versions are exact catalogued upstream versions. `^`, `~`, ranges, wildcards,
  tags and arbitrary URLs fail.
- Omitting a version in `add` or `update` selects the catalog's current default.
- Adding the same exact dependency is idempotent. Adding the same package at a
  different version instructs the user to use `update`.
- `update` without a package recomputes every manifest dependency from the
  current catalog, including source metadata, recipe revision, dependency list,
  provides set and target link plan, even when its version string is unchanged.
  With a package, it applies that complete expansion only to that package; an
  omitted version selects the current catalog default.
- `remove` changes manifest and lock but never deletes the shared global cache.
- `list` and `doctor` never network and never mutate project or cache state.
- Success exits 0; usage, validation, integrity, toolchain and build errors exit
  non-zero. Diagnostics go to stderr; stable human-readable status goes to
  stdout.
- `elephc native --help` and `<verb> --help` print the relevant synopsis and
  exit 0 without project discovery. Bare `elephc native` is a usage error after
  printing the native synopsis. V1 has no JSON output contract.
- `list` prints one deterministic row per manifest dependency with package,
  manifest version, locked version, selected target, selected ABI and one of
  `installed`, `missing`, `corrupt`, `stale` or `toolchain-error`. When the ABI
  cannot be resolved it prints `unresolved`, retains the manifest/lock columns
  and exits non-zero after all rows. No manifest prints an empty-list message
  and succeeds.
- `doctor` reports project discovery, manifest/lock consistency, cache-root
  availability, selected toolchain tuple and every declared package receipt.
  It exits non-zero when any check is broken and succeeds with an explicit
  `healthy` summary otherwise. It remains read-only.

### 5.3 Command state transitions

`add` and `update` use this order:

1. acquire the project mutation lock;
2. calculate the desired manifest and lock entirely in memory;
3. validate catalog/target/toolchain before network access;
4. install and verify the desired target artifact;
5. atomically replace `elephc.toml` and then `elephc.lock`;
6. release the lock.

Installation occurs before project-file publication, so an ordinary download or
build failure leaves both files unchanged. Atomic replacement is per-file; a
process crash between the two renames can expose a detectable mismatch.
`install` without `--locked` repairs that mismatch from the manifest. All other
consumers reject it rather than guessing.

`install` uses this order:

1. discover project paths without reading manifest or lock contents;
2. when lock reconciliation is allowed, acquire the project mutation lock
   before reading `elephc.toml` or `elephc.lock` and retain it through any lock
   publication; `--locked` never takes this mutation lock because it cannot
   write project state;
3. read/parse the project, then derive the lock from the catalog or validate the
   existing lock;
4. acquire a per-artifact cache lock;
5. reuse a fully verified receipt or materialize into a unique staging sibling;
6. verify every output and atomically rename staging to the final cache path;
7. atomically publish a reconciled lock when not in `--locked` mode.

`remove` atomically publishes the manifest without the package, then the lock
without the package. No install/cache mutation is necessary. Every project-file
publication writes a sibling temporary file, flushes it, and renames it on the
same filesystem.

### 5.4 Project discovery

- Compilation absolutizes the PHP source relative to the current directory,
  starts at its parent and selects the nearest ancestor containing
  `elephc.toml`.
- Native commands start from the current directory and select the nearest
  ancestor containing `elephc.toml`.
- `native add` uses the ancestor manifest found by that walk. When none exists,
  it creates `./elephc.toml` and treats the current directory as the new project
  root. Its success message prints that absolute root and warns when the source
  path mentioned by a recovery diagnostic would not discover it as an ancestor.
- An explicit `--manifest-path` disables discovery.
- Symlink/canonicalization errors are reported; discovery must not silently
  switch to a different ancestor.
- A project manifest is not mandatory for programs that need no managed native
  package. A regex requirement without one is a hard error suggesting
  `elephc native add pcre2`.

## 6. Project files

### 6.1 Manifest

Minimal v1 syntax:

```toml
[native]
schema = 1

[native.dependencies]
pcre2 = "10.47"
```

`toml_edit` is used so edits preserve comments, formatting and unrelated
top-level sections. The native section is strict: dependency values must be
exact-version strings and duplicate/case-variant keys fail. Unknown top-level
sections and keys outside `[native]` are retained for forward compatibility.
`native.schema` is required and must equal 1. Unknown keys inside `[native]`
fail in v1.

### 6.2 Lockfile

`elephc.lock` is deterministic TOML, schema version 1, generated with a
do-not-edit preamble. Packages and dependencies are sorted lexicographically;
link items retain catalog order.

Normative logical shape:

```toml
schema = 1

[[package]]
name = "pcre2"
version = "10.47"
recipe = 1
dependencies = []
provides = ["pcre2"]

[package.source]
url = "https://github.com/PCRE2Project/pcre2/releases/download/pcre2-10.47/pcre2-10.47.tar.gz"
sha256 = "c08ae2388ef333e8403e670ad70c0a11f1eed021fd88308d7e02f596fcd9dc16"
size = 2792969

[[package.target]]
name = "macos-aarch64"
archives = [
  "lib/libelephc_pcre2_shim.a",
  "lib/libpcre2-posix.a",
  "lib/libpcre2-8.a",
]
system_libraries = []
frameworks = []
```

Equivalent target entries exist for `linux-aarch64` and `linux-x86_64`. The lock
contains no absolute path, host cache directory, environment override or local
compiler output. A lock is stale when any exact manifest version, source URL,
source size/SHA-256, recipe revision, transitive dependency list, provides set
or supported-target link plan differs from the current catalog expansion.

Unknown lock schemas and unknown fields fail closed. This strictness is safe
because Elephc owns the generated lockfile.

### 6.3 Local receipt

Each published artifact contains a deterministic JSON receipt with:

- schema version;
- package/version/recipe/source SHA-256;
- Elephc target;
- target C ABI triple from the selected compiler, including GNU or musl;
- compiler, archiver and ranlib command identities;
- a SHA-256 fingerprint of tool version output and target tuple;
- relative output path, size and SHA-256 for every header/archive used;
- creation tool version for diagnostics only, not cache compatibility.

Absolute tool paths may be recorded for diagnostics but are not trusted as an
identity. Compilation recomputes the selected toolchain fingerprint, locates the
exact cache key, parses the receipt and verifies required output sizes and
SHA-256 values before linking. Fingerprint computation and receipt/output
validation may be cached in memory for one Elephc process invocation only.

Receipt schema changes whenever a field used for cache identity or output
verification changes. Compilation rejects an unrecognized receipt schema as a
hard missing/incompatible-artifact error and prints
`elephc native install --locked --target <target>` as the recovery command.

## 7. Catalog and PCRE2 recipe

### 7.1 Catalog boundary

The binary owns a static catalog:

```rust
PackageSpec {
    name,
    default_version,
    versions: &[PackageVersion],
}

PackageVersion {
    version,
    source: SourceArchive { https_url, sha256, size_limit },
    recipe_revision,
    dependencies,
    supported_targets,
    ordered_link_outputs,
    provides,
}
```

The manifest selects only `name` and exact `version`. Every executable recipe,
URL, checksum and output path comes from the compiled catalog. Adding a future
package means adding a catalog entry and a reviewed built-in recipe; it does not
mean accepting project-supplied shell commands.

### 7.2 PCRE2 v1 catalog entry

- Package: `pcre2`
- Default and only initial version: `10.47`
- Upstream archive and SHA-256: exactly as shown in the lock example
- Recipe revision: `1`
- Provides: `pcre2`
- Supported targets: all three current targets
- Required tools: POSIX shell, Make, target C compiler, `ar`, `ranlib`
- Ordered outputs: shim, POSIX archive, 8-bit archive

The catalog snapshot was checked against the official GitHub release asset: the
download is 2,792,969 bytes with the stated SHA-256. Its tar contains 480 entries,
only regular files/directories, 13,307,947 expanded bytes, a maximum 649,735-byte
file and a maximum 77-byte path. Those facts are regression assertions
for the v1 extraction policy, not mutable network observations at install time.

### 7.3 Source build

The release archive is configured in a disposable extracted/build tree. The
normative configure feature set is static-only, PIC, 8-bit, Unicode and POSIX,
with shared libraries, 16/32-bit libraries and JIT disabled. The implementation
passes the corresponding explicit upstream flags and `CFLAGS=-fPIC` rather than
relying on defaults. For cross builds it also passes `--host=<validated target
tuple>`. It never invokes autoconf.

The recipe invokes Make only for the required `libpcre2-8.la` and
`libpcre2-posix.la` targets; it does not build/run PCRE2 tests or install
`pcre2grep`/`pcre2test`. It copies the resulting `.a` files and generated public
headers into the staging prefix under catalog-declared paths, then verifies
them. It does not run a broad `make install` target.

Source installation executes upstream build code with the user's permissions;
v1 does not claim a portable network/filesystem sandbox that Rust cannot enforce
equally on macOS and Linux. Its trust root is the reviewed catalog embedded in
the installed Elephc binary plus HTTPS PKI and the exact catalog SHA-256. The
operation is explicit (`native add`/`install`/`update`), uses a minimal scrubbed
environment, confines all recipe-selected build/output paths to unique staging, and never runs
manifest- or lock-supplied commands. Build stdout/stderr identify the trusted
package and recipe. Strong OS sandboxing and independently signed prebuilds are
separate future providers, not a v1 promise.

The build environment is allowlisted, not inherited. User `CC`, `CXX`,
`CFLAGS`, `CXXFLAGS`, `CPPFLAGS`, `LDFLAGS`, `LIBS`, `AR`, `RANLIB` and
`MAKEFLAGS` are unset or replaced by the selected toolchain and catalog recipe.
Only the minimal OS variables required to start tools, explicit recipe values
and resolved `ELEPHC_NATIVE_*` overrides can influence `configure`/Make. Every
effective build variable used in output production participates in the recipe
revision or toolchain fingerprint.

The recipe retains only catalog-declared headers and static archives. It rejects
missing, symlinked or unexpected-type outputs. Archives must be non-empty and
recognized by the target archiver/toolchain. All C objects, including the shim,
are compiled as position-independent code so the same artifact can link into an
executable or `--emit cdylib` output.

After PCRE2 installation, the recipe writes the embedded Elephc shim source to
staging, compiles it with the same target compiler and the staged PCRE2 headers
using `PCRE2_STATIC`, then creates `libelephc_pcre2_shim.a` with the selected
archiver/ranlib. The shim source is embedded in the Elephc binary via
`include_str!` or `include_bytes!`; installed Elephc releases must not depend on
the source repository being present.

### 7.4 Toolchain selection

The public Elephc target matrix remains exactly `macos-aarch64`,
`linux-aarch64` and `linux-x86_64`; GNU and musl are artifact ABI variants, not
new PHP/compiler target names. Multiple ABI-qualified artifacts may coexist for
one Linux target. Host selection uses the libc environment of the running
Elephc binary. Cross selection uses the explicitly selected compiler tuple.

Commands are resolved in this order:

1. target-specific overrides;
2. unsuffixed overrides;
3. `cc`, `ar` and `ranlib`, but only for the host target.

The override names are:

```text
ELEPHC_NATIVE_CC_<TARGET_ENV>
ELEPHC_NATIVE_AR_<TARGET_ENV>
ELEPHC_NATIVE_RANLIB_<TARGET_ENV>
```

where `TARGET_ENV` is the uppercase target name with `-` replaced by `_`, for
example `LINUX_AARCH64`. The unsuffixed overrides are
`ELEPHC_NATIVE_CC`, `ELEPHC_NATIVE_AR` and `ELEPHC_NATIVE_RANLIB`.

For a non-host target, the effective compiler, archiver and ranlib must all come
from target-specific or unsuffixed overrides; host defaults are forbidden. For
every target, the compiler-reported tuple must match the requested OS and
architecture, and `ar`/`ranlib` must accept objects from that compiler. For a
Linux cross target, the tuple must also identify GNU or musl. A missing,
mismatched or ambiguous cross toolchain fails before download or project
mutation.

Some host compilers report only OS/architecture. For the host target only, such
an otherwise matching tuple inherits the running Elephc binary's compile-time
`target_env` (`gnu` or `musl`); it never guesses from a generic Linux string for
a cross target. The effective ABI identity is stored separately from `Target`
and is included in the cache key and receipt. On macOS, the normalized stdout of
`xcrun --sdk macosx --show-sdk-version` participates in the fingerprint; failure
to query the SDK selected by the existing linker is a toolchain diagnostic.

## 8. Cache, concurrency and supply-chain controls

### 8.1 Cache locations

Factor a common Elephc cache-root helper from the existing runtime-cache logic:

1. `ELEPHC_NATIVE_CACHE` for the native cache when set;
2. otherwise `$XDG_CACHE_HOME/elephc/native`;
3. otherwise `$HOME/.cache/elephc/native`.

If none is available, native commands and managed-package resolution fail with
an actionable cache-root diagnostic. The OS temporary directory is never a
durable/shared cache fallback; process-local temporary data must still be
created on the same filesystem as its atomic publication destination.
Every selected cache root is lexically absolutized once against the invocation
working directory before locks, staging paths or receipts are constructed.

The runtime object cache retains its current external behavior.

Source archives are content-addressed by SHA-256. Artifact directories are
keyed by package, version, recipe, source SHA, Elephc target, compiler ABI tuple
and toolchain fingerprint. Paths are assembled from validated components only.

### 8.2 Download policy

- Use an in-process Rust HTTPS client with Rustls/web PKI roots; do not shell out
  to `curl`, `wget` or a platform package manager.
- Only HTTPS catalog URLs and HTTPS redirects are accepted.
- At most five redirects are followed. Connect timeout is 30 seconds, per-read
  timeout is 60 seconds, total request time is five minutes, and the PCRE2
  response body is capped at 32 MiB.
- Stream to a uniquely named temporary file while hashing; require the exact
  catalog SHA-256 before atomic publication in the source cache.
- A checksum or size failure removes/quarantines the temporary file and never
  reaches extraction.
- `--offline` checks the content-addressed source cache and otherwise fails with
  an actionable message.

Production downloader and recipe execution are behind injected traits so tests
can prove call order and offline behavior without adding a catalog override or
performing network access.

### 8.3 Safe extraction

Only regular files and directories below exactly one archive root are allowed.
Reject absolute paths, `..`, platform prefixes, path escapes, device entries,
FIFOs, sockets, hard links and all symlinks. V1's sole catalog archive is
validated against that policy; a future catalog recipe that legitimately needs
links must define and test a package-specific safe normalizer rather than
weakening this default.

Before/during expansion enforce at most 50,000 entries, 256 MiB total expanded
bytes, 4,096 bytes per entry path, 64 MiB per regular file and a 100:1 total
expanded-to-compressed ratio. Strip exactly the single common top-level
directory component. Every resulting path remains relative and below staging.
A violation reports the offending entry, fails closed and discards only that
unique staging directory.

### 8.4 Locking and publication

- Advisory project lock under `native/locks/project/<sha256(canonical manifest
  path)>.lock`: serializes manifest/lock mutation without adding lock debris to
  the repository.
- Advisory source lock under `native/locks/source/<source-sha256>.lock`:
  serializes download publication for one content address.
- Advisory artifact lock under `native/locks/artifact/<artifact-key>.lock`:
  serializes installation for the exact target ABI/toolchain identity.
- Lock acquisition polls for at most 30 seconds, then fails with the exact lock
  path and owning-process metadata when available. On acquisition the holder
  writes PID, command kind and start timestamp for diagnostics. It never
  proceeds unlocked; a persistent lock file without an active advisory lock is
  reusable and not itself evidence of a live owner.
- Unique staging directories are siblings of the final artifact directory.
- Receipt and output verification complete inside staging.
- When no final artifact exists, publication is an atomic staging-to-final
  rename on the same filesystem. A valid existing artifact is reused. Under the
  artifact lock, an invalid existing final directory is first renamed to one
  exact-key quarantine sibling, then verified staging is renamed to final; a
  failed second rename attempts to restore the quarantine. Resolution sees a
  complete valid final artifact or a missing/error state, never staging or
  quarantine as usable output.
- If another process publishes first, the loser discards its staging directory
  only after verifying the winner's receipt.
- Stale staging directories are always ignored by resolution and reported by
  read-only `doctor`. An installer holding the matching per-artifact lock
  removes only invalid staging siblings older than 24 hours before beginning
  new work. Each command removes its own staging directory on handled failure;
  no broad recursive cache deletion is performed.

## 9. Opaque PCRE2 shim ABI

### 9.1 Exported C symbols

The shim exports exactly these versioned symbols:

```c
int32_t elephc_pcre2_v1_compile(
    void **handle_out,
    const char *pattern_z,
    uint32_t cflags,
    uint64_t *match_slot_count_out
);

int32_t elephc_pcre2_v1_exec(
    void *handle,
    const char *subject_z,
    uint64_t requested_slots,
    int64_t *offset_pairs,
    uint32_t eflags
);

void elephc_pcre2_v1_free(void *handle);
```

No PCRE2-owned type appears at the boundary.

### 9.2 Behavioral contract

- The implementation continues to use `pcre2_regcomp`, `pcre2_regexec` and
  `pcre2_regfree` internally. Migration to the native PCRE2 API is out of scope.
- `compile` returns 0 on success. When both output pointers are valid, any
  failure returns a non-zero code, stores null in `handle_out` and zero in
  `match_slot_count_out`.
- Null `handle_out`, null `match_slot_count_out`, null pattern, null handle or
  null subject are non-zero failures without dereference. A null offset buffer
  is valid only when `requested_slots == 0`. Allocation failure is non-zero.
- A successful `match_slot_count_out` is `re_nsub + 1`, including the full
  match. Overflow is rejected.
- `exec` returns 0 for a match and non-zero for no match or an error, preserving
  current runtime branching.
- `offset_pairs` is `requested_slots` contiguous `[start, end]` pairs. Each
  field is a signed 64-bit byte offset relative to `subject_z` for that call.
- Before execution every pair is initialized to `[-1, -1]`. Unmatched captures
  and slots beyond the compiled capture count retain that value.
- Results are truncated when fewer slots are requested. Requesting zero slots
  is valid and permits a null `offset_pairs`.
- Validate `re_nsub + 1`, allocation sizes, `uint64_t -> size_t` conversions and
  multiplication by 16 before allocation or writes.
- `free(NULL)` is a no-op.
- Pattern and subject remain NUL-terminated C strings to preserve current
  embedded-NUL behavior.
- Handles contain no global state. Concurrent use of distinct handles is safe;
  concurrent calls on one handle are not promised in v1.

### 9.3 Runtime migration

All direct PCRE2 calls in these runtime emitters are replaced by shim calls for
both ARM64 and Linux x86_64 paths:

- `mb_ereg_match.rs`
- `preg_match.rs`
- `preg_match_all.rs`
- `preg_replace.rs`
- `preg_replace_callback.rs`
- `preg_split.rs`

The existing pattern stripping, C-string conversion, locale setup, PHP result
construction, match-empty progression, replacement expansion and SPL feature
detection stay semantically unchanged. Do not opportunistically alter the
existing locale behavior of `preg_replace_callback` in this work.

All capture storage uses a fixed 16-byte Elephc pair. Remove the regex-specific
layout helpers and tests from `src/codegen_support/platform/target.rs` and
`platform/mod.rs`:

- `regex_t_size`
- `regex_re_nsub_offset`
- `regmatch_t_size`
- `regmatch_rm_eo_offset`
- `regoff_load_instr`

Retain unrelated `dirent` and `glob` platform layout assertions.

Assembly emitted or modified by this migration must remain target-aware and
must satisfy the repository's column-81 inline-comment policy.

## 10. Compiler and linker integration

### 10.1 Typed requirements

Replace regex's raw library-name emission with a logical typed requirement:

```rust
enum LinkRequirement {
    NativePackage(&'static str),
    Bridge(&'static str),
    SystemLibrary(String),
}
```

`RuntimeFeatures::regex` produces `NativePackage("pcre2")`. Existing bridge,
PHAR compression, extern and explicit CLI requirements keep their current
meaning but are classified by origin. Catalog resolution occurs in the pipeline
after runtime-feature discovery and only on paths that perform a final link.
The linker never reads project files and never installs packages.

Declaring a package makes it available; it does not force-link it. A project
that declares PCRE2 but compiles a non-regex program does not add PCRE2 archives
to the link.

### 10.2 Link plan

Introduce a reusable typed plan, exposed to the test harness:

```rust
struct LinkPlan {
    ordered: Vec<LinkItem>,
    linux_mode: LinuxLinkMode,
}

enum LinkItem {
    StaticArchive { path: PathBuf, whole_archive: bool, origin: LinkOrigin },
    NamedLibrary { name: String, origin: LinkOrigin },
    SearchPath(PathBuf),
    Framework(String),
}

enum LinuxLinkMode {
    Static,
    Dynamic { reasons: Vec<String> },
}
```

Exact managed PCRE2 archives do not force dynamic Linux linking: the planner
preserves the current `-static` preference when every input is classified as
static-compatible. A named user or extern library does force dynamic mode.
Existing bridge behavior remains conservative and can force the current dynamic
mode where required by bridge/platform dependencies. `Dynamic.reasons` is
diagnostic provenance for that command choice, not a second independent linker
policy. A toolchain that rejects the rendered static link produces the normal
explicit linker error; v1 does not silently retry dynamically. Whole-archive
markers remain scoped to the relevant bridge archive. macOS does not add
Homebrew paths merely because exact managed archives are present.

The PCRE2 archive order is never sorted or deduplicated across boundaries. The
shim is first, then POSIX, then 8-bit. Existing explicit search paths,
frameworks, bridge discovery and user link ordering remain stable.

### 10.3 Linker organization and errors

Because the current linker mixes unrelated responsibilities, split it while
preserving behavior:

- `src/linker/mod.rs`: public facade and orchestration;
- `src/linker/bridges.rs`: bridge registry/discovery/auto-build;
- `src/linker/command.rs`: target command rendering and execution;
- `src/linker/archive_dedup.rs`: macOS archive-member handling;
- `src/linker/sdk.rs`: SDK lookup.

Missing bridges and managed artifacts return structured errors rather than
falling through to a cryptic named-library link. Keep the bridge table
authoritative and table-driven.

Adapt the duplicated test runner so link-plan rendering is shared or explicitly
translated through the same types. It must exercise the shim archive and managed
archive ordering rather than continuing to test only raw `-lpcre2-*` behavior.

### 10.4 Compile diagnostics

For a final-link regex program:

- no project manifest:
  `regex support requires managed native package pcre2; run elephc native add pcre2`;
- dependency missing from manifest:
  `project does not declare required native package pcre2; run ...`;
- lock absent/stale:
  `native lock is missing or stale; run elephc native install` and mention that
  `install --locked` is the CI check that would deliberately reject this state;
- receipt/artifact absent:
  include selected target/ABI and
  `elephc native install --locked --target <target>`;
- corrupt receipt/output:
  report the exact bad relative output and instruct reinstall; never fall back.

Raw linker flags are not an override for a missing managed PCRE2 requirement.
They remain available for unrelated FFI libraries.

## 11. Module boundaries

Expected new production modules:

```text
src/native_deps/
  mod.rs          orchestration and injected services
  cli.rs          native subcommand parsing
  project.rs      root discovery and project paths
  manifest.rs     typed/comment-preserving manifest access
  lockfile.rs     deterministic schema and consistency checks
  catalog.rs      trusted static package registry
  requirements.rs logical requirement types
  resolver.rs     project + lock + receipt to exact link items
  cache.rs        roots, keys, locks, staging, receipts
  download.rs     bounded HTTPS downloader abstraction
  archive.rs      safe tar.gz extraction
  toolchain.rs    selection, target validation and fingerprint
  recipe.rs       curated recipe dispatch
  recipes/
    pcre2.rs
    pcre2_shim.c  embedded opaque shim source
  doctor.rs       read-only project/tool/cache diagnosis

src/link_plan.rs  reusable typed link representation
src/linker/       split existing linker implementation
```

Orchestration files must stay slim. Every repo-owned Rust file has the required
module preamble; every explicit Rust function, including tests, has a specific
Rustdoc docblock. No new multi-purpose leaf should become a miscellaneous
bucket.

Recommended direct dependencies are a comment-preserving TOML editor, `serde`
derive, SHA-256, safe tar handling, an advisory-file-lock crate and a blocking
Rustls-backed HTTP client. `flate2` is already direct. Dependencies must be
declared directly rather than relied on transitively, use Rustls rather than
OpenSSL for the downloader, and avoid an async runtime for this synchronous CLI.

## 12. Test strategy

Normal tests are deterministic and network-free. Download and recipe services
are injected. No production environment variable may replace the catalog or
disable checksum validation.

### 12.1 CLI and project tests

- every verb, flag, missing value, invalid combination and exact-version rule;
- unchanged legacy compile parsing;
- source-relative nearest-ancestor discovery versus native cwd discovery;
- explicit `--manifest-path`;
- `add` project creation and same-version idempotence;
- failed add/update leaves project files byte-identical;
- mismatch detection and install repair;
- locked/offline diagnostics;
- deterministic `list` and read-only `doctor`.

### 12.2 Manifest, lock and catalog tests

- preserve manifest comments/unrelated sections;
- reject invalid native values and unknown native keys;
- deterministic lock rendering and package sorting;
- reject unknown schema/fields and every stale catalog dimension;
- exact official PCRE2 URL, size and SHA fixture;
- archive order for all three targets;
- unknown package/version diagnostics.

### 12.3 Cache/security tests

- cache-root precedence;
- distinct GNU/musl and toolchain-fingerprint keys;
- offline never calls downloader;
- size/checksum failure prevents extraction/build/publication;
- reject absolute, parent, symlink, hardlink and special tar entries;
- failed build leaves no resolvable artifact;
- receipt/output corruption is detected before linking;
- concurrent installers publish one verified artifact;
- interrupted staging is ignored;
- atomic project/cache publication behavior.

Use a tiny local tar/configure/Makefile fixture for recipe integration. The real
official PCRE2 download/build path runs only in the dedicated three-target CI
smokes defined below, never in a mandatory unit test dependent on outbound
network.

### 12.4 Shim tests

Compile the embedded shim against aligned PCRE2 headers/libs in the platform test
provider and verify:

- `(a)?(b)` on `b` returns three slots with `[0,1]`, `[-1,-1]`, `[0,1]`;
- requesting 100 slots keeps surplus pairs at `[-1,-1]`;
- invalid pattern returns non-zero, null handle and zero slots;
- empty match returns zero offsets;
- zero requested slots;
- `free(NULL)`;
- overflow/invalid-pointer guards that are testable without undefined behavior.

### 12.5 Linker tests

- exact shim/POSIX/8-bit order on macOS and both Linux architectures;
- exact managed archives alone retain static Linux mode;
- named user/extern libraries produce explicit dynamic reasons;
- whole-archive bridge markers are correctly bounded;
- exact archives do not add Homebrew search paths;
- explicit user paths/frameworks/order remain stable;
- production and codegen-test planning cannot diverge silently.

### 12.6 Regex regressions

Run at least these focused existing filters on macOS ARM64, Linux x86_64 and
Linux ARM64:

- `test_mb_ereg_match_start_anchored`
- `test_preg_match_populates_matches_beyond_ninety_nine`
- `test_preg_match_unmatched_interior_capture_is_empty`
- `test_preg_match_all_count`
- `test_preg_replace_two_digit_backreferences`
- `test_preg_replace_unmatched_capture_backreference_is_empty`
- `test_preg_replace_callback_capture_groups_beyond_ninety_nine`
- `test_preg_replace_callback_unmatched_interior_capture_is_empty`
- `test_preg_split_delimiter_capture_beyond_ninety_nine`
- `test_preg_split_limit_delimiter_and_offset_capture`
- `test_preg_match_unicode_property_letter`
- `test_regex_iterator_split_keeps_delimiter_captures_beyond_ninety_nine`
- `test_regex_program_after_dead_strip`

Add invalid-pattern regression coverage for each affected runtime family. Keep
PHP-visible behavior identical.

The normal network-free in-process/codegen test provider builds the embedded
shim source against the headers and libraries from the same CI-installed system
PCRE2 package, so the shim's private POSIX layout is aligned with the library it
calls. This provider tests runtime ABI migration and PHP semantics only; it is
explicitly test-only and cannot be selected by the production compiler.

A separate managed-PCRE2 smoke on each supported target materializes the exact
catalogue 10.47 source/SHA/recipe into an empty isolated cache, compiles and runs
the regex example through the production resolver/linker, then repeats
`install --locked --offline`. That gate tests the production package path and
must not use system PCRE2 archives. The Linux Dockerfiles add `make`; system
`pcre2-dev` remains only for the fast network-free runtime provider until that
provider is replaced by a checked-in, legally reviewed pinned fixture.

### 12.7 Focused verification gates

- `cargo build` warning-free;
- focused native/CLI/linker unit and integration tests;
- focused regex tests on the host;
- focused Linux x86_64 and ARM64 filters through the existing scripts;
- assembly-comment checker for every modified emitter;
- `git diff --check`;
- one local macOS `native add pcre2` -> clean-cache source build -> regex
  compile/run -> offline reinstall verification, plus the three target-specific
  managed smokes described above in CI;
- CI remains the final complete target matrix; do not run unrelated full local
  suites unless focused evidence proves insufficient.

## 13. Documentation and example

Update:

- add `docs/compiling/native-dependencies.md` and index it;
- `docs/compiling/cli-reference.md`;
- `docs/compiling/linking-and-conditional-compilation.md`;
- `docs/getting-started/installation.md` with required build tools;
- `docs/php/regex.md`, removing the future-command text and system-PCRE2
  instructions;
- `docs/php/spl.md`, `docs/php/strings.md`, `docs/php/system-and-io.md` references;
- `docs/internals/the-runtime.md` for the opaque shim;
- relevant architecture/README/roadmap/changelog text without claiming native
  packages cover Composer, Rust bridges or toolchains.

Update the existing `examples/date-json-regex/` rather than creating a redundant
PHP example. Its README or adjacent documentation must show:

```bash
cd examples/date-json-regex
elephc native add pcre2
elephc main.php
./main
```

Commit `examples/date-json-regex/elephc.toml` and its deterministic
`elephc.lock` so the example is a reproducible managed-native project. Keep the
global artifact cache out of the repository; the example `.gitignore` continues
to cover only generated compiler outputs, not its manifest or lock.

## 14. Delivery lots and ownership

Implementation begins only after this RFC has no unresolved blocking objection
from both padawan jurors. First implementation is delegated in non-overlapping
lots:

1. **Core manager:** catalog, project/manifest/lock, toolchain/cache,
   download/extraction, recipe, receipts, native command implementation and unit
   tests. It does not wire the top-level compile pipeline.
2. **CLI/linker integration:** top-level `Command`, pipeline requirement
   resolution, typed link plan, linker split/rendering, test-runner adaptation
   and focused tests. It consumes the core manager's public contracts.
3. **PCRE2 ABI migration:** embedded C shim, six runtime emitter migrations,
   platform-layout removal, shim provider tests and focused regex regressions.
4. **Integration/documentation:** reconcile seams, Docker/CI focused plumbing,
   end-to-end CLI tests, example and documentation.

Each lot has one writer. Shared-file changes are sequenced, not concurrently
edited. The primary agent audits each diff and delegates corrections back to
the responsible writer with a tightened contract; it does not silently repair
first implementations itself.

## 15. Exit criteria

The feature is complete only when all of the following are true:

- From an empty cache in a project, `elephc native add pcre2` creates/updates a
  comment-preserving manifest, deterministic lock and verified host artifact.
- `elephc native install --locked --offline` succeeds with cached verified
  source/artifact state and provably performs no network call.
- A cache miss during ordinary regex compilation performs no installation and
  reports the exact recovery command.
- A regex program links exact shim + PCRE2 archives and runs without a system
  PCRE2 package in the production path.
- Non-regex programs neither require nor link PCRE2 even when declared.
- Generated runtime code contains no direct `pcre2_reg*` call and no PCRE2
  layout constant.
- The link plan preserves the existing Linux `-static` preference for managed
  PCRE2-only programs, never silently retries a failed static link dynamically,
  and preserves existing bridge/user-link behavior.
- GNU and musl artifacts cannot collide or be selected for one another.
- Interrupted, concurrent, corrupt, checksum-failing and offline paths meet the
  invariants above.
- Focused regex behavior is green on macOS ARM64, Linux x86_64 and Linux ARM64.
- All changed Rust functions/files meet documentation policy; all changed
  assembly emissions meet the comment policy; focused build/tests and
  `git diff --check` pass.
- User-facing docs no longer claim that the command is hypothetical and clearly
  distinguish native packages, Composer packages, Rust bridges and toolchains.
- Both GLM and Kimi final reviews report no blocking contract or implementation
  issue, and every accepted improvement has been applied and reverified.

## 16. Adversarial review decisions

Round-one GLM and Kimi objections were resolved as follows:

- **Accepted:** manifest-first project publication; explicit `install`
  reconciliation semantics; full catalog refresh on `update`; unknown receipt
  schemas fail closed; durable cache root required; numerical archive bounds;
  deterministic lock timeout; host/cross override precedence; in-process
  validation caching; manifest schema; SDK-aware toolchain fingerprint; exact
  target in missing-artifact recovery diagnostics.
- **Accepted in convergence round:** a reconciling `install` locks the project
  before reading file contents, and recipe processes receive an allowlisted
  environment with user compiler/linker flags removed or fingerprinted.
- **Public Linux targets remain unchanged:** the repository defines three
  first-class compiler targets. GNU/musl is a second artifact-ABI identity
  derived from the effective C toolchain, not a fourth/fifth user-facing target.
  This resolves collision without changing PHP codegen target semantics.
- **All symlinks remain rejected in v1:** the pinned PCRE2 archive must pass the
  strict policy. Future curated packages can add a reviewed package-specific
  normalizer without expanding v1's attack surface.
- **No fictitious portable build sandbox:** v1 explicitly trusts the catalogued
  source digest and runs it only on an explicit native command with a scrubbed
  environment/staging prefix. Claiming that a portable Rust CLI can block all
  filesystem/network access on every supported OS would be a false guarantee.
- **`doctor` stays read-only:** the installing command holding the exact
  artifact lock owns narrowly scoped stale-staging cleanup. This preserves the
  stated read-only contract while bounding abandoned staging.
- **Tests use two providers intentionally:** a system-header-aligned shim keeps
  the broad semantic suite fast and network-free; a separate production-path
  managed 10.47 smoke on every supported target proves the catalog recipe,
  resolver and exact archives. Neither creates a production system fallback.
- **Linux mode preserves current behavior:** managed static archives do not
  cause the current planner to drop `-static`; linker failure remains explicit
  and is never hidden by a dynamic retry.

No item above is left to implementation discretion. A later reviewer must
challenge the stated decision and invariant rather than reintroduce the rejected
alternative as an implicit default.

## 17. Explicit post-v1 extensions

The design intentionally leaves these compatible additions:

- signed Elephc prebuilt artifacts as a provider selected by the same lock and
  verified before cache publication;
- multiple catalogued versions and transitive native package dependencies;
- mapping reviewed `extern "library"` declarations to a package `provides` set;
- more curated packages such as zlib, bzip2, iconv, SQLite or SDL;
- cache garbage collection and explicit reinstall commands;
- a separately designed safe system provider where a package-specific ABI
  boundary makes that possible.

None of these may weaken v1's no-script, exact-lock, checksum, cache-identity,
transactional-publication or no-compile-install invariants.
