---
title: "PHP Session Support"
description: "Implementation specification for php-src-compatible sessions in --web binaries."
sidebar:
  order: 12
---

This document describes the implemented session subsystem for `--web` binaries.
The compatibility references are the maintained `php/php-src` branches
`PHP-8.2`, `PHP-8.3`, `PHP-8.4`, and `PHP-8.5`. The compiler selects their
version-dependent session semantics with `--php-version` (default `8.5`).

## Scope

Sessions are available only when compiling with `--web`. The web prelude exposes
the PHP-visible API and the `elephc-web` static library owns per-worker state,
file storage, locking, upload progress, and trans-SID output rewriting.

The implemented surface includes:

- `$_SESSION`, `SID`, and the `PHP_SESSION_*` constants;
- all PHP session functions exposed by the web prelude;
- the `files` save handler plus object and legacy callable user handlers;
- `php`, `php_binary`, and `php_serialize` serialization;
- strict mode, lazy write, probabilistic/manual GC, cache limiters, cookies,
  trans-SID rewriting, and upload progress;
- `ini_get()`, `ini_set()`, and `ini_get_all()` for `session.*` directives.

CLI programs do not receive the prelude and cannot call the session API.

## Architecture

```text
PHP handler
  -> web prelude (`src/web_prelude.rs`)
     -> PHP lifecycle, serializer selection, user handlers, cookies and INI
  -> C ABI (`crates/elephc-web/src/session/`)
     -> state.rs             per-request settings and transfer buffer
     -> file_io.rs           files handler, flock, lazy touch and GC
     -> id.rs                ID validation and generation
     -> wire_format.rs       php/php_binary entry parsing
     -> upload_progress.rs   streaming multipart RMW updates
  -> trans_sid.rs            response URL/form/Location rewriting
```

One worker executes PHP synchronously on one thread. Mutable bridge state is
therefore process-local and race-free within a worker. Cross-worker access to a
session file is serialized with `flock(LOCK_EX)`.

## PHP lifecycle

`session_start()` performs these transitions in order:

1. Applies supported options and selects a manually-set, Cookie, GET, or POST
   ID according to `use_cookies` and `use_only_cookies`.
2. Rejects dangerous ID characters and external-referer mismatches.
3. Marks the status active before invoking save-handler callbacks.
4. Opens the handler, validates strict-mode IDs, and creates a collision-free
   ID when needed.
5. Clears `$_SESSION`, reads the selected record, stores the read-time snapshot,
   and decodes the payload.
6. Runs probabilistic GC, emits a new cookie only when required, and sends cache
   headers.
7. For `read_and_close`, closes the handler and returns to inactive status after
   the new cookie/cache headers have been produced.

The wrapper appended by the compiler calls `session_write_close()` at handler
shutdown while the session remains active.

Important observable rules:

- An accepted cookie is not needlessly reissued.
- A new `read_and_close` session still sends its cookie.
- `session_abort()` closes without writing and leaves current in-memory values
  untouched.
- `session_reset()` reloads the read-time state.
- `session_destroy()` removes storage and clears status/ID, but does not clear
  `$_SESSION` or delete the browser cookie.
- `session_gc()` warns and returns `false` unless a session is active.
- Starting another session in the same request clears stale array entries before
  decoding the next record.

### ID regeneration

`session_regenerate_id(false)` writes the old record and closes its handler or
file descriptor before selecting the new ID. It then opens and reads the new
record to establish the normal lock/module state while preserving the current
in-memory array. Shutdown writes that array under the new ID. With
`delete_old=true`, the old record is destroyed instead of written.

Generated IDs follow `session.sid_length` and
`session.sid_bits_per_character`; strict mode defaults to disabled. User handlers
implementing `SessionIdInterface` and
`SessionUpdateTimestampHandlerInterface` participate in creation and validation.

## Binary-safe bridge ABI

Configuration strings use NUL-terminated getters. Serialized payloads do not:
PHP strings may contain embedded NUL bytes, so file and wire-format operations
use a shared byte buffer plus explicit length.

The generated prelude stages outbound PHP bytes with
`elephc_web_session_data_stage()` and `ptr_write_string()`. It reads returned
bytes using the operation's pointer, `elephc_web_session_data_len()`, and
`ptr_read_string()`. The core symbols are:

```text
elephc_web_session_read_bytes(id, path, read_and_close) -> ptr
elephc_web_session_write_bytes(id, path, ptr, len) -> 0|1
elephc_web_session_snapshot_bytes() -> ptr
elephc_web_session_{count_entries,entry_key,entry_value}_bytes(...)
elephc_web_session_*_bin_bytes(...)
```

Legacy C-string wrappers remain exported for existing Rust-side callers, but
generated PHP uses only the pointer/length variants.

## Files save handler

The files handler follows php-src's `[depth;[mode;]]path` grammar:

| Configuration | Meaning |
|---|---|
| `/var/lib/php/sessions` | Flat files, mode `0600`. |
| `2;/var/lib/php/sessions` | Two ID-derived subdirectories, mode `0600`. |
| `2;0640;/var/lib/php/sessions` | Two levels, creation mode `0640`. |

For ID `abcdef`, depth 2 resolves to
`<base>/a/b/sess_abcdef`. Sharding directories are not created implicitly,
matching php-src. An empty configured path resolves to the platform temporary
directory when the handler opens.

Files are opened with no-follow and close-on-exec flags, validated for acceptable
ownership, and locked with EINTR retry. Writes on the held descriptor loop until
all bytes are written, then `fsync` before unlocking. Destroy keeps the lock
through unlink. GC recursively walks exactly the configured depth and excludes
the active session path.

## Serialization and lazy write

| Handler | Layout |
|---|---|
| `php` | Repeated `key|serialize(value)` entries. |
| `php_binary` | Repeated one-byte key length, key, serialized value. |
| `php_serialize` | One serialized top-level session array. |

All formats preserve embedded NUL bytes. `php` rejects keys containing `|`;
`php_binary` enforces its key-length limit.

With `session.lazy_write=1`, unchanged files are timestamped rather than
rewritten. A custom handler that returned the same serialized data receives
`updateTimestamp()` when it implements
`SessionUpdateTimestampHandlerInterface`; otherwise `write()` is used. Setting
lazy write off forces `write()`.

## Cookies and transport

Supported cookie parameters are `lifetime`, `path`, `domain`, `secure`,
`httponly`, and `samesite`. PHP 8.5 adds `partitioned`; Partitioned cookies require Secure;
an invalid combination fails session start rather than sending an invalid
header.

The SID source priority is manual ID, accepted cookie, GET, then POST. GET/POST
transport is considered only with `session.use_only_cookies=0`. Cookies can be
disabled independently with `session.use_cookies=0`.

Trans-SID output rewriting requires `use_trans_sid=1`,
`use_only_cookies=0`, an active non-empty ID, and no accepted cookie transport.
It rewrites configured same-origin tag attributes, hidden form fields, and
same-origin `Location` headers. A Cookie header does not suppress rewriting when
cookies are disabled.

## Runtime configuration

The session INI surface includes save handler/path, cache settings, all cookie
settings, `use_cookies`, strict/cookie-only/trans-SID flags, lazy write,
serialization, GC, SID parameters, auto-start, referer checking, and upload
progress.

Runtime-settable directives report access `7`. The php-src PERDIR directives
`session.auto_start` and `session.upload_progress.*` report access `2` and
cannot be changed through `ini_set()` during a request. Auto-start and the two
deployment upload-progress toggles can be seeded for a worker with the documented
`ELEPHC_SESSION_*` environment variables. The upload tracker also accepts
deployment-time name, save-path, serializer, and cookie-only policy variables,
because body draining necessarily precedes execution of PHP request code. The
same deployment seed is reapplied before PHP execution, so upload tracking and
`session_start()` cannot select different names, paths, serializers, or SID
transport policies for one request.

For `ini_set()`, invalid names, serializers, save handlers, SameSite values,
negative lifetimes, invalid GC divisors, or invalid SID parameters return
`false` without installing the invalid value. PHP 8.4+ emits the upstream
deprecations for legacy SID/GET/POST settings and rejects invalid GC values.
PHP 8.5 requires string keys and scalar string/int/bool option values in
`session_start()`, including an int-compatible `read_and_close` value.

## Upload progress

The request body drain creates a progress tracker for multipart uploads when the
feature is enabled and a valid SID is supplied by Cookie, GET, or an earlier POST
field under non-cookie-only mode. It supports all three serializers.

Each update uses an independent open-lock-read-modify-write-unlock cycle, so the
upload never holds the session lock for its full duration. The progress trigger
field must precede file parts. Frequency and minimum-time throttles match the
session directives. Cleanup removes the completed entry when enabled.

`tmp_name` remains empty because the web server buffers multipart bodies rather
than exposing a PHP temporary upload path. A concurrent polling request observes
progress; the upload handler itself runs only after the body drain completes.

## Compatibility boundary

Within elephc's supported static PHP subset, parity covers PHP-visible return
values, persistence, headers, callback order, file layout, locking, serializer
bytes, and supported configuration. Elephc does not load a host `php.ini` or
dynamically-loadable session modules. The only built-in module is `files`;
custom PHP handlers provide other storage backends. Diagnostics omit Zend
source-file/line suffixes, consistently with the compiler's web-SAPI channel.

## Verification

Focused unit coverage lives in the session submodules of `elephc-web`. HTTP
regressions live in `tests/web_session_tests.rs` and cover lifecycle, binary
payloads, cookies, URL transport, regeneration, custom handlers, INI metadata,
trans-SID behavior, and upload progress. PHP equivalence cases should also be
checked against the matching `ext/session/tests/*.phpt` behavior when changed.
