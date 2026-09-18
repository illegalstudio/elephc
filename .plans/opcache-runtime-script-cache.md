# OPcache: a real runtime script cache for the dynamic tier

## The premise, corrected

OPcache's perf gain in php-src is "do not lex/parse/compile the source on every
request". For every file in elephc's **script manifest** that gain is already
banked at link time, and exceeded — the code is native. There is nothing left to
cache there, and adding a cache would be pure cost.

The gain that is *not* banked lives in the **dynamic tier**: an
`include`/`require` whose path the resolver cannot fold to a constant. Those
files are not in the binary. They are read from disk and run through the eval
bridge interpreter, on every single execution.

`crates/elephc-magician/src/interpreter/include_exec.rs` today does, per
include, *every time*:

1. `std::fs::read(&resolved_path)` — the whole file, every request;
2. `eval_find_php_open_tag` / `eval_find_php_close_tag` — a `windows(5)` /
   `windows(2)` byte scan over the whole file, to split `<?php … ?>` blocks;
3. per code block, `parse_fragment_cached(code)` — a `HashMap<Vec<u8>, _>` lookup
   that SipHashes the block bytes;
4. interpretation.

Steps 1–3 are exactly what OPcache exists to remove. Step 3 has a cache already
(`parse_cache.rs`) but it is keyed on the **source bytes**, so it cannot avoid
1 or 2, it is capped at 64 KiB per fragment (a compiled Symfony container or a
real template is never cached), it is FIFO-256, and it is invisible to the
OPcache API — which reports `hits => 0`, `misses => 0` and
`opcache_is_script_cached() === false` for a file it is actively caching.

So the same change buys both things at once: the dynamic tier gets faster, and
the API stops under-reporting a capability the binary already has.

## The gate that makes this safe

The runtime script cache is active **only when the OPcache cache is enabled**,
which is the state elephc already computes at compile time from
`opcache.enable` / `opcache.enable_cli`:

| Build | Cache | Script cache |
|---|---|---|
| CLI (default) | disabled | **off** — behaviour byte-identical to today |
| CLI `--ini opcache.enable_cli=1` | enabled | on |
| `--web` / `--with-web` | enabled | on |

A default CLI binary therefore changes in no observable way. `--web`, where the
same dynamic include is re-read on every request and where reference PHP would
also have OPcache on, is where the cache turns on. This also promotes
`opcache.enable` / `opcache.enable_cli` from *reported* directives to
*load-bearing* ones.

## Tiers

### T1 — a path-keyed, mtime-validated **script** cache (the perf work)

Cache the **segmented script**, not the eval fragment: one entry per canonical
path holding the alternating `Output(bytes)` / `Code(Arc<EvalProgram>)` segment
list that `eval_execute_include_bytes` currently recomputes. That collapses
steps 1, 2 and 3 into one `stat()` plus one short-string hash lookup.

Directives that become live:

- `opcache.validate_timestamps` — `1` (default): revalidate by mtime+size;
  `0`: the entry is authoritative until a reset.
- `opcache.revalidate_freq` — do not re-`stat` an entry more often than every
  N seconds (php-src semantics: a changed file may serve stale for up to N).
- `opcache.max_file_size` — `0` (default) means no limit; a non-zero value
  refuses larger files, replacing the magic 64 KiB fragment cap.
- `opcache.memory_consumption` — a real byte budget, with real `used_memory`,
  `wasted_memory`, `cache_full`, and an OOM restart when it fills.
- `opcache.max_accelerated_files` — a real entry-count ceiling.
- `opcache.blacklist_filename` — a real prefix filter, and
  `opcache_get_configuration()['blacklist']` stops being `[]`.

### T0 — report the live cache through the OPcache API (the honesty work)

The OPcache functions are generated PHP ASTs (`src/opcache_prelude/build.rs`)
that already carry mutable state in PHP `static` variables. Add a bridge symbol,
in the shape of `__elephc_eval_set_php_version_id`, so those bodies can read the
live cache:

- `opcache_get_status()` — real `hits` / `misses` / `opcache_hit_rate`, real
  `num_cached_scripts`, and dynamically-cached files appearing in `scripts`
  alongside the frozen manifest entries.
- `opcache_is_script_cached($f)` — `true` for a warm dynamic file.
- `opcache_compile_file($f)` — actually parses and caches a non-manifest file,
  returning `true`. Today it returns `false` for a file the binary *can* run.
- `opcache_invalidate($f, true)` — actually evicts a dynamic entry.
- `opcache_reset()` — actually empties the dynamic cache.

This retires five rows of the *Limitations* table in `docs/php/opcache.md`,
including **D5**, the only divergence that can silently change what a program
*does*. `--strict-opcache` then narrows to manifest members, which is correct
and stays.

### T2 — `opcache.file_cache`: the on-disk tier

Serialize the segmented script to disk, keyed on `realpath + mtime + build-id`,
so a cold process and the `request` isolation mode (one disposable handler per
request, `crates/elephc-web/src/server.rs`) start warm. Makes
`opcache_is_script_cached_in_file_cache()` able to return `true`, and
`opcache.file_cache` / `file_cache_only` / `file_cache_read_only` live.

### T2b — `opcache.preload`: prefork warming

In `--web`, warm the cache in the supervisor **before** `fork()`. Copy-on-write
then shares it with every worker for free — no shared memory, no locks, no
`restart_pending` protocol. This is what `opcache.preload` means for an AOT
binary, and it makes `preload_statistics` describe something real.

### T3 — compiling the dynamic tier to native at run time

**Rejected.** It needs codegen plus an assembler plus W^X memory inside the
shipped binary, and it contradicts the AOT thesis (size, no toolchain
dependency, security posture). If a native tier ever exists, T2's on-disk store
becomes its code cache — but not before.

## Non-goals

- **Never cache the manifest.** It is frozen at link time. The status surface
  will carry two populations: frozen manifest entries (`hits` always 0) and live
  dynamic entries. Reference PHP also has entries with 0 hits, so this is not
  observable as a divergence.
- **Zero cost when unused.** The cache lives behind the same link gate as the
  eval bridge. A binary with no dynamic include must pay 0 bytes for it.

## Verification

- A measurement **before** T1 that sizes the prize: the read / scan / parse /
  interpret split of a dynamic include. If parse is a rounding error next to
  interpretation, T1's ceiling is low and the plan must be re-aimed.
- Differential tests against `php -n` with matching `-d opcache.*` flags, per
  the repo rule that a test asserts the observed value, not just "no error".
- Freshness tests: a file mutated between two includes, at
  `validate_timestamps` 0 and 1 and across `revalidate_freq`.
- A pinned test that a default CLI binary's behaviour is unchanged.
