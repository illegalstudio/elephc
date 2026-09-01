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
    'version'    => ['version' => '8.5.10-dev', 'opcache_product_name' => 'Zend OPcache'],
    'blacklist'  => [],
]
```

`directives` carries the **normalized** typed values — booleans as `true`/
`false`, byte sizes as byte counts (`opcache.memory_consumption` → `134217728`),
`opcache.max_wasted_percentage` as the fraction `0.05`,
`opcache.optimization_level` as the decimal `2147401727`. Key order is
registration order, not sorted.

`version.version` is the targeted PHP version (`8.2.0` … `8.5.10-dev`) and is
identical to `PHP_VERSION`. `blacklist` is always empty.

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
    'scripts'                => [ /* keyed by canonical full_path */ ],
    'jit'                    => [ /* see below */ ],
]
```

`$include_scripts = false` omits the `scripts` key entirely (it is absent, not
empty). `preload_statistics` is unaffected by that flag and still precedes
`jit`, matching reference PHP.

`num_cached_scripts` and `num_cached_keys` are the manifest size.
`max_cached_keys` is the first prime `>=` `opcache.max_accelerated_files` from
php-src's own table (`223, 463, 983, 1979, 3907, 7963, 16229, 32531, 65407,
130987, 262237, 524521, 1048793`), so the default `10000` reports `16229` and
`--ini opcache.max_accelerated_files=1000` reports `1979`. Memory figures are
synthetic but internally coherent: `free_memory = memory_consumption -
used_memory - wasted_memory` with `wasted_memory = 0`, and
`free_memory = buffer_size - used_memory` (with `used_memory` strictly below
`buffer_size`, so `free_memory` is never zero or negative) for the
interned-strings block. Counters (`hits`, `misses`, `opcache_hit_rate`,
`blacklist_misses`) are always zero — a native binary has no cache lookups to
count.

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
interfaces, traits and enums, fully qualified, original case), never built-ins.
They are the symbols of the *whole binary* rather than of the preload file
specifically — an AOT binary cannot separate "preloaded" from "compiled in".
Reference PHP omits `functions`/`classes` when empty; elephc reproduces that.
Pinned by `tests/opcache_preload_tests.rs`.

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

There is still nothing to evict: the binary's code cannot be recompiled at run
time, so the latch is the whole of the effect.

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

Always `false`, and that is exact. php-src returns early on
`!ZCG(accel_directives).file_cache`, and `opcache.file_cache` is the one
directive registered with a C `NULL` default — so an unconfigured reference PHP
returns `false` for every path too. elephc has no on-disk opcode cache to point
the directive at. Guarded by `opcache.restrict_api`.

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

`ini_set('opcache.*', …)` always returns `false`. Every directive is baked into
the binary and cannot be mutated at run time. This is exact for the
`PHP_INI_SYSTEM` majority and a divergence for the 18 `PHP_INI_ALL` directives —
see [Limitations](#limitations).

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
set": assigning it (`--ini opcache.file_cache=/x`, or `ELEPHC_INI_*` at run
time) reports the string, and assigning the *empty* string reports `''`.
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
for directives elephc merely *reports*. Ten directives are consumed at compile
time to bake code or baked constants, and honoring them on the reporting surface
alone would produce a binary that contradicts itself —
`ini_get('opcache.enable_cli') === '1'` next to an `opcache_get_status()` that
still returns `false`. Their environment variables are ignored:

| Directive | Compile-time consumer |
|---|---|
| `opcache.enable`, `opcache.enable_cli` | the baked enabled gate in every OPcache function |
| `opcache.memory_consumption`, `opcache.interned_strings_buffer`, `opcache.max_accelerated_files` | the `opcache_get_status()` memory arithmetic, the `interned_strings_usage` key's presence, and the `max_cached_keys` prime rounding |
| `opcache.revalidate_freq` | the `scripts` map's `revalidate` field |
| `opcache.jit`, `opcache.jit_buffer_size` | the `opcache_get_status()['jit']` triple |
| `opcache.restrict_api` | selects the restricted function bodies |
| `opcache.preload` | can fail the compile; bakes `preload_statistics` |

The other 44 directives of the 8.5 set are runtime-overridable. Pinned by
`tests/opcache_env_override_tests.rs`.

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

### `opcache.preload`

Reference PHP resolves `opcache.preload` during startup, before a line of the
script runs, and a missing file is a **startup fatal**. For an AOT binary,
"startup" is compile time, so elephc resolves it there:

| `opcache.preload` | Cache | Path | Result |
|---|---|---|---|
| empty (default) | any | — | nothing happens; no `preload_statistics` key |
| set | disabled | any | nothing happens; the path is never validated |
| set | enabled | resolves | `preload_statistics` is emitted |
| set | enabled | outside the manifest | compile **warning**, build proceeds |
| set | enabled | unresolvable | compile **error** |

Refusing to build is the only way to avoid shipping a binary that would report
statistics for a file that is not there. Preloading a file this program never
includes is a legitimate configuration, so it warns rather than fails.

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
  un-waited result as a permanent divergence. It is not one.)
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
| Cache population | Grows at run time as scripts are compiled/included | Fixed at compile time; membership never *grows* (a forced `opcache_invalidate()` can discard an entry, and `opcache_compile_file()` restore it) | The binary is the cache; there is no runtime compiler to add an entry |
| `opcache_compile_file()` on a file outside the manifest | Compiles it, returns `true`, and the file becomes cached | Returns `false` | A file not baked into the binary cannot be compiled at run time |
| `opcache_is_script_cached()` on a file outside the manifest | `false` until something compiles it, then `true` | `false`, permanently | Same reason |
| `ini_set('opcache.*', …)` | Succeeds for the 18 `PHP_INI_ALL` directives (e.g. `opcache.enable`, `opcache.jit_debug`), returning the previous value | Always returns `false` | Values are baked into the binary; a successful `ini_set()` would report a value nothing else honors. Exact for the `PHP_INI_SYSTEM` majority |
| `hits`, `misses`, `opcache_hit_rate`, `blacklist_misses` | Live counters | Always `0` | There are no cache lookups to count |
| `memory_usage` / `interned_strings_usage` *absolute figures* | Real shared-memory accounting | Synthetic baselines, plus Σ of the manifest's source-file sizes | No shared-memory segment exists. The *invariants* are exact: `free = total − used − wasted`, `free = buffer_size − used`, `0 < used < buffer_size`, and the whole `interned_strings_usage` key is omitted for a zero buffer. `max_cached_keys` is the exact php-src prime rounding |
| `num_cached_scripts` / `num_cached_keys` | Live cache entry count | The manifest size | Same reason |
| `jit.enabled`, `jit.on`, `jit.buffer_size`, `jit.buffer_free` | Reflect the running JIT | Clamped to `false`/`false`/`0`/`0` | Reference emits this same shape when the JIT is configured but unavailable, which is an AOT binary's permanent state. `kind`/`opt_level`/`opt_flags` *are* the real directive-derived values |
| `preload_statistics.functions` / `.classes` | The symbols the preload file added | The whole binary's user-declared symbols | An AOT binary cannot separate "preloaded" from "compiled in". A superset, never a fabrication — every name reported is genuinely declared |
| `scripts` under preloading | Carries a synthetic `$PRELOAD$` pseudo-entry and `num_cached_scripts` is bumped by one | No such entry | It stands for a shared-memory block an elephc binary never allocates |
| `opcache_get_configuration()['blacklist']` | Lists the resolved patterns from `opcache.blacklist_filename` | Always `[]` | The directive is reported but not applied |
| Directives that change engine behavior (`validate_timestamps`, `max_file_size`, `file_cache*`, `huge_code_pages`, `protect_memory`, `blacklist_filename`, …) | Change what the cache does | Reported faithfully, inert | There is no cache for them to act on. `revalidate_freq` is the exception: it feeds the `scripts` map's `revalidate` field |
| Diagnostics | `Warning: … in <file> on line <n>` | Same text, no ` in <file> on line <n>` suffix | elephc does not synthesize the call-site suffix |
| `opcache.max_accelerated_files` / `opcache.interned_strings_buffer` out of range | Refuses the store and logs through `zend_accel_error`, which is silent below `opcache.log_verbosity_level = 2` | Refuses the store, silently | The refusal is exact; the verbosity-gated timestamped log channel has no elephc counterpart, so the diagnostic is not reproduced — matching what reference PHP prints at its default verbosity |
| `ini_get_all()` unfiltered | Every directive of every loaded module (403 on the reference build) | Only the blocks elephc owns — 54 on CLI, 87 under `--web` | The filter *rule* is reproduced; the population is elephc's |
| `ini_get_all('pdo')` in a `--with-pdo` build | `[]` (known module) | `false` + `E_WARNING` | The known-module list is rendered before codegen decides the link set |
| Per-directive environment override | Does not exist (`PHP_INI_opcache_jit`, `opcache_jit`, `opcache.jit` in the environment all do nothing) | `ELEPHC_INI_*` re-points 44 of the 54 directives at run time | An elephc extension, not parity: an AOT binary has no `php.ini` to edit |
| `extension_loaded()` under `eval()` | n/a | Reports only the core set, so `extension_loaded('PDO')` is `false` under `eval()` even in a `--with-pdo` build | The eval interpreter runs at compile time with no link step |
| OPcache file functions under `eval()` | n/a | `opcache_is_script_cached()` / `opcache_invalidate()` / `opcache_compile_file()` / `opcache_is_script_cached_in_file_cache()` always return `false` (and `opcache_jit_blacklist()` `null`), with no notice | The eval interpreter has no AOT binary and therefore no manifest |
| `get_loaded_extensions()` argument | Accepts any expression | Must be a `bool`/`int` (literal or dynamic) | Both candidate lists are compile-time constants, so a dynamic flag selects between them at run time; a non-bool/int argument has no runtime truthiness conversion |

Compatibility notes (**not** divergences, called out because they are easy to
mistake for one): `opcache_get_status()`'s top-level key order and the position
of `preload_statistics` (eighth, between `opcache_statistics` and `scripts`)
match reference PHP; `ini_get_all()`'s ascending key order matches reference,
while `opcache_get_configuration()['directives']` keeps registration order in
both; `opcache_get_configuration()` returns its array even when the cache is
disabled, in both; and `opcache_invalidate()` on an existing but uncached file
returns `true` in both.
