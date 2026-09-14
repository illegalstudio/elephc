---
title: "OPcache"
description: "The observable Zend OPcache API emulated over elephc's compile-time script manifest."
sidebar:
  order: 20
---

elephc is an ahead-of-time compiler. There is no opcode cache, no runtime
compiler, and no shared-memory segment: the binary **is** the cache. Every PHP
source file that ends up in the executable was compiled once, at build time, and
stays resident for the life of the process.

What elephc provides is an emulation of OPcache's *observable API* over that
fact. The cache it reports is virtual and compile-time-known — the **script
manifest**, the exact set of PHP files baked into this binary. Queries against
it (`opcache_get_status()`, `opcache_is_script_cached()`,
`opcache_compile_file()`) answer from the manifest; the configuration surface
(`opcache_get_configuration()`, `ini_get()`, `ini_get_all()`) answers from a
per-version directive matrix compiled into the binary.

The `Zend OPcache` *extension* is always reported present, on every target.
Only the *cache* has an enabled state, and it follows the SAPI exactly as
reference PHP does.

Each OPcache function is a real declared PHP function injected into the program
only when it is referenced, so `function_exists('opcache_reset')` reports
`true`, an unrelated program pays nothing, and a program that declares its own
`opcache_reset()` keeps it.

## Enabled state

The enabled state is a compile-time constant, derived from the same directive
table everything else reads:

| Build | Predicate | Defaults | Cache |
|---|---|---|---|
| CLI (default) | `opcache.enable` **and** `opcache.enable_cli` | `1` and `0` | disabled |
| `--web` / `--with-web` | `opcache.enable` | `1` | enabled |

**Both** directives are consulted on CLI. `opcache.enable` is the master switch
that php-src copies into `ZCG(enabled)` and every cache-API guard tests first,
whatever the SAPI; `opcache.enable_cli` is the extra condition the CLI SAPI
adds. Neither one alone is sufficient there:

| `opcache.enable` | `opcache.enable_cli` | CLI | `--web` |
|---|---|---|---|
| `0` | `1` | disabled | disabled |
| `1` | `0` | disabled | enabled |
| `1` | `1` | **enabled** | enabled |
| `0` | `0` | disabled | disabled |

A default CLI binary therefore reports the cache **disabled**, matching a bare
`php script.php` run, where `opcache.enable_cli` is off:

```php
<?php
var_dump(opcache_get_status());              // false
var_dump(opcache_reset());                   // false
var_dump(opcache_is_script_cached(__FILE__));// false
```

`--ini opcache.enable_cli=1` flips a CLI binary on (the default
`opcache.enable=1` satisfies the other half), and `--ini opcache.enable=0` turns
either build off. Both are compile-time flags; see
[CLI reference — INI directives](../compiling/cli-reference.md#ini-directives).

```bash
elephc --ini opcache.enable_cli=1 app.php
```

Because the state is baked, `opcache.enable` / `opcache.enable_cli` are **not**
runtime-overridable through `ELEPHC_INI_*` — see
[Directives and configuration](#directives-and-configuration).

## The function surface

All **eight** functions reference PHP's `Zend OPcache` extension exports are
provided: `opcache_get_configuration`, `opcache_get_status`, `opcache_reset`,
`opcache_is_script_cached`, `opcache_is_script_cached_in_file_cache`,
`opcache_invalidate`, `opcache_compile_file` and `opcache_jit_blacklist`.

### `opcache_get_configuration()`

```php
opcache_get_configuration(): array
```

Returns the compile-time configuration in both the enabled and the disabled
state (reference PHP does the same — the configuration belongs to the
extension, not the cache):

```php
[
    'directives' => [ /* every opcache.* directive, registration order */ ],
    'version'    => ['version' => '8.5.0', 'opcache_product_name' => 'Zend OPcache'],
    'blacklist'  => [ /* the resolved opcache.blacklist_filename patterns */ ],
]
```

`directives` carries the **normalized** typed values — booleans as `true`/
`false`, byte sizes as byte counts (`opcache.memory_consumption` → `134217728`),
`opcache.max_wasted_percentage` as the fraction `0.05`,
`opcache.optimization_level` as the decimal `2147401727`. Key order is
registration order, not sorted.

`version.version` is the targeted *language* version (`8.2.0` … `8.5.0`), since
elephc targets a PHP minor rather than a patch release. `blacklist` carries the
patterns [`opcache.blacklist_filename`](#opcacheblacklist_filename) resolved —
reported verbatim, keyed `0..n-1` — and is empty when the directive is unset or
the binary has no dynamic tier.

Under `opcache.restrict_api` denial the function warns and returns `false`, so
its signature is `array|false`; guard with `is_array()`.

### `opcache_get_status()`

```php
opcache_get_status(bool $include_scripts = true): array|false
```

Disabled → `false`. Enabled → the reference-shaped array, keys in reference
order:

```php
[
    'opcache_enabled'        => true,
    'cache_full'             => false,
    'restart_pending'        => false,
    'restart_in_progress'    => false,
    'memory_usage'           => ['used_memory' => …, 'free_memory' => …,
                                 'wasted_memory' => 0,
                                 'current_wasted_percentage' => 0.0],
    'interned_strings_usage' => ['buffer_size' => …, 'used_memory' => …,
                                 'free_memory' => …, 'number_of_strings' => …],
    'opcache_statistics'     => ['num_cached_scripts' => …, 'num_cached_keys' => …,
                                 'max_cached_keys' => …, 'hits' => 0,
                                 'start_time' => …, 'last_restart_time' => 0,
                                 'oom_restarts' => 0, 'hash_restarts' => 0,
                                 'manual_restarts' => 0, 'misses' => 0,
                                 'blacklist_misses' => 0,
                                 'blacklist_miss_ratio' => 0.0,
                                 'opcache_hit_rate' => 0.0],
    // 'preload_statistics' here, when opcache.preload is set — see below
    'scripts'                => [ /* keyed by canonical full_path, plus '$PRELOAD$' when preloading */ ],
    'jit'                    => [ /* see below */ ],
]
```

`$include_scripts = false` omits the `scripts` key entirely (it is absent, not
empty). `preload_statistics` is unaffected by that flag and still precedes
`jit`, matching reference PHP.

`num_cached_scripts` and `num_cached_keys` are the manifest size, plus one for the
synthetic `$PRELOAD$` entry when [preloading](#opcachepreload) is configured.
`max_cached_keys` is the first prime `>=` `opcache.max_accelerated_files` from
php-src's own table (`223, 463, 983, 1979, 3907, 7963, 16229, 32531, 65407,
130987, 262237, 524521, 1048793`), so the default `10000` reports `16229` and
`--ini opcache.max_accelerated_files=1000` reports `1979`. Memory figures are
synthetic but internally coherent: `free_memory = memory_consumption -
used_memory - wasted_memory` with `wasted_memory = 0`, and
`free_memory = buffer_size - used_memory` (with `used_memory` strictly below
`buffer_size`, so `free_memory` is never zero or negative) for the
interned-strings block. `hits`, `misses` and `opcache_hit_rate` are LIVE, counted by the
[runtime script cache](#the-runtime-script-cache); they stay at zero in a binary
that has no dynamic tier, which genuinely performs no cache lookups.
`blacklist_misses` and `blacklist_miss_ratio` are LIVE too. The counter is bumped
by [`opcache.blacklist_filename`](#opcacheblacklist_filename) **and** by the
`opcache.max_file_size` refusal — php-src counts both, the name meaning "compiled
but deliberately not stored" rather than "matched a blacklist". The ratio is
`blacklist_misses * 100 / (hits + misses + blacklist_misses)`: php-src divides by
its INTERNAL miss count, which includes blacklist misses, while the `misses` it
*reports* has them subtracted back out — so all three reported figures are in the
denominator. It is `0.0` when there have been no lookups at all.

`interned_strings_usage` is **absent** — not empty, not zeroed — when
`opcache.interned_strings_buffer=0`, leaving eight top-level keys instead of
nine. php-src guards the sub-array on the buffer having actually been allocated.

`restart_pending` reflects the in-process restart latch — see
[`opcache_reset()`](#opcache_reset).

Each `scripts` entry has the reference 7-key shape:

```php
'/abs/path/main.php' => [
    'full_path'           => '/abs/path/main.php',
    'hits'                => 0,
    'memory_consumption'  => 458,          // source file size in bytes
    'last_used'           => 'Sat Jul 25 17:53:05 2026',
    'last_used_timestamp' => 1784994785,   // the REQUEST clock
    'timestamp'           => 1784994505,   // the source mtime
    'revalidate'          => 1784994787,   // last_used_timestamp + revalidate_freq
],
```

The three clock fields come from **two** clocks, exactly as php-src reads them:

| Field | Source | Note |
|---|---|---|
| `timestamp` | the source file's mtime | `0` for a force-invalidated entry |
| `last_used_timestamp` | the request clock | identical for every entry |
| `revalidate` | `last_used_timestamp + opcache.revalidate_freq` | never in the past |

`last_used` is built with libc `asctime(localtime(…))` in reference PHP, so it
follows the **system** timezone (`TZ`, else `/etc/localtime`) rather than
`date.timezone`, and its day-of-month is space-padded (`Thu Jul  2 13:46:40
2026`). elephc resolves the system zone the same way and applies it only around
this one field, restoring the previous default timezone afterwards — a caller's
own `date()` is unaffected.

#### The `jit` sub-array

```php
// with --ini opcache.jit=tracing
'jit' => [
    'enabled'     => false,   // always
    'on'          => false,   // always
    'kind'        => 5,       // from opcache.jit
    'opt_level'   => 4,       // from opcache.jit
    'opt_flags'   => 6,       // from opcache.jit
    'buffer_size' => 0,       // always
    'buffer_free' => 0,       // always
],
```

`kind` / `opt_level` / `opt_flags` are the **real** directive-derived values:
elephc implements php-src's full `opcache.jit` spelling parser, including the
keyword forms (`disable`, `off`, `on`, `tracing`, `function`) and the four-digit
`CRTO` numeric form with its per-digit validation and its observable
partial-assignment residue on a rejected value.

`enabled`, `on`, and both buffer figures are clamped to `false`/`false`/`0`/`0`
unconditionally. This is not a shortfall: reference PHP emits exactly this shape
whenever the JIT is *configured but unavailable in this process* (verified on
8.5.6 with `opcache.jit_buffer_size=0`, and with an extension that overrides
`zend_execute_ex`). An AOT binary has no JIT engine and no JIT buffer, so
"configured but unavailable" is its permanent state; reporting `enabled = true`
would be the divergence.

On an 8.4/8.5 target the default `opcache.jit = disable` renders the all-zero
array. On 8.2/8.3 the default is `tracing`, so the default array carries
`kind = 5, opt_level = 4, opt_flags = 6`. Pinned by
`tests/opcache_jit_status_tests.rs`.

#### `preload_statistics`

When `opcache.preload` is set and the cache is enabled, an eighth key appears
between `opcache_statistics` and `scripts`:

```php
'preload_statistics' => [
    'memory_consumption' => 458,                    // Σ of the manifest entries
    'functions'          => ['baz'],                // omitted when empty
    'classes'            => ['Foo', 'Bar'],         // omitted when empty
    'scripts'            => ['/abs/path/main.php'],
],
```

`functions` and `classes` are real user-declared symbols (functions, classes,
interfaces, traits and enums, fully qualified, original case), never built-ins,
and never the `__elephc_include_variant_…` spelling the resolver gives a
function it inlines out of an include. They are the symbols of the *whole
binary* rather than of the preload file specifically — an AOT binary cannot
separate "preloaded" from "compiled in", and since the preload file is now one
of the files compiled in, its own symbols are genuinely among them. Reference
PHP omits `functions`/`classes` when empty; elephc reproduces that. Pinned by
`tests/opcache_preload_tests.rs`.

### `opcache_reset()`

```php
opcache_reset(): bool
```

Disabled → `false`. Enabled → `true` on the **first** call, `false` on every
call after that, and `opcache_get_status()['restart_pending']` flips to `true`.

That is reference PHP's behavior, not a simplification: php-src's
`zend_accel_schedule_restart()` sets `ZCSG(restart_pending)` **and** clears the
shared `ZCSG(accelerator_enabled)` flag that `opcache_reset()`'s own guard
tests, so a second call in the same request takes the `false` exit. The restart
itself is deferred to the next request, so nothing else observable moves within
this one — `opcache_enabled` stays `true`, `num_cached_scripts` and
`manual_restarts` are untouched, and `opcache_is_script_cached()` /
`opcache_invalidate()` keep answering from the cache (they read a
request-local snapshot of the same flag).

```php
var_dump(opcache_reset());                                  // true
var_dump(opcache_get_status()['restart_pending']);          // true
var_dump(opcache_reset());                                  // false
```

The binary's own code cannot be recompiled at run time, so for the compile-time
manifest the latch is still the whole of the effect. With the
[runtime script cache](#the-runtime-script-cache) enabled there IS something to
evict, and a reset issued from inside `eval()` schedules one — **deferred to the
next request**, exactly as php-src defers it.

Within the scheduling request nothing else moves: the cache keeps answering,
`num_cached_scripts` holds, and `manual_restarts` and `last_restart_time` stay at
their previous values. Reference PHP 8.5.10 behaves identically (verified: it
still answers `true` to `opcache_is_script_cached()` straight after a reset). The
restart itself runs at the start of the next `--web` request, where generated code
performs it beside the other per-request resets. A CLI program is one request and
therefore never performs it — which is also right, since reference PHP would
restart at a next request a CLI process does not have.

This holds wherever the reset is written. A natively compiled `opcache_reset()`
schedules on the dynamic tier too, not only on the reported latch, so ordinary
code gets the same behaviour as a reset issued from inside `eval()`.

### `opcache_is_script_cached()`

```php
opcache_is_script_cached(string $filename): bool
```

Disabled → `false` for every path. Enabled → `$filename` resolved and tested for
membership in the manifest, so both `__FILE__` and a relative path hit the same
entry. A file outside the manifest is always `false`, and no runtime action can
change that.

A manifest member that a forced `opcache_invalidate()` has **discarded** also
reports `false`, until `opcache_compile_file()` re-caches it — see
[`opcache_invalidate()`](#opcache_invalidate).

### `opcache_invalidate()`

```php
opcache_invalidate(string $filename, bool $force = false): bool
```

Disabled → `false`. Enabled → `true` whenever the path **resolves**, directories
included.

The return value is exact, not an approximation. php-src's
`zend_accel_invalidate()` returns "the script is in the cache **or** the path
resolves"; every manifest member is a canonicalized path that was stat'd at
build time, so cache membership already implies the right-hand side and the
disjunction reduces to it. Verified against reference PHP 8.5.6: `''`, `'.'`,
`'..'`, `'/'`, `'/tmp'`, `__FILE__` and a relative path all return `true`;
`'/no/such/file'`, `' '` and a NUL byte return `false`. The empty string is a
`getcwd()` case — PHP's `realpath('')` resolves to the current working
directory.

`$force = true` **discards** a manifest member, reproducing php-src's
`zend_accel_discard_script()`:

```php
var_dump(opcache_is_script_cached(__FILE__));        // true
var_dump(opcache_invalidate(__FILE__, true));        // true
var_dump(opcache_is_script_cached(__FILE__));        // false
$s = opcache_get_status();
var_dump(count($s['scripts']));                      // 1  — the entry STAYS
var_dump($s['scripts'][__FILE__]['timestamp']);      // 0  — the only field that moves
var_dump(opcache_compile_file(__FILE__));            // true
var_dump(opcache_is_script_cached(__FILE__));        // true — re-cached
```

`num_cached_scripts`, `num_cached_keys` and the `scripts` map's membership do
**not** change, because reference PHP does not change them either: the discarded
script keeps its shared-memory slot until the next restart. A **non**-forced
call discards nothing (the file's mtime has not moved, so php-src's timestamp
validation succeeds).

#### `--strict-opcache`

What the discard above reproduces is the **reported** cache state. What it
cannot reproduce is the effect reference PHP's users are usually after: there,
a forced invalidate means the next `include` re-reads and re-compiles the file
**from disk**. Code that elephc compiled into the binary is frozen at link time
and can never be re-read, so a program that invalidates in order to pick up
*changed code* — a dev-mode cache-buster, a plugin reloader — keeps running the
old code with no signal at all. This is divergence **D5**, and it is the only
one in this model that can silently change what a program *does* rather than
what it *reports*.

Compile with `--strict-opcache` to make that case throw a `RuntimeException`
instead:

```console
$ elephc --strict-opcache --ini opcache.enable=1 --ini opcache.enable_cli=1 app.php
```

The throw is deliberately narrow — only the request that cannot be honored:

| `$force` | path in manifest | reference PHP | default | `--strict-opcache` |
|----------|------------------|---------------|---------|--------------------|
| `false`  | yes              | `true`        | `true`  | `true`             |
| `true`   | no               | `true`        | `true`  | `true`             |
| `true`   | yes              | `true`        | `true`  | **throws**         |

Without `$force`, reference PHP discards nothing either, so elephc is not
failing to do anything. A non-manifest path is a file this binary never
compiled, so invalidating it is a no-op there too. A **disabled** cache still
returns `false` without throwing, exactly as reference PHP short-circuits before
invalidating.

The flag is opt-in and changes nothing when absent: the default remains
byte-identical to reference PHP.

### `opcache_compile_file()`

```php
opcache_compile_file(string $filename): bool
```

Disabled → writes reference PHP's notice to `STDERR` and returns `false`:

```text
Notice: Zend OPcache has not been properly started, can't compile file
```

Enabled → `true` for a manifest member (it was already compiled into the
binary — the same thing reference PHP reports when compiling an already-cached
file), `false` for anything else. A file that is not in the binary cannot be
compiled at run time; see [Limitations](#limitations).

Compiling a manifest member also **re-caches** it, clearing any forced
invalidation — unless a restart is pending, in which case php-src's
`persistent_compile_file()` compiles but does not store, and
`opcache_is_script_cached()` stays `false`. The return value reports the
compile, not the store, so it is `true` either way.

### `opcache_is_script_cached_in_file_cache()`

```php
opcache_is_script_cached_in_file_cache(string $filename): bool
```

`false` with `opcache.file_cache` unset — php-src returns early on
`!ZCG(accel_directives).file_cache`, which is the one directive registered with a
C `NULL` default, so an unconfigured reference PHP answers `false` for every path
too. Guarded by `opcache.restrict_api`.

With the directive **set**, it answers from the real on-disk cache: `true` once a
script has been stored there, `false` before that and after the source changes.
It applies exactly the validation a read does, so it never reports an entry a read
would reject. Reachable from inside `eval()`, where the dynamic tier lives; a
natively compiled call still answers `false`, for the same reason its sibling file
functions do. See [`opcache.file_cache`](#opcachefile_cache).

### `opcache_jit_blacklist()`

```php
opcache_jit_blacklist(Closure $closure): void
```

A no-op returning `null`. php-src's body only mutates the JIT's own blacklist,
behind `#ifdef HAVE_JIT`; an AOT binary has no runtime JIT engine and therefore
no blacklist — the same fact that clamps
[`opcache_get_status()['jit']`](#the-jit-sub-array). **Not** guarded by
`opcache.restrict_api`, matching reference PHP.

### `opcache.restrict_api`

Reference PHP compares this directive as a plain byte prefix against the **entry
script** path. An elephc binary has exactly one entry script, fixed when it was
built, and `--ini` is a compile-time flag — so the decision has no
runtime-varying input and is resolved once, at compile time.

When it denies, six of the eight exported functions warn and return `false`:
`opcache_get_configuration`, `opcache_get_status`, `opcache_reset`,
`opcache_is_script_cached`, `opcache_is_script_cached_in_file_cache`,
`opcache_invalidate`. `opcache_compile_file` and `opcache_jit_blacklist` are
**not** guarded, matching reference PHP (verified on 8.5.6: they return `true`
and `null` respectively, silently). The warning text is byte-identical to
php-src's:

```text
Warning: Zend OPcache API is restricted by "restrict_api" configuration directive
```

The matching rule reproduces php-src exactly — empty prefix disables the
restriction, the comparison is a byte prefix rather than a path-component match,
it is case-sensitive even on a case-insensitive filesystem, a prefix longer than
the entry path denies, an equal prefix allows, and the path compared is the
resolved (canonicalized) one. Each rule is pinned by
`tests/opcache_restrict_api_tests.rs`.

## The script manifest

The manifest is the set of physical source files compiled into the binary,
including tagged `.php` and tagless `.lfc` files. It has three sources, each
path stat'd once at build time:

1. the entry file,
2. every statically-resolved `include` / `require` / `include_once` /
   `require_once` target,
3. every autoloaded file — Composer `autoload.files`, PSR-4 and SPL-rule class
   files, and the includes those files themselves pull in.

Order is deterministic: the entry file, then the included files sorted by
canonical path, then the autoloaded files sorted by canonical path, with
duplicates dropped across all three groups (first occurrence wins). Reference
PHP's own `scripts` order is its internal hash order and is not reproducible, so
any stable order is as faithful.

**Not in the manifest**: a file reached only through a dynamic include whose
path the resolver cannot fold to a constant. Such a file is not compiled into
the binary either, so omitting it is correct rather than a shortfall. A path
whose `canonicalize`/`metadata` lookup fails is skipped rather than reported
with fabricated values.

Paths are canonicalized with the same normalization `__FILE__` uses, so they
match a userland `realpath()` result. On macOS a `/tmp/...` invocation appears
as `/private/tmp/...`, exactly as reference PHP reports it.

```php
<?php
require __DIR__ . '/lib.php';

$s = opcache_get_status();
echo $s['opcache_statistics']['num_cached_scripts'];        // 2
var_dump(opcache_is_script_cached(__DIR__ . '/lib.php'));   // true
var_dump(opcache_is_script_cached('./lib.php'));            // true (realpath'd)
```

Pinned by `tests/opcache_manifest_tests.rs`.

## The runtime script cache

The manifest above is the compile-time tier and it cannot grow. There is a second
tier: a PHP file included at run time through a path the resolver could not fold
to a constant. Such a file is not in the binary, so it is read from disk and run
through the eval bridge interpreter. At AOT top level a runtime-dynamic include
is a **compile error**, so this tier is reached only from inside `eval()` — which
is where a template or a generated container ends up in a `--web` application.

That tier is the one place in an elephc binary where an opcode-cache-shaped
saving still exists, and it is where OPcache's caching directives actually act.

### What it caches

The unit is the **file**, not the eval fragment: one entry per canonical path
holding the file already split into its alternating literal-output and parsed-code
segments. A warm include therefore skips the file read, the `<?php` / `?>` byte
scan, and the parse — everything except executing the statements.

Measured on macOS arm64, including the same file in a loop: identical interpreted
work in every row, with only the file's size varying (the padding is a PHP comment,
so it grows the read, the scan and the parse without adding a statement).

| Included file | Uncached | Cached | |
|---|---|---|---|
| 52 B | 263 µs | 47 µs | ≈6× |
| 64 KB | 1 580 µs | 43 µs | **≈40×** |
| 128 KB | 10 979 µs | 67 µs | **≈160×** |

What transfers between machines is the SHAPE, not the absolute microseconds. The
warm cost is **flat in file size** and was stable to within 2 µs across repeated
runs and machine loads; the uncached cost is CPU-bound and moved by up to 40%
with load. The step between 64 KB and 128 KB is the old byte-keyed fragment
cache's 64 KiB ceiling, above which every include re-parsed the whole file.
`opcache.max_file_size` replaces that ceiling with the directive reference PHP
uses for the same decision.

### When it is on

The script cache follows the cache-enabled state exactly — the same predicate
[`opcache_get_status()`](#opcache_get_status) reports:

| Build | Cache | Script cache |
|---|---|---|
| CLI (default) | disabled | **off** |
| CLI `--ini opcache.enable_cli=1` | enabled | on |
| `--web` / `--with-web` | enabled | on |

A default CLI binary caches nothing, and its includes behave exactly as they did
before this tier existed. `opcache.enable` and `opcache.enable_cli` are therefore
no longer reporting-only directives: they decide whether the cache runs.

### Freshness

Freshness follows php-src rather than "always re-read". On a fill the entry
records `revalidate_at = now + opcache.revalidate_freq`; it is re-`stat`ed only
once that instant has passed, and the entry is refilled when the file's mtime or
size has moved.

```bash
# Pick up a changed include immediately, at the cost of a stat per include.
elephc --ini opcache.enable_cli=1 --ini opcache.revalidate_freq=0 app.php

# Never stat again: the entry is authoritative until opcache_reset().
elephc --ini opcache.enable_cli=1 --ini opcache.validate_timestamps=0 app.php
```

**This is a behaviour change with the cache on**: at the default
`opcache.revalidate_freq = 2`, a file edited between two includes can serve its
previous contents for up to two seconds. Reference PHP behaves the same way and
for the same reason, and a default CLI binary is unaffected because its cache is
off.

### What the API answers about it

Inside `eval()` — the only place the dynamic tier is reachable — the OPcache file
functions stop being terminal `false`s and answer about the real cache:

```php
<?php
eval('
$f = getenv("TEMPLATES") . "/page.php";

var_dump(opcache_is_script_cached($f));   // false — never loaded
include $f;
var_dump(opcache_is_script_cached($f));   // true  — cached by the include

var_dump(opcache_invalidate($f, true));   // true  — and discards the entry
var_dump(opcache_is_script_cached($f));   // false

var_dump(opcache_compile_file($f));       // true  — reads, parses and caches it
var_dump(opcache_is_script_cached($f));   // true  — without ever running it

var_dump(opcache_reset());                // true  — flushes, once
var_dump(opcache_reset());                // false
');
```

`opcache_compile_file()` is the one that changes most: it used to answer `false`
for every file outside the compile-time manifest, including files the binary can
actually run. It now compiles and stores one, which is what reference PHP does.

`opcache_invalidate()` reports whether the **path resolves**, not whether it was
cached — php-src's `zend_accel_invalidate()` returns "cached **or** resolvable",
and a cached path was canonicalized when it was stored, so the disjunction
reduces to the right-hand side. `$force` is what discards the entry.

A dynamic callable reaches the same answers: `call_user_func('opcache_invalidate',
$f, true)` and `opcache_invalidate($f, true)` go through one shared core.

Two functions are deliberately unchanged.
`opcache_is_script_cached_in_file_cache()` stays `false` — php-src returns early
on an unset `opcache.file_cache`, and elephc has no on-disk opcode cache to point
the directive at. `opcache_jit_blacklist()` stays a no-op returning `null`.

### What `opcache_get_status()` reports

`opcache_get_status()` reports **both tiers**, from natively compiled code as well
as from inside `eval()`:

```php
<?php
eval('include __DIR__ . "/lib.php"; include __DIR__ . "/lib.php";');

$s = opcache_get_status();
$s['opcache_statistics']['num_cached_scripts'];  // manifest + runtime entries
$s['opcache_statistics']['hits'];                // 1 — the second include
$s['opcache_statistics']['misses'];              // 1 — the first
$s['opcache_statistics']['opcache_hit_rate'];    // 50.0
$s['scripts'][__DIR__ . '/lib.php'];             // the dynamic entry, 7-key shape
```

Every figure the runtime cache owns is live: `hits`, `misses`,
`opcache_hit_rate` (php-src's percentage of lookups, `0.0` when there have been
none), `num_cached_scripts` / `num_cached_keys`, `cache_full`,
`manual_restarts`, `last_restart_time`, and `memory_usage.used_memory` /
`free_memory`. `restart_pending` is the **union** of the two latches, so a
restart scheduled by an `opcache_reset()` inside `eval()` is visible natively.

A runtime entry's `scripts` row carries its own real numbers rather than the
manifest's shared request clock: its `hits` is that file's hit count,
`last_used_timestamp` is when it was last served, `timestamp` is its source
mtime (`0` once a forced invalidate discarded it), and `revalidate` is
`last_used_timestamp + opcache.revalidate_freq` — present from an 8.3 target on,
under the same per-version gate the manifest entries use.

`opcache_get_status()` answers the same thing wherever it is **written**. The
eval interpreter carries its own handler for the name, but it now prefers the
program's own declaration when there is one — the prelude's body, which knows
both the manifest and the live cache. It used to dispatch its handler first, so
the same call in the same binary answered an array natively and `false` from
inside `eval()`. Its handler remains the answer for a program with no such
declaration, where the cache is genuinely absent.

**Cost when there is no dynamic tier: none.** A program that never reaches the
eval bridge cannot have a runtime cache, so the calls that would read it are
folded to the empty-cache answer at lowering time and the interpreter archive is
never referenced. Such a binary reports exactly its manifest, with zero counters
— byte-identical to what it reported before this tier existed.

### Directives that now act

| Directive | Effect on the dynamic tier |
|---|---|
| `opcache.validate_timestamps` | `1`: revalidate by mtime and size. `0`: never re-stat |
| `opcache.revalidate_freq` | Seconds between two revalidations of one entry |
| `opcache.max_file_size` | Refuses to *cache* a larger file; the file still runs, and the refusal counts as a `blacklist_misses`, as in php-src. `0` means no limit |
| `opcache.memory_consumption` | A real byte budget for the cached segments |
| `opcache.max_accelerated_files` | A real entry-count ceiling |
| `opcache.file_update_protection` | Refuses to *cache* a file younger than its value; the file still runs. `0` disables it |
| `opcache.blacklist_filename` | Refuses to *cache* any path a blacklist entry matches; the file still runs. See [below](#opcacheblacklist_filename) |

The cache **never evicts**. Like php-src, it refuses new entries once the budget
or the entry ceiling is reached and latches `cache_full`; a refused file is still
read and executed, it is simply not stored. `opcache_reset()` is what releases
both.

## Directives and configuration

elephc carries a per-version directive matrix for PHP 8.2 through 8.5. The 8.5
set has 54 directives and is byte-verified against reference PHP 8.5.6; the
older sets apply the documented deltas:

| Delta | 8.2 | 8.3 | 8.4 | 8.5 |
|---|---|---|---|---|
| `opcache.consistency_checks` | present | — | — | — |
| `opcache.jit_max_trace_length` | — | present | present | present |
| `opcache.file_cache_read_only` | — | — | — | present |
| `opcache.jit` default | `tracing` | `tracing` | `disable` | `disable` |
| `opcache.jit_buffer_size` default | `0` | `0` | `64M` | `64M` |
| `opcache.jit_hot_loop` default | `64` | `64` | `64` | `61` |
| `opcache.jit_prof_threshold` reported type | `int(0)` | `float(0.005)` | `float(0.005)` | `float(0.005)` |

Select the profile with `--php-version=8.2` … `8.5` (default `8.5`).

### `ini_get()`

`ini_get('opcache.*')` reports the **raw INI string**, which is not the same as
the normalized value `opcache_get_configuration()` reports. Booleans render as
`"1"` / `"0"` (unlike the `session.*` block, which uses `"1"` / `""`), and four
directives carry a raw spelling that cannot be derived from the normalized
value:

| Directive | `ini_get()` | `opcache_get_configuration()` |
|---|---|---|
| `opcache.memory_consumption` | `"128"` | `134217728` |
| `opcache.max_wasted_percentage` | `"5"` | `0.05` |
| `opcache.optimization_level` | `"0x7FFEBFFF"` | `2147401727` |
| `opcache.jit_buffer_size` (8.4/8.5) | `"64M"` | `67108864` |

On a CLI binary `ini_get()` models the `opcache.*` block and nothing else — it
returns `false` for every other key, including `session.*`, matching reference
PHP where a directive of an absent extension reports `false`. Under `--web` the
same function also serves the `session.*` block; see
[Sessions](sessions.md#runtime-configuration-ini_get--ini_set).

### `ini_set()`

`ini_set('opcache.*', …)` succeeds for **three** directives and returns `false`
for the other 51:

| Directive | Effect |
|---|---|
| `opcache.revalidate_freq` | the [runtime script cache](#the-runtime-script-cache)'s revalidation interval |
| `opcache.validate_timestamps` | whether it revalidates at all |
| `opcache.file_update_protection` | how young a file it refuses to store |

These are exactly the intersection of two sets: the 18 directives php-src
registers `PHP_INI_ALL`, where reference PHP's own `ini_set()` succeeds, and the
ones elephc's cache actually reads. A successful call returns the **previous**
raw value and moves every surface together — `ini_get()`,
`ini_get_all()`, `opcache_get_configuration()['directives']`, and the cache
itself, from the very next include:

```php
var_dump(ini_get('opcache.revalidate_freq'));   // "2"
var_dump(ini_set('opcache.revalidate_freq', '77'));  // "2"  — the previous value
var_dump(ini_get('opcache.revalidate_freq'));   // "77"
var_dump(opcache_get_configuration()['directives']['opcache.revalidate_freq']); // 77
var_dump(ini_set('opcache.memory_consumption', '256'));  // false — PHP_INI_SYSTEM
```

That sequence is byte-identical on reference PHP 8.5.10, `false` included:
`opcache.memory_consumption` is `PHP_INI_SYSTEM` there too, so refusing it is
**exact** rather than a shortfall.

The remaining 15 `PHP_INI_ALL` directives still return `false`, and that is
deliberate. Fourteen of them are JIT knobs and one is `opcache.dups_fix`; all are
inert in elephc, so succeeding would move a reported value while nothing changed
— the same contradiction the
[runtime-override scope rule](#overriding-a-directive) exists to prevent. The
remaining divergence is listed in [Limitations](#limitations).

An `ini_set()` **before the program's first `eval()`** still reaches the cache.
That ordering is not free: generated code installs the compiled configuration
when the eval context is first built, which is that first `eval()` — so an
override is held separately and applied on read, and the later install cannot
clobber the earlier call.

This works identically under `--web`, where the session-aware `ini_set()` owns
the name: it runs the same three arms before its own `opcache.*` refusal, so a
CLI and a `--web` binary never disagree about the same directive. The session
directives that wrapper handles are unaffected.

### `ini_get_all()`

```php
ini_get_all(?string $extension = null, bool $details = true): array|false
```

Keys are sorted ascending, matching reference PHP (whose `ini_get_all('zend
opcache')` is likewise sorted, while `opcache_get_configuration()` keeps
registration order). With `$details = true` each entry is
`['global_value' => …, 'local_value' => …, 'access' => …]`, where `access` is the
`PHP_INI_*` bitmask — `7` (`PHP_INI_ALL`) for 18 directives, `4`
(`PHP_INI_SYSTEM`) for the other 36, matching reference PHP exactly.

`opcache.file_cache` reports `global_value` and `local_value` as **`null`**, not
`''` — the only one of the 54 that does, in both the `$details = true` and
`$details = false` surfaces. php-src registers it with a C `NULL` default while
every other opcache string directive defaults to `""`. The `null` means "never
set": assigning it (`--ini opcache.file_cache=/x`) reports the string, and
assigning the *empty* string reports `''`. It is a compile-time assignment only —
the directive now bakes a [startup validation](#opcachefile_cache), so
`ELEPHC_INI_*` does not re-point it.
`ini_get('opcache.file_cache')` reports `''` in all three cases — the `null` is
visible through `ini_get_all()` alone.

The `$extension` filter reproduces php-src's rule exactly: the name is matched
**verbatim** against the lowercase module registry with no case folding (unlike
`extension_loaded()`, which *is* case-insensitive).

```php
$oc  = ini_get_all('zend opcache');   // the 54 opcache.* entries
$no  = ini_get_all('Zend OPcache');   // false + E_WARNING — matched verbatim
$spl = ini_get_all('spl');            // [] — known module, no INI directives
$nx  = ini_get_all('nope');           // false + E_WARNING
$all = ini_get_all('core');           // the unfiltered surface
```

`'core'` selects the unfiltered surface, reproducing php-src's rule that Core's
module number is 0 so the per-module filter is skipped. `ini_get_all()` is
therefore `array|false` — narrow with `is_array()` before counting or indexing.
The unfiltered surface is 54 entries on CLI and 87 under `--web` (33 `session.*`
in registration order, then 54 `opcache.*` sorted). Pinned by
`tests/opcache_ini_tests.rs` and
`tests/web_session_tests.rs::session_ini_surface_is_pinned`.

### Overriding a directive

Two mechanisms, both documented in full on the
[CLI reference](../compiling/cli-reference.md#ini-directives):

- **`--ini KEY=VALUE`** — compile time, the analogue of `php -d`. It moves both
  `ini_get()` (the raw string) and
  `opcache_get_configuration()['directives']` (the normalized value) together.
  The value first goes through PHP's INI *scanner*, which rewrites the boolean
  barewords `on`/`true`/`yes` → `"1"` and `off`/`false`/`no`/`none`/`null` → `""`
  case-insensitively for **every** directive — so `--ini opcache.jit=on` reports
  `ini_get('opcache.jit') === '1'` (while still selecting the tracing JIT) — and
  the directive's type handler then reads the result. Booleans and quantities
  **never fail**: `--ini opcache.save_comments=garbage` stores `false`, and
  `--ini opcache.max_file_size=12abc` stores `12` and emits a compile warning
  (`Invalid "opcache.max_file_size" setting. Invalid quantity "12abc": unknown
  multiplier "c", interpreting as "12" for backwards compatibility`), which is
  where reference PHP emits its startup warning. `ini_get()` echoes the value as
  the scanner stored it in both cases. Only the handlers that genuinely refuse a
  value in php-src leave the compiled default in place:
  `opcache.max_wasted_percentage` outside `1..=50`, `opcache.memory_consumption`
  below its 8 MiB floor, an invalid `opcache.jit` spelling, and the twelve
  [range-validated integers](#range-validated-directives) below.
- **`ELEPHC_INI_opcache__<directive>`** — run time, on an already-built binary.
  This is an elephc extension; reference PHP has no per-directive environment
  override (only `PHPRC` / `PHP_INI_SCAN_DIR`, which are file-granularity).

The runtime override is deliberately narrower than `--ini`. It is honored only
for directives elephc merely *reports*. **Eighteen** directives are consumed at
compile time to bake code or baked constants, and honoring them on the reporting
surface alone would produce a binary that contradicts itself —
`ini_get('opcache.enable_cli') === '1'` next to an `opcache_get_status()` that
still returns `false`. Their environment variables are ignored:

| Directive | Compile-time consumer |
|---|---|
| `opcache.enable`, `opcache.enable_cli` | the baked enabled gate in every OPcache function |
| `opcache.memory_consumption`, `opcache.interned_strings_buffer`, `opcache.max_accelerated_files` | the `opcache_get_status()` memory arithmetic, the `interned_strings_usage` key's presence, and the `max_cached_keys` prime rounding |
| `opcache.revalidate_freq` | the `scripts` map's `revalidate` field, and the [runtime script cache](#the-runtime-script-cache)'s revalidation interval |
| `opcache.validate_timestamps`, `opcache.max_file_size`, `opcache.file_update_protection` | the [runtime script cache](#the-runtime-script-cache)'s freshness and admission rules |
| `opcache.jit`, `opcache.jit_buffer_size` | the `opcache_get_status()['jit']` triple |
| `opcache.restrict_api` | selects the restricted function bodies |
| `opcache.preload` | compiles the preload file into the binary; can fail the compile; bakes `preload_statistics` |
| `opcache.file_cache`, `opcache.file_cache_read_only` | the [startup validation](#opcachefile_cache) that can refuse to run |
| `opcache.log_verbosity_level`, `opcache.error_log` | the `zend_accel_error` channel that reports it |
| `opcache.blacklist_filename` | the [blacklist](#opcacheblacklist_filename) the runtime script cache consults, read once when the eval context is built |

The other 36 directives of the 8.5 set are runtime-overridable. Pinned by
`tests/opcache_env_override_tests.rs` and
`tests/opcache_blacklist_tests.rs`.

### Range-validated directives

Twelve integer directives have their own bounds check in php-src. An
out-of-range value is **refused**, never clamped, so both surfaces keep
reporting the compiled default. Ten of them also print a warning at compile
time, where reference PHP prints its startup one.

| Directive | Accepted | Warning |
|---|---|---|
| `opcache.max_accelerated_files` | 200 … 1000000 | — (silent) |
| `opcache.interned_strings_buffer` | 0 … 32767 | — (silent) |
| `opcache.jit_blacklist_root_trace` | 0 … 255 | `Invalid "…" setting; using default value instead. Should be between 0 and 255` |
| `opcache.jit_blacklist_side_trace` | 0 … 255 | same |
| `opcache.jit_hot_func` | 0 … 255 | same |
| `opcache.jit_hot_loop` | 0 … 255 | same |
| `opcache.jit_hot_return` | 0 … 255 | same |
| `opcache.jit_hot_side_exit` | 0 … 255 | same |
| `opcache.jit_max_loop_unrolls` | 1 … **9** | `Invalid "…" setting. Should be between 1 and 10` |
| `opcache.jit_max_recursive_calls` | 1 … **9** | `Invalid "…" setting. Should be between 1 and 10` |
| `opcache.jit_max_recursive_returns` | 0 … **3** | `Invalid "…" setting. Should be between 0 and 4` |
| `opcache.jit_max_trace_length` | 4 … 1024 | `Invalid "…" setting. Should be between 4 and 1024` |

The three bolded ceilings are php-src's own off-by-one: those handlers test with
a strict `<` against the constant their message prints as an inclusive bound, so
`--ini opcache.jit_max_recursive_returns=4` is refused by a warning that calls
`4` legal. elephc reproduces both the accepted range and the message.

The first two are silent because php-src reports them through
`zend_accel_error()`, which is gated on `opcache.log_verbosity_level >= 2` — at
the default verbosity reference PHP prints nothing either.

`opcache.max_accelerated_files` is also one of only two integer directives read
with C `atoi` rather than the quantity parser (the other is
`opcache.memory_consumption`), so a `K`/`M`/`G` suffix or an `0x` prefix is
ignored: `--ini opcache.max_accelerated_files=8K` reads `8`, falls below the 200
floor, and leaves the default `10000`. It carries no quantity diagnostic either.

### `opcache.file_cache`

Set it to an absolute, writable directory and elephc keeps a **real on-disk cache
of parsed scripts** there, so a process that starts cold skips the read, the
`<?php` scan and the parse for anything it cached before:

```bash
elephc --ini opcache.enable_cli=1 --ini opcache.file_cache=/var/cache/elephc app.php
```

That matters most under `--web`, where workers are forked from a master that has
run no PHP and are recycled after `--max-requests`: without it, every recycled
worker re-parses every dynamically included template, continuously, in production.

What it stores is elephc's parsed form, not php-src's opcodes — the two formats
have nothing to do with each other, and a directory written by one is never read
by the other. Entries are encoded with bincode, which was chosen by measurement:
decoding is **3.5–5× faster than re-parsing** at about 1.8× the source size, while
`serde_json` decodes a small script *slower* than parsing it. The benchmark ships
as an `#[ignore]`d test (`script_cache::format_bench`) so the choice can be
re-checked rather than believed.

**An entry is never trusted blind.** Three independent checks must all pass, and
any failure — including a corrupt or unreadable file — is treated as a miss rather
than an error:

| Check | Stops |
|---|---|
| the writer's directory (crate version + format version) | one build reading another build's parsed form |
| the header's magic and format version | a foreign or outdated file |
| the source's mtime, size and canonical path | a changed file running as its old self, and a file-name hash collision |

The format version is guarded by a test that fingerprints the IR's own source, so
a change to the stored shape cannot silently keep an old version number — bincode
is not self-describing, and a mismatched payload could otherwise decode into a
plausible but wrong tree.

`opcache.file_cache_read_only` reads entries without ever creating one, which is
what makes a shared read-only cache directory usable.

Reference PHP also **validates the directory at startup and refuses to run** if it
is unusable, and elephc reproduces that refusal exactly.

The check runs only when the cache is **enabled**. That is reference behavior,
not an elephc shortcut — verified on PHP 8.5.10, `opcache.enable=0` and the CLI
default `opcache.enable_cli=0` both run the script without looking at the
directory at all, however broken it is. A default CLI binary is therefore
completely unaffected.

| `opcache.file_cache` | `file_cache_read_only` | Result |
|---|---|---|
| empty (default) | `0` | nothing happens — php-src's `NULL` default |
| empty | `1` | **fatal**: `opcache.file_cache_read_only is set without a proper setting of opcache.file_cache` |
| absolute dir, `R_OK`+`W_OK` | `0` | accepted |
| absolute dir, `R_OK` only | `1` | accepted — read-only needs no write access |
| absolute dir, `R_OK` only | `0` | **fatal**: `opcache.file_cache must be a full path of an accessible directory` |
| relative path, missing path, or a file | either | same **fatal** |

Both fatals go through php-src's `zend_accel_error` channel rather than PHP's
error reporting, so the line carries its timestamp and pid and the process exits
with status **254** (php-src's `exit(-2)`):

```console
$ elephc --ini opcache.enable_cli=1 --ini opcache.file_cache=/no/such/dir app.php
$ ./app
Sat Sep 12 18:19:28 2026 (26639): Fatal Error opcache.file_cache must be a full path of an accessible directory
$ echo $?
254
```

`opcache.log_verbosity_level` gates that channel, but **not** these two lines: the
gate is `level <= verbosity` and a fatal is level `0`, so it prints even at `0`.
`opcache.error_log` redirects it to a file, falling back to stderr if the file
cannot be opened — both exactly as php-src does. `opcache.file_cache_read_only`
exists only on an 8.5 target; older profiles never reach its branch.

The check happens where the runtime cache is configured, which is the first time
the program reaches the eval bridge **at run time** — not before the program's
first statement, as reference PHP's startup does. Two consequences, both in
[Limitations](#limitations).

First, a binary that never reaches the bridge never validates. That is a wider
set than "contains no `eval()`": an `eval()` whose argument folds to a constant
is resolved at compile time and emits no bridge call at all, so

```php
eval('$x = 1;');                          // const-folded — never validates
eval('include __DIR__ . "/lib.php";');    // reaches the bridge — validates
```

Second, output written before that point is already flushed when the fatal
lands:

```console
$ ./app                # --ini opcache.file_cache=/no/such/dir
BEFORE
Sat Sep 12 18:35:29 2026 (93850): Fatal Error opcache.file_cache must be a full path of an accessible directory
$ echo $?
254
```

Reference PHP prints no `BEFORE`, because its check runs before the script does.
The message and the exit status are identical; only the position differs.

### `opcache.blacklist_filename`

Names the files that list paths to **run but never cache**. A blacklisted
include executes exactly as it would otherwise — the directive changes what is
*stored*, never what happens — so the only observable difference is in
`opcache_get_status()`:

- the script is absent from `scripts`;
- `opcache_is_script_cached()` answers `false` for it;
- `blacklist_misses` counts one per **refusal**, not per file: a script included
  twice is refused twice;
- `misses` does **not** move. php-src hands a blacklisted file back to the
  original compiler before any cache accounting, so the refusal replaces the
  miss rather than accompanying it.

The directive value is itself a `glob()` naming the blacklist files — wildcards
and `[...]` classes included — and **every** matching file is loaded and their
entries unioned:

```ini
opcache.blacklist_filename=/etc/opcache/deny-*.list
```

The files are read **once**, when the eval context is built, and never re-read.

Inside each file:

| Line | Meaning |
|---|---|
| `;` as the **first** character | Comment, skipped. A `;` after anything else — even a space — does not start one |
| Blank, or only whitespace | Skipped |
| Anything else | A pattern |

Every surviving line is **expanded, not taken verbatim**: a surrounding pair of
double quotes is stripped, a relative entry is resolved against *the blacklist
file's own directory* (not the process cwd), and `.` / `..` are folded out. So a
list sitting beside the code it names can simply say `vendor/`. The expanded form
is also what `opcache_get_configuration()['blacklist']` reports.

A pattern is **anchored at the start and open at the end**, so it matches as a
prefix: a bare directory blocks everything under it, and `/srv/app/p_pref`
blocks `/srv/app/p_prefix.php`. Matching is case-sensitive even where the
filesystem is not.

Three wildcards, and the difference between the first two is the subtle one —
php-src compiles `*` to `[^/]*` but `**` to `.*`:

| Wildcard | Matches | Crosses `/` |
|---|---|---|
| `?` | exactly one character | no |
| `*` | any run of characters | **no** — `/srv/app/*.php` stays out of `/srv/app/sub/` |
| `**` | any run of characters | **yes** — `/srv/app/**.php` reaches every depth |

A value matching no file is not an error: it blacklists nothing and logs
`No blacklist file found matching: <value>` through the
[accelerator channel](#opcachefile_cache), which needs
`opcache.log_verbosity_level >= 2` to be visible.

**Divergences.**

- Only the **dynamic tier** is governed. The compile-time script manifest is
  frozen into the binary and is never consulted against the blacklist, so a
  blacklisted path that is part of the compiled program still appears in
  `scripts`. Blacklisting is a property of what the *runtime cache* stores.
- The blacklist is read when the eval context is built, so a binary with no
  dynamic tier never loads one at all.
- `opcache_get_configuration()['blacklist']` lists the resolved entries exactly as
  reference does: verbatim, wildcards unexpanded, keyed `0..n-1`, with every matched
  file's lines unioned. A binary with **no dynamic tier** reports `[]` — truthful,
  since such a binary never loads a blacklist at all.
- The directive is **compile-time only**. Unlike the reporting-only majority it
  ignores its `ELEPHC_INI_*` runtime override, because a value arriving after the
  cache was built could not retroactively keep anything out of it — see
  [Overriding a directive](#overriding-a-directive).

### `opcache.preload_user`

Reported faithfully, and **inert**. This is the one preload directive elephc does
not act on, and the reason is structural rather than unfinished work.

Reference PHP runs the preload file in a **privileged startup pass**: the process
compiles and executes it before serving anything, and because that pass may run as
root, OPcache requires `opcache.preload_user` to drop to an unprivileged user
first. Its absence under uid 0 is a startup fatal
(`"opcache.preload" requires "opcache.preload_user" when running under uid 0`),
and when the process is *not* root the directive is ignored outright — at
`opcache.log_verbosity_level >= 2` it says so:
`"opcache.preload_user" is ignored because the current user is not "root"`
(both VERIFIED on PHP 8.5.10; the warning needs `opcache.preload` set too).

An elephc binary has no such pass. `opcache.preload` is resolved at compile time
and the file is *inlined* — its top-level code becomes part of the program and
runs with exactly the privileges of whoever runs the binary. No privilege boundary
is crossed, so there is nothing to drop from.

That makes each half of the directive's behaviour un-transferable for a different
reason:

| Reference behaviour | Why elephc does not reproduce it |
|---|---|
| Fatal under uid 0 with no `preload_user` | Would refuse to start a binary that is doing nothing privileged |
| Switch to `preload_user` when root | Means dropping privileges in a compiled binary — a security-behaviour change, not a reporting one |
| Warn when not root | Reference's message names a reason (*"the current user is not root"*) that is not elephc's: elephc ignores the directive at **every** uid, so the message would be false when run as root |

The two halves also cannot be split: implementing the fatal **without** the
privilege switch would be worse than neither, because setting `preload_user` as
root would then let the binary keep running *as root* while appearing to have
honoured the guard.

If you rely on this guard, the equivalent in an elephc deployment is not to run
the binary as root in the first place — which is what the guard is trying to
achieve anyway.

### `opcache.preload`

Reference PHP resolves `opcache.preload` during startup, before a line of the
script runs, and a missing file is a **startup fatal**. For an AOT binary,
"startup" is compile time, so elephc resolves it there:

| `opcache.preload` | Cache | Path | Result |
|---|---|---|---|
| empty (default) | any | — | nothing happens; no `preload_statistics` key |
| set | disabled | any | nothing happens; the path is never validated |
| set | enabled | resolves | the file is **compiled into the binary**; `preload_statistics` is emitted |
| set | enabled | is the entry file | already the program; the injection is skipped |
| set | enabled | unresolvable | compile **error** |

Refusing to build is the only way to avoid shipping a binary that would report
statistics for a file that is not there.

### What preloading actually does

`opcache.preload` becomes an implicit `require_once` of the preload file at the
very top of the entry program, so the resolver inlines it: its declarations are
compiled into the binary and its top-level statements run first, exactly once.
That is reference PHP's semantic expressed in the mechanism a compiler already
has — and it means the entry script can **use** preloaded symbols without
including the file:

```php
// lib.php, named by --ini opcache.preload=lib.php
function preloaded_helper(): string { return "from preload"; }
class PreloadedClass {}

// the entry script, which never mentions lib.php
preloaded_helper();          // works
new PreloadedClass();        // works
```

Functions, classes, interfaces, traits and enums all carry over, and so do the
symbols of everything the preload file itself `require`s — preloading is
transitive, and every file it pulls in joins the script manifest. All of it
verified against reference PHP 8.5.6 and pinned by
`tests/opcache_preload_tests.rs`.

One difference, in elephc's favour and unavoidable: reference PHP does **not**
carry the preload file's CONSTANTS into the request (verified — both `const X =
1;` and `define('X', 1)` leave `defined('X')` false), because preloading keeps
the compiled function and class tables while the startup request's own symbol
table is torn down. An AOT binary has no torn-down startup request: the preload
file's top-level code is part of the program, so its constants exist. elephc is
a superset here; reproducing the absence would mean building machinery to
un-define a constant the program legitimately declared.

## Extensions

`extension_loaded()` and `get_loaded_extensions()` resolve against a
compile-time-known set: an always-present core set plus the bridges actually
linked into this compilation.

```php
get_loaded_extensions();
// Core, standard, SPL, json, pcre, date, ctype, mbstring, Reflection, Zend OPcache

get_loaded_extensions(true);
// Zend OPcache
```

Bridge-linked extensions are added on top, per compilation:

| Bridge | Reported extension | Linked when |
|---|---|---|
| `elephc-tls` | `openssl` | TLS streams used, or `--with-tls` |
| `elephc-pdo` | `PDO` | PDO used, or `--with-pdo` |
| `elephc-pdo` | `mysqli` | mysqli used, or `--with-mysqli` |
| `elephc-crypto` | `hash` | `hash()` used, or `--with-crypto` |
| `elephc-bcmath` | `bcmath` | A `bc*` function used, or `--with-bcmath` |
| `elephc-phar` | `Phar` | Phar used, or `--with-phar` |
| `elephc-image` | `gd` | GD/Imagick used, or `--with-image` |
| `elephc-web` | `session` | `--web` |

The `elephc-pdo` archive backs two PHP surfaces; the reported extension follows
the surface the program actually uses (a mysqli-only program reports `mysqli`
but not `PDO`, and vice versa; `mysqlnd` is never reported).

`elephc-tz` and `elephc-magician` expose no distinct PHP extension surface and
report nothing.

Name matching is case-insensitive, as in PHP, but only over the **canonical**
names — `extension_loaded('zend opcache')` is `true`, `extension_loaded('opcache')`
is `false`, exactly as in reference PHP.

A literal argument const-folds to a static boolean at compile time. A dynamic
argument is supported too: the compile-time-known set is baked into the binary
and compared case-insensitively at run time.

```php
$name = 'JSON';
var_dump(extension_loaded($name));   // true
```

`get_loaded_extensions()`'s optional flag must be a `bool`/`int`, but it does not
have to be a literal: both candidate lists (regular and Zend) are compile-time
constants, so a literal flag bakes one of them in and a dynamic flag selects
between the two at run time. Pinned by `tests/extension_loaded_tests.rs`.

## Comparing against reference PHP

Two host-PHP behaviors will make a naive A/B comparison look like an elephc
divergence when it is not:

- **`opcache.file_update_protection` (default `2`)** — reference PHP refuses to
  cache a file whose mtime is less than that many seconds old, so a freshly
  written probe script reports an EMPTY `scripts` map and
  `opcache_is_script_cached(__FILE__) === false`. Wait three seconds, or pass
  `-d opcache.file_update_protection=0`, and reference caches its own entry
  script exactly as elephc does. (An earlier revision of this page recorded the
  un-waited result as a permanent divergence. It is not one.) elephc's
  [runtime script cache](#the-runtime-script-cache) now applies the same rule to
  the dynamic tier, so a comparison of *that* tier needs the same care.
- **Xdebug** — the host `php` loads it, and it overrides `var_dump()`. Pass
  `-d xdebug.mode=off` for byte-comparable output. It also puts the JIT in
  reference PHP's "configured but unavailable" state, which is coincidentally
  the shape elephc always reports.

A comparable reference invocation is therefore:

```bash
php -d xdebug.mode=off \
    -d opcache.enable=1 -d opcache.enable_cli=1 \
    -d opcache.file_update_protection=0 probe.php
```

### Verify on Linux, not macOS

macOS's shared-memory model hides the `scripts` map from reference PHP entirely,
so a local A/B there compares an empty entry set against elephc's populated one
and cannot see inside it at all. The official Docker images are the oracle:

```bash
docker run --rm -v "$PWD:/w" -w /w php:8.2-cli \
    php -d opcache.enable=1 -d opcache.enable_cli=1 probe.php
```

Running that across `php:8.2-cli` … `php:8.5-cli` confirmed the per-version
directive matrix byte for byte — 53/53/53/54 directives, identical values, and
the same nine `opcache_get_status()` keys — and it is what caught elephc
reporting a `revalidate` key in every script entry under `--php-version 8.2`,
where reference PHP only added that key in 8.3. That divergence was structurally
invisible on macOS.

### FPM is deliberately NOT used as a reference

An earlier plan called for capturing the same fixtures under `php-fpm`. That is
**out of scope on purpose**, for two independent reasons.

elephc does not target FPM and cannot: FPM is a process manager for the PHP
*interpreter*, while elephc emits native binaries. `--web` is elephc's own
prefork server — it replaces FPM rather than plugging into it, so there is no
integration to validate.

What FPM would additionally expose is cross-request state: accumulating `hits`
and `misses`, a growing `scripts` map, and `opcache_reset()`'s deferred restart
(`manual_restarts` / `last_restart_time` landing on the *next* request). Under
AOT there is no cache to accumulate into — the code is frozen in the binary — so
those counters are **class-B** values by design: synthetic but internally
coherent. Deferred restart belongs to the same family as divergence **D5**:
elephc cannot restart a cache that does not exist. Comparing either against FPM
would measure the fidelity of a number the model deliberately invents.

The defects worth catching here are **class-A** — reporting a key or a value the
targeted PHP version does not have — and the CLI images above expose all of
them. What remains useful for `--web` is that the surface stay *self-consistent*
across requests within elephc's own model, which needs no reference at all and
is pinned by `tests/web_tests.rs`.

## Limitations

Divergences from reference PHP. Each row was checked against reference PHP 8.5.6
on macOS arm64.

| Behavior | Reference PHP | elephc | Why |
|---|---|---|---|
| Cache population | Grows at run time as scripts are compiled/included | The compile-time manifest never *grows*; the [runtime script cache](#the-runtime-script-cache) does, for dynamically included files | The binary is the cache for everything compiled into it; only the dynamic tier can gain an entry at run time |
| `opcache_compile_file()` on a file outside the manifest | Compiles it, returns `true`, and the file becomes cached | Inside `eval()`: compiles and caches it, returns `true`. In natively compiled code: still `false` | A dynamic include is a compile error at AOT top level, so a natively compiled `opcache_compile_file()` names a file that program could never run |
| `opcache_is_script_cached()` on a file outside the manifest | `false` until something compiles it, then `true` | Inside `eval()`: `true` once it is cached. In natively compiled code: `false` | Same reason. `opcache_get_status()['scripts']` DOES report it from native code |
| `ini_set('opcache.*', …)` | Succeeds for all 18 `PHP_INI_ALL` directives, returning the previous value | Succeeds for **3** of them — `revalidate_freq`, `validate_timestamps`, `file_update_protection` — and returns `false` for the other 15 | Those three are the only `PHP_INI_ALL` directives elephc's cache actually reads, and for them the whole surface moves together, byte-identical to reference. The other 15 are inert here (14 JIT knobs and `dups_fix`), so succeeding would report a value nothing honors. Exact for the 36 `PHP_INI_SYSTEM` directives |
| `oom_restarts`, `hash_restarts` | Live counters | Always `0` | The runtime cache refuses rather than restarting when it fills. `hits`, `misses`, `opcache_hit_rate`, `blacklist_misses` and `blacklist_miss_ratio` are NOT in this row any more: they are live for the [runtime script cache](#the-runtime-script-cache) |

| `memory_usage` / `interned_strings_usage` *absolute figures* | Real shared-memory accounting | Synthetic baselines, plus Σ of the manifest's source-file sizes, plus the runtime cache's real accounted bytes | No shared-memory segment exists. The *invariants* are exact: `free = total − used − wasted`, `free = buffer_size − used`, `0 < used < buffer_size`, and the whole `interned_strings_usage` key is omitted for a zero buffer. `max_cached_keys` is the exact php-src prime rounding |
| `num_cached_scripts` / `num_cached_keys` | Live cache entry count | The manifest size PLUS the runtime cache's entries | The manifest half cannot grow; the runtime half does |
| `jit.enabled`, `jit.on`, `jit.buffer_size`, `jit.buffer_free` | Reflect the running JIT | Clamped to `false`/`false`/`0`/`0` | Reference emits this same shape when the JIT is configured but unavailable, which is an AOT binary's permanent state. `kind`/`opt_level`/`opt_flags` *are* the real directive-derived values |
| `preload_statistics.functions` / `.classes` | The symbols the preload file added | The whole binary's user-declared symbols | An AOT binary cannot separate "preloaded" from "compiled in". A superset, never a fabrication — every name reported is genuinely declared, and the preload file's own symbols are among them |
| A preloaded file's CONSTANTS | Not carried into the request | Available, like any compiled-in declaration | The startup request whose symbol table reference tears down does not exist in an AOT binary |
| `opcache.preload_user` | Switches the preload pass to that user, and its absence is a startup fatal under uid 0 | Reported faithfully, inert | There is no privileged preload pass to switch: the preload file's top-level code is part of the program and runs with the program's own privileges. See [below](#opcachepreload_user) |
| `opcache_get_configuration()['blacklist']` | Lists the resolved patterns from `opcache.blacklist_filename` | Lists them, verbatim and in order | No divergence any more. A binary with no dynamic tier reports `[]`, which is truthful: it never loads a blacklist |
| Directives that change engine behavior (`huge_code_pages`, `protect_memory`, …) | Change what the cache does | Reported faithfully, inert | There is no compile-time cache for them to act on. `validate_timestamps`, `revalidate_freq`, `max_file_size`, `memory_consumption`, `max_accelerated_files`, `file_update_protection` and `blacklist_filename` are NOT in this row: they govern the [runtime script cache](#the-runtime-script-cache) |
| `opcache.file_cache` contents | Serialized php-src opcodes, keyed by a `system_id` | elephc's own parsed form, keyed by crate version plus format version | The two are different compilers; neither could read the other's file. The directive, the validation, the read-only mode and the `opcache_is_script_cached_in_file_cache()` answer all behave as reference does — only the bytes inside differ |
| `opcache_is_script_cached_in_file_cache()` from NATIVE code | Answers for any path | `false`; only a call inside `eval()` answers from the cache | The same native-versus-`eval()` boundary its sibling file functions have: the dynamic tier the cache belongs to is reachable only from `eval()` |
| `opcache.file_cache` validation in a binary that never reaches the eval bridge | Validated at every startup | Never validated | The check runs where the runtime cache is configured. A binary with no dynamic tier has no cache to configure, and paying for the check in every binary would break the pay-for-use rule the rest of this tier follows. The set is wider than "no `eval()`": a const-folded `eval('$x = 1;')` is resolved at compile time and emits no bridge call |
| When the `opcache.file_cache` fatal is raised | Before the first statement runs, so nothing is output | At the first `eval()`, so output written before that point is already flushed | Same cause as the row above. The fatal, its message and its exit status 254 are identical; only its position relative to the program's own output can differ |
| `version.version` | The running patch release (`8.5.6`) | The targeted language version (`8.5.0`) | elephc targets a PHP minor, not a patch. Understating is the safe direction — a caller gating on `>= 8.5.6` applies a redundant workaround rather than skipping a fix elephc may not have. See [System and I/O](system-and-io.md) for the full rationale and its cost |
| Diagnostics | `Warning: … in <file> on line <n>` | Same text, no ` in <file> on line <n>` suffix | elephc does not synthesize the call-site suffix |
| `opcache.max_accelerated_files` / `opcache.interned_strings_buffer` out of range | Refuses the store and logs through `zend_accel_error`, which is silent below `opcache.log_verbosity_level = 2` | Refuses the store, silently | The refusal is exact, and it happens at COMPILE time, where there is no running process to log from. The `zend_accel_error` channel itself now exists — it is what carries the [`opcache.file_cache`](#opcachefile_cache) fatals — but only at run time. At reference PHP's default verbosity these two lines are not printed either |
| `ini_get_all()` unfiltered | Every directive of every loaded module (403 on the reference build) | Only the blocks elephc owns — 54 on CLI, 87 under `--web` | The filter *rule* is reproduced; the population is elephc's |
| `ini_get_all('pdo')` in a `--with-pdo` build | `[]` (known module) | `false` + `E_WARNING` | The known-module list is rendered before codegen decides the link set |
| Per-directive environment override | Does not exist (`PHP_INI_opcache_jit`, `opcache_jit`, `opcache.jit` in the environment all do nothing) | `ELEPHC_INI_*` re-points 36 of the 54 directives at run time | An elephc extension, not parity: an AOT binary has no `php.ini` to edit |
| `extension_loaded()` under `eval()` | n/a | Reports only the core set, so `extension_loaded('PDO')` is `false` under `eval()` even in a `--with-pdo` build | The eval interpreter runs at compile time with no link step |
| OPcache file functions under `eval()` | n/a | `opcache_is_script_cached()`, `opcache_invalidate()` and `opcache_compile_file()` answer about the [runtime script cache](#the-runtime-script-cache) — they are no longer terminal `false`s. `opcache_is_script_cached_in_file_cache()` stays `false` and `opcache_jit_blacklist()` `null` | The dynamic tier gave the first three something real to answer about. The last two have no backing subsystem in either tier |
| `get_loaded_extensions()` argument | Accepts any expression | Must be a `bool`/`int` (literal or dynamic) | Both candidate lists are compile-time constants, so a dynamic flag selects between them at run time; a non-bool/int argument has no runtime truthiness conversion |

Compatibility notes (**not** divergences, called out because they are easy to
mistake for one): `opcache_get_status()`'s top-level key order and the position
of `preload_statistics` (eighth, between `opcache_statistics` and `scripts`)
match reference PHP; `ini_get_all()`'s ascending key order matches reference,
while `opcache_get_configuration()['directives']` keeps registration order in
both; `opcache_get_configuration()` returns its array even when the cache is
disabled, in both; and `opcache_invalidate()` on an existing but uncached file
returns `true` in both.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`opcache_compile_file()`](./builtins/misc/opcache_compile_file.md) | `(mixed $filename): bool` | `bool` | ✓ | — |
| [`opcache_get_configuration()`](./builtins/misc/opcache_get_configuration.md) | `(): array` | `array` | ✓ | — |
| [`opcache_get_status()`](./builtins/misc/opcache_get_status.md) | `(mixed $include_scripts = true): mixed` | `mixed` | ✓ | — |
| [`opcache_invalidate()`](./builtins/misc/opcache_invalidate.md) | `(mixed $filename, mixed $force = false): bool` | `bool` | ✓ | — |
| [`opcache_is_script_cached()`](./builtins/misc/opcache_is_script_cached.md) | `(mixed $filename): bool` | `bool` | ✓ | — |
| [`opcache_is_script_cached_in_file_cache()`](./builtins/misc/opcache_is_script_cached_in_file_cache.md) | `(mixed $filename): bool` | `bool` | ✓ | — |
| [`opcache_jit_blacklist()`](./builtins/misc/opcache_jit_blacklist.md) | `(mixed $closure): void` | `void` | ✓ | — |
| [`opcache_reset()`](./builtins/misc/opcache_reset.md) | `(): bool` | `bool` | ✓ | — |

<!-- elephc:generated:symbols:end -->
