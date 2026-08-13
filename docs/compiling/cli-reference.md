---
title: "CLI reference"
description: "The complete, authoritative list of every elephc command-line flag, its accepted values, default, and environment-variable override."
sidebar:
  order: 3
---

This page lists every compiler flag and native-package subcommand the `elephc`
command accepts. Topical pages
([optimization](optimization.md), [output](output-and-diagnostics.md),
[linking](linking-and-conditional-compilation.md), [native
dependencies](native-dependencies.md)) explain the *why*; this page is the
exhaustive *what*.

## Synopsis

```text
elephc [OPTIONS] <source-file>
elephc native <COMMAND> [OPTIONS]
```

Exactly one positional argument is required: the path to tagged `.php` or
tagless `.lfc` source. The binary is written next to it, named after the source
without its extension.
Only an exact first argument of `native` selects the package command family. A
source file literally named `native` must therefore be passed as `./native` or
by another explicit path.

## Native dependency commands

| Command | Arguments and flags | Description |
|---|---|---|
| `native add` | `<package>[@<exact-version>] [--target TARGET] [--offline] [--manifest-path FILE]` | Declare, lock, and install one catalog package. |
| `native install` | `[--target TARGET] [--locked] [--offline] [--manifest-path FILE]` | Materialize declared artifacts; without `--locked`, reconcile the lock from the manifest. |
| `native update` | `[<package>[@<exact-version>]] [--target TARGET] [--offline] [--manifest-path FILE]` | Refresh one or every dependency from the current catalog and install it. |
| `native remove` | `<package> [--manifest-path FILE]` | Remove a declaration and lock entry without deleting the shared cache. |
| `native list` | `[--target TARGET] [--manifest-path FILE]` | Print deterministic read-only package status. |
| `native doctor` | `[--target TARGET] [--manifest-path FILE]` | Diagnose project, lock, approximate cache size, stale staging, toolchain, and receipt state without mutation. |
| `native prune` | `[--target TARGET]` | Explicitly remove abandoned staging, catalog-orphan artifacts, and old selected-target toolchain fingerprints from the global native cache. Never changes project files. |

`--offline` guarantees that the downloader is never invoked. `--locked` is
accepted only by `install` and rejects an absent or stale lock without rewriting
it. `--manifest-path` names an `elephc.toml` file and disables ancestor
discovery. Package versions are exact catalog versions; ranges and arbitrary
URLs are rejected. `native --help` and `<command> --help` need no project.
`native remove` changes only the selected project; global cache deletion happens
only through the explicit `native prune` command.

See [Native dependencies](native-dependencies.md) for project files, cache
selection, toolchain overrides, and transactional behavior.

## Input and output

| Flag | Values | Default | Description |
|---|---|---|---|
| `<source-file>` | path | — | Required. A tagged `.php` or tagless `.lfc` file to compile. Other suffixes retain tagged-PHP behavior. |
| `--emit KIND` / `--emit=KIND` | `executable` (`exe`, `bin`), `cdylib` (`dylib`, `shared`) | `executable` | Output artifact kind. `cdylib` builds a C-ABI shared library. |
| `--emit-asm` | — | off | Write generated assembly instead of a binary. |
| `--emit-ir` | — | off | Print the EIR textual form and stop. |
| `--check` | — | off | Run front-end checks only; write nothing. |
| `--strict-php` | — | off | Reject elephc extensions in every physical PHP-mode file; `.lfc` remains extension-enabled. See [Strict PHP mode](#strict-php-mode). |
| `--source-map` | — | off | Emit a `.map` JSON sidecar next to the assembly ([schema](source-maps.md)). |
| `--debug-info` | — | off | Embed DWARF `.file`/`.loc` line directives in the assembly for lldb/gdb/profilers. |
| `--keep-symbols` | — | off | Keep the symbol table in the linked executable. It is stripped by default; `--debug-info` also implies keeping it. See [Symbol stripping](#symbol-stripping). |
| `--php-version VERSION` | `8.2`, `8.3`, `8.4`, `8.5` | detected, else `8.5` | Select the maintained PHP compatibility profile for version-dependent behavior. Sessions use it for PHP 8.4 deprecations/validation and PHP 8.5 CHIPS/option semantics. Usually unnecessary — see [Where the profile comes from](#where-the-profile-comes-from) and [Profile dependence](#profile-dependence). |
| `--web` | — | off | Compile a prefork HTTP server binary instead of a CLI executable. See [Web Server](../beyond-php/web.md). |

`--emit-ir`, `--emit-asm`, and `--check` are mutually exclusive. `--web` cannot
be combined with `--check`, `--emit cdylib`, `--emit-asm`, or `--emit-ir`. See
[Output formats and diagnostics](output-and-diagnostics.md).

### Where the profile comes from

Without an explicit `--php-version`, elephc uses the profile the project already
declares. It walks up from the entry file's directory to the filesystem root and
takes the first directory that declares one, trying these in order:

| Source | Meaning |
|---|---|
| `--php-version` | Always wins. |
| `composer.lock` → `platform-overrides.php` | What the project actually installed against. |
| `composer.json` → `config.platform.php` | Composer's own "resolve as if PHP were exactly this". |
| `.php-version` | The phpenv/asdf toolchain convention. |
| `composer.json` → `require.php` | Only when it *excludes* the newest profile — see below. |
| *(nothing)* | The newest maintained profile. |

```
$ elephc src/app.php          # composer.json pins config.platform.php = "8.3.11"
php profile 8.3 (composer.json); 2 constructs depend on it
```

**Nothing is required.** Every source is optional at every level, so a lone
`.php` file compiles with no manifest, exactly as before. Only the patch
component is ignored — `8.3.11` and `8.3` name the same profile.

A `require.php` **constraint** is a range rather than a pin, so it is honored
only when it *narrows* — when the newest profile it admits is not the one that
would have been chosen anyway:

| Constraint | Effect |
|---|---|
| `"^8.2"` | Nothing. It admits everything through the newest profile, so it says nothing elephc did not already assume. |
| `"~8.3.0"` | Profile `8.3`. It excludes everything above 8.3, which is a deliberate statement. |
| `">=8.2 <8.5"` | Profile `8.4`. |
| `"~7.4.0"` | Nothing — it admits no maintained profile, so the default stands. |

Picking a point inside a range is a judgement call, and this makes it only in
the case where every reasonable reading agrees: the project has explicitly
ruled newer PHP out. Composer's range syntax is parsed on its own terms, not
Cargo's — Composer's `~8.2` means `>=8.2 <9.0` where Cargo's means
`>=8.2 <8.3`. Hyphen ranges follow Composer too: `8.2 - 8.4` admits 8.4,
because a partial upper bound admits everything carrying that prefix. A
constraint elephc cannot read leaves the default in place rather than being
half-read into a wrong answer.

A pin outside the maintained range is clamped to the nearest supported profile
and reported, never applied silently. A malformed `composer.json` never fails
the build; elephc says the pin was not read and carries on.

### Profile dependence

`--php-version` selects a **semantics profile**, not a compatibility floor. A
compiled binary *is* its own runtime — there is no target machine's PHP for it
to be compatible with — so the only question the profile answers is *which
PHP's observable behavior should this binary emulate?*

For most programs the answer never matters, and elephc says nothing. A program
has to go out of its way to notice which profile it was built for: by asking
the runtime about its own version (`PHP_VERSION`, `PHP_VERSION_ID`,
`PHP_MINOR_VERSION`, `phpversion()`, `zend_version()`), by querying OPcache
(`opcache_get_configuration()`, `opcache_get_status()`, `ini_get('opcache.*')`,
`ini_get_all()`), or — under `--web` — by driving sessions.

When a program *does* depend on the profile, the compiler says so and points at
the construct responsible:

```
$ elephc app.php
php profile 8.5 (default); 2 constructs depend on it — pin it with --php-version to make the choice explicit
  note[3:5]: PHP_VERSION_ID reports 80200 through 80500 depending on the profile
  note[6:6]: phpversion() returns the profile's version string
```

The report is emitted whether or not `--php-version` was passed. With an
explicit flag it drops the pinning suggestion and confirms what that flag is
governing, so a deliberate choice stays visible rather than silently doing work
you cannot see.

These are `note[…]` lines, not warnings: nothing is wrong with the program.
They stay out of the `warning[…]` stream that tooling scans for real problems.

Detection is argument-aware where the dependence is: `ini_get('opcache.jit')`
reads a directive whose value moves with the profile, while
`ini_get('precision')` does not, and only the first is reported. An argument
the compiler cannot resolve to a literal is treated as a dependence, since it
cannot know what the program will ask for at runtime.

`eval()` counts too, and is matched on what its fragment *contains* rather than
on what it names — a fragment is a program, not a subject. `eval('echo
PHP_VERSION;')` is reported; `eval('echo 1 + 1;')` is not; `eval($code)` is,
because the compiler cannot read a string it does not have. Eval'd code sees
the profile the binary was compiled for, the same one the surrounding code
sees — see [eval()](../php/eval.md).

`PHP_MAJOR_VERSION`, `PHP_RELEASE_VERSION` and `PHP_EXTRA_VERSION` are never
reported: across the maintained profiles they are invariant (`8`, `0` and `""`
— every profile is an `8.x` at patch `.0`).

### Minimum version

elephc's parser is version-agnostic: it accepts the whole language whatever
`--php-version` says. A profile older than the program's own syntax is
therefore rejected, so a binary cannot claim a version its source could never
have run under:

```
$ elephc --php-version 8.2 app.php
error[3:5]: this program needs PHP 8.4 (property hooks), but --php-version selected 8.2; a binary built for 8.2 could not have run this source
```

Detected today: the pipe operator `|>` (8.5), property hooks and asymmetric
property visibility (8.4), typed class constants (8.3), and calls to functions
introduced after 8.2 (`json_validate`, `array_find`, `array_any`, `array_all`).

Two properties worth knowing:

- **A default build is never rejected.** The default profile is the newest
  maintained one and the minimum can never exceed it, so this check fires only
  when `--php-version` explicitly names an older profile.
- **Feature detection is honored.** `function_exists('json_validate')` anywhere
  in the program suppresses that function's requirement — it is the idiom for
  staying portable, and rejecting it would defeat its purpose. The guard is
  inert inside elephc (these builtins exist at every profile), but it states
  the author's intent, and that is what the check serves.

Where a construct's version mapping is not certain, it is left out rather than
guessed. A missed requirement preserves existing behavior; an invented one
would break a working build of valid code.

## Web server binary runtime arguments

When a program is compiled with `--web`, the produced binary accepts these
runtime arguments (not elephc compiler flags):

| Argument | Required | Default | Description |
|---|---|---|---|
| `--listen host:port` | Yes | — | Address and port to bind. Missing `--listen` prints an error to stderr and exits non-zero. |
| `--workers N` | No | CPU count | Number of prefork worker processes. Minimum 1. |
| `--max-body-size N` | No | `8388608` (8 MiB) | Max request body in bytes (`0` = unlimited); oversized bodies get `413`. |
| `--max-requests N` | No | `0` (never) | Recycle each worker after N requests (bounds memory growth). |
| `--max-execution-time N` | No | `0` (no limit) | Kill and respawn a worker whose request handler runs longer than N seconds. |
| `--gzip` | No | off | Compress responses when the client sends `Accept-Encoding: gzip`. |
| `--access-log` | No | off | Log one line per request to stderr. |
| `--help` (`-h`), `--version` (`-V`) | No | — | Print usage / version and exit. |

```bash
elephc --web app.php
./app --listen 127.0.0.1:8080
./app --listen 0.0.0.0:8080 --workers 4 --max-body-size 1048576 --access-log
```

The served program also receives `$_COOKIE`, `$_REQUEST`, and `$_ENV`, and can
emit cookies with `setcookie()`. The server shuts down cleanly on
`SIGINT`/`SIGTERM` and respawns workers that die.

The served program receives the HTTP request through the standard superglobals
`$_SERVER`, `$_GET`, `$_POST`, and `php://input`, and controls the response
status and headers with `http_response_code()` and `header()`. See
[Web Server](../beyond-php/web.md#request-input).

## Targets

| Flag | Values | Default | Description |
|---|---|---|---|
| `--target TARGET` / `--target=TARGET` | `macos-aarch64`, `linux-aarch64`, `linux-x86_64` (plus alias spellings; recognized future targets produce an unsupported-backend diagnostic) | host platform | Select the compilation target. |

See [Targets and cross-compilation](targets.md) for the full list of accepted
spellings.

## Optimization and code generation

| Flag | Values | Default | Env override | Description |
|---|---|---|---|---|
| `--ir-opt=on\|off` | `on`, `off` | `on` | `ELEPHC_IR_OPT` | Toggle the EIR optimization passes: identity folding, peepholes, constant folding, common-subexpression elimination, loop-invariant code motion, dead-instruction elimination, dead-store elimination, branch simplification, and the cross-function small-function inliner — run to a module-level fixed point. |
| `--no-ir-opt` | — | — | `ELEPHC_IR_OPT=off` | Shorthand for `--ir-opt=off`. |
| `--regalloc=linear\|stack` | `linear`, `stack` | `linear` | `ELEPHC_REGALLOC` | Register allocator: linear-scan, or stack-only fallback. |
| `--null-repr=sentinel\|tagged` | `sentinel`, `tagged` | `tagged` | `ELEPHC_NULL_REPR` | Representation for null-capable scalar slots. |

See [Optimization and codegen controls](optimization.md).

## Linking and FFI

| Flag | Values | Default | Description |
|---|---|---|---|
| `--link LIB` / `-l LIB` / `-lLIB` | library name | — | Link an extra native library (repeatable). |
| `--link-path DIR` / `-L DIR` / `-LDIR` | directory | — | Add a library search path (repeatable). |
| `--framework NAME` | framework name | — | Link a macOS framework (repeatable). |
| `--with-NAME` | `pdo`, `tls`, `crypto`, `phar`, `tz`, `image`, `eval`, `regex`, `web` | — | Force-enable an optional bridge or runtime capability (repeatable). Bridge names force-link their staticlib and inject any PHP-surface prelude. `--with-regex` enables managed PCRE2 for opaque dynamic eval; the project must declare `pcre2`. `--with-eval` force-links Magician but is not required for normal `eval()` use. `--with-web` is an alias for `--web`. An unknown name is an error. |

See [Linking, heap, and conditional compilation](linking-and-conditional-compilation.md).

## Memory and conditional compilation

| Flag | Values | Default | Description |
|---|---|---|---|
| `--heap-size=BYTES` | integer ≥ 65536 | `8388608` (8 MB) | Size of the program's runtime heap. |
| `--define SYMBOL` / `--define=SYMBOL` | symbol name | — | Define a compile-time symbol for `ifdef` (repeatable). |

## Strict PHP mode

| Flag | Values | Default | Description |
|---|---|---|---|
| `--strict-php` | — | off | Accept only PHP-compatible constructs in PHP-mode user files; LFC and compiler-generated source remain extension-enabled. |

Under `--strict-php` the compiler rejects the
[beyond-PHP extensions](../beyond-php/pointers.md) at the source level in every
physical PHP-mode file:

- extension syntax — `ifdef` blocks, `packed class`, `extern` declarations,
  `ptr_cast<T>(...)`, `buffer_new<T>(...)`, typed local variable declarations
  (`int $x = 5;`), and `ptr`/`buffer<T>` type annotations — is reported with a
  `rejected by --strict-php` diagnostic, one error per violation, wherever the
  construct appears (statement bodies, closures, class members, and PHP
  attribute arguments alike);
- extension builtins (`ptr_*`, `zval_*`, `buffer_*`, `class_attribute_*`) behave
  as if they did not exist, exactly as under the PHP interpreter:
  `function_exists()` returns `false` for them, calling one is an undefined
  function (the diagnostic names the disabled extension), and user code may
  declare its own functions with those names;
- names prefixed with `__elephc_` are reserved for the compiler and rejected in
  user code.

The audit covers a PHP entry plus every PHP-mode `include`/`require`d and
autoloaded user file. Physical `.lfc` files are always extension-enabled, even
when reached from a strict PHP entry; conversely, PHP included by an LFC entry
is still audited. Compiler-injected preludes (PDO, timezone, image, web, …) are
exempt, so programs using those PHP-level APIs keep compiling in strict mode.
This same call-site profile controls direct and dynamic calls,
`function_exists()`, `is_callable()`, first-class callables, and `eval()`.

Strict mode also reaches `eval()`, matching PHP's runtime semantics for eval'd
code: the compiled binary marks the eval bridge as strict, so extension
builtins do not exist inside eval'd fragments either — calling one is a runtime
fatal (like any unknown function in eval), `function_exists()`/`is_callable()`
report them as missing, and extension syntax in a fragment is a runtime parse
error. Fragments are never rejected at compile time: PHP only fails eval'd code
when it actually executes, and strict mode preserves that. User functions that
shadow extension names remain callable from eval'd code.

`--strict-php` may be combined with `--define`. LFC `ifdef` blocks consume the
symbol normally, while a PHP-mode file containing `ifdef` is rejected by the
strict audit before conditional compilation can remove either branch. Supplying
an otherwise unused define is valid.

Strict mode guarantees that the *constructs* used are PHP-compatible; it does
not change elephc's static-subset semantics. A strict-valid program can still be
rejected by the type checker in places where the PHP interpreter would run it.

## INI directives

An AOT binary has no `php.ini` to read at startup: its INI surface is compiled
in. elephc therefore splits PHP's `-d` into two mechanisms — one at compile time
and one at run time.

| Flag | Values | Default | Description |
|---|---|---|---|
| `--ini KEY=VALUE` / `--ini=KEY=VALUE` | any `opcache.*` directive | — | Compile-time override of one INI directive. Repeatable; last wins for a repeated key. Splits on the FIRST `=`, so a value may itself contain `=`. An unknown key is accepted and ignored. |
| `--strict-opcache` | — | off | Throw a `RuntimeException` when `opcache_invalidate($file, true)` targets code compiled into this binary, instead of reporting the success reference PHP reports. Off, the default is byte-identical to reference PHP. See [`--strict-opcache`](../php/opcache.md#--strict-opcache). |

```bash
elephc --ini opcache.enable_cli=1 --ini opcache.jit=tracing app.php
```

`--ini` is the exact analogue of `php -d`: it moves both `ini_get()` (the raw
INI string, reported verbatim) and `opcache_get_configuration()['directives']`
(the normalized typed value), and a value that does not parse for the
directive's type is ignored, leaving the compiled-in default.

### Runtime overrides: `ELEPHC_INI_*`

Once a binary is built, a directive can still be re-pointed for a single run
through an environment variable:

```bash
ELEPHC_INI_opcache__save_comments=0 ./app       # primary spelling
env 'ELEPHC_INI_opcache.save_comments=0' ./app  # secondary spelling
```

- **Primary spelling** — `ELEPHC_INI_` + the directive with every `.` replaced
  by `__`. This is the only form a POSIX shell can assign inline: `FOO.BAR=1
  cmd` is a syntax error in `sh`/`bash`/`zsh`.
- **Secondary spelling** — `ELEPHC_INI_` + the literal dotted directive name.
  Consulted only when the primary is unset or empty; reachable through `env`,
  `putenv`, Docker `--env`, and systemd unit files, all of which accept dots.
- The directive part stays verbatim lowercase in both spellings. It is not
  upper-cased, so multi-dot directive names cannot collide.

Precedence is **baked default → `--ini` → `ELEPHC_INI_*`**; the environment
wins. Both surfaces move together, exactly as `-d` moves both in reference PHP:

```php
// with ELEPHC_INI_opcache__save_comments=0
ini_get('opcache.save_comments');                                    // '0'
opcache_get_configuration()['directives']['opcache.save_comments'];  // false
ini_get_all()['opcache.save_comments'];                              // '0'
```

A value that does not parse for the directive's type is **ignored** — the
compile-time value stays, on both surfaces — rather than corrupting the report.
An environment variable set to the empty string is treated as unset, because
`getenv()` cannot distinguish the two.

**Which directives are overridable at run time.** Only the ones elephc merely
*reports*. Ten `opcache.*` directives are consumed at compile time to bake code
or baked constants, and honoring them on the reporting surface alone would
produce a binary that contradicts itself (`ini_get('opcache.enable_cli') === '1'`
next to an `opcache_get_status()` that still returns `false`). Their environment
variables are ignored; use `--ini` for them instead:

`opcache.enable`, `opcache.enable_cli`, `opcache.memory_consumption`,
`opcache.interned_strings_buffer`, `opcache.max_accelerated_files`,
`opcache.revalidate_freq`, `opcache.jit`, `opcache.jit_buffer_size`,
`opcache.restrict_api`, `opcache.preload`.

The other 44 directives of the PHP 8.5 set are runtime-overridable.

> **Not PHP parity — an elephc extension.** Reference PHP has *no*
> per-directive environment override. Its only environment mechanisms are
> file-granularity (`PHPRC`, `PHP_INI_SCAN_DIR`); `PHP_INI_opcache_jit=…`,
> `opcache_jit=…` and `opcache.jit=…` in the environment all do nothing
> (verified on PHP 8.5.6). `ELEPHC_INI_*` is elephc's answer to `-d` for an AOT
> binary whose `php.ini` is compiled in, and `--strict-php` does not reject it
> because it is not a language construct.

## Diagnostics and debugging

| Flag | Values | Default | Description |
|---|---|---|---|
| `--timings` | — | off | Print per-phase compiler timings to stderr. |
| `--quiet` / `-q` | — | off | Disable progress lines and colorized compiler output. |
| `--gc-stats` | — | off | Print allocation/free counters at exit. |
| `--heap-debug` | — | off | Enable runtime heap verification (double-free, bad refcount, free-list corruption). |
| `--mascotte` | — | off | Print the embedded ASCII mascot and a randomly selected quote before normal output. |

See [Output formats and diagnostics](output-and-diagnostics.md).

## Symbol stripping

A linked executable is stripped of its symbol table, which removes roughly a
quarter of the file. Nothing in a compiled program reads those names, so this
changes size only, never behavior.

| Invocation | Symbol table | DWARF |
|---|---|---|
| `elephc app.php` | stripped | — |
| `elephc --keep-symbols app.php` | kept | — |
| `elephc --debug-info app.php` | kept | emitted |

Use `--keep-symbols` when a profiler needs function names but the full DWARF of
`--debug-info` is unwanted. Shared libraries built with `--emit cdylib` are
never stripped, because their exported symbols are their interface.

Details, including what happens when the `strip` tool is unavailable, are in
[Symbol stripping](linking-and-conditional-compilation.md#symbol-stripping).

## Environment variables

Compiler environment variables provide defaults that the matching flag
overrides. Native-package variables select the cache and target C toolchain:

| Variable | Values | Equivalent flag |
|---|---|---|
| `ELEPHC_IR_OPT` | `on`, `off` | `--ir-opt=` |
| `ELEPHC_REGALLOC` | `linear`, `stack` | `--regalloc=` |
| `ELEPHC_NULL_REPR` | `tagged`, `sentinel` | `--null-repr=` |
| `ELEPHC_NATIVE_CACHE` | absolute or invocation-relative directory | Native artifact/source cache root |
| `ELEPHC_NATIVE_CC` | executable | Host or explicit cross C compiler fallback |
| `ELEPHC_NATIVE_AR` | executable | Host or explicit cross archiver fallback |
| `ELEPHC_NATIVE_RANLIB` | executable | Host or explicit cross archive indexer fallback |
| `ELEPHC_NATIVE_CC_<TARGET_ENV>` | executable | Target-specific C compiler; takes precedence over the unsuffixed value |
| `ELEPHC_NATIVE_AR_<TARGET_ENV>` | executable | Target-specific archiver; takes precedence over the unsuffixed value |
| `ELEPHC_NATIVE_RANLIB_<TARGET_ENV>` | executable | Target-specific archive indexer; takes precedence over the unsuffixed value |

`TARGET_ENV` is the uppercase target with hyphens replaced by underscores, such
as `LINUX_AARCH64`. All three tool overrides are required for a non-host target.

The variables in this table are read by the **compiler**. A separate family,
`ELEPHC_INI_<directive>`, is read by the **compiled binary** at run time — see
[Runtime overrides](#runtime-overrides-elephc_ini_).
