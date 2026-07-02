# PHP Session Support — Implementation Specification (v2 — Revised)

> Revised after jury review (GLM-5.2 + Kimi K2.7 proxy). Key changes from v1:
> - **C1 fix**: Session state moved to bridge (Rust statics) — no PHP-level globals, no memory leak.
> - **C2 fix**: `define()` guarded with `defined()` check.
> - **C3 fix**: File locking (`flock(LOCK_EX)`) added to bridge session read/write.
> - **H1 fix**: Session ID preserved in bridge for restart after `session_write_close()`.
> - **H2 fix**: Session ID charset is `0-9a-f` (hex).
> - **H3 fix**: `cache_limiter`/`cache_expire` added to bridge state.
> - **H4 fix**: `read_and_close` option and full options handling.
> - **M2 fix**: Single consistent C-ABI interface (string-based).
> - **M4 fix**: `session_set_cookie_params` supports both array and positional forms.
> - **M5 fix**: Error test for CLI-mode usage added.
> - **M6 fix**: Windows limitation documented.
> - **L1 fix**: `session_regenerate_id(false)` does not write data immediately.
> - **L2 fix**: `session_set_save_handler` returns `false` silently (`trigger_error` not available).

## 1. Overview

Implement the full PHP session extension (`session_*()` functions, `$_SESSION`
superglobal, and `PHP_SESSION_*` constants) for elephc's `--web` mode.

**Reference**: https://www.php.net/manual/en/ref.session.php

### Design principles

- Sessions are **`--web` only**: the bridge provides C-ABI session primitives;
  the web prelude defines the PHP-visible functions. In CLI mode, session
  functions are not defined (compile error if called).
- File-based storage (matching PHP's default `session.save_handler = files`).
- Session data uses PHP's `php` serialize handler format
  (`key|serialize($value)` concatenated).
- **Session state lives in the bridge** (Rust process-statics), exposed via
  C-ABI getters/setters. This avoids per-request memory leaks from PHP-level
  globals (which `StoreGlobal` does not release and `__rt_web_reset` does not
  reset). The bridge resets session state at the start of each request.
- The prelude handles session encode/decode (using `serialize()`/`unserialize()`
  builtins) and cookie emission via `header()`.
- Automatic `session_write_close()` at handler end via a `finally` block.
- File locking (`flock(LOCK_EX)`) prevents concurrent requests with the same
  session ID from losing data.

## 2. PHP Session API Surface

### 2.1 Constants

Defined in the web prelude via `define()`, guarded with `defined()` to prevent
re-definition warnings on re-run:

```php
if (!defined('PHP_SESSION_NONE')) {
    define('PHP_SESSION_DISABLED', 0);
    define('PHP_SESSION_NONE', 1);
    define('PHP_SESSION_ACTIVE', 2);
}
```

| Constant | Value | Description |
|---|---|---|
| `PHP_SESSION_DISABLED` | `0` | Sessions disabled (never returned by elephc) |
| `PHP_SESSION_NONE` | `1` | Sessions enabled, no session active |
| `PHP_SESSION_ACTIVE` | `2` | Session active |

### 2.2 Superglobal

`$_SESSION` — added to `src/superglobals.rs::SUPERGLOBALS`. Type:
`AssocArray<Str, Mixed>`. Populated by `session_start()`, written by
`session_write_close()`, reset by `__rt_web_reset` (automatic — it iterates
`SUPERGLOBALS`).

### 2.3 Functions

All 23 PHP session functions, defined in the web prelude as PHP functions:

| Function | PHP Signature | Return | Behavior |
|---|---|---|---|
| `session_start` | `session_start(array $options = []): bool` | `bool` | Start new or resume existing session |
| `session_id` | `session_id(?string $id = null): string\|false` | `string\|false` | Get/set current session ID |
| `session_name` | `session_name(?string $name = null): string\|false` | `string\|false` | Get/set current session name (cookie name) |
| `session_status` | `session_status(): int` | `int` | Return `PHP_SESSION_*` constant |
| `session_destroy` | `session_destroy(): bool` | `bool` | Destroy session data (delete file, clear `$_SESSION`) |
| `session_unset` | `session_unset(): bool` | `bool` | Clear all `$_SESSION` variables (keep session active) |
| `session_write_close` | `session_write_close(bool $commit = true): bool` | `bool` | Write session data and end session |
| `session_commit` | — | — | Alias of `session_write_close()` |
| `session_regenerate_id` | `session_regenerate_id(bool $delete_old = false): bool` | `bool` | Generate new session ID, optionally delete old file |
| `session_create_id` | `session_create_id(string $prefix = ""): string\|false` | `string\|false` | Create new session ID string (does not start session) |
| `session_save_path` | `session_save_path(?string $path = null): string\|false` | `string\|false` | Get/set session save path |
| `session_module_name` | `session_module_name(?string $module = null): string\|false` | `string\|false` | Get/set session module (always "files") |
| `session_encode` | `session_encode(): string\|false` | `string\|false` | Encode `$_SESSION` as session-format string |
| `session_decode` | `session_decode(string $data): bool` | `bool` | Decode session-format string into `$_SESSION` |
| `session_abort` | `session_abort(): bool` | `bool` | Discard `$_SESSION` changes, finish session |
| `session_reset` | `session_reset(): bool` | `bool` | Re-initialize session array with original values |
| `session_gc` | `session_gc(): int\|false` | `int\|false` | Garbage collection, returns deleted session count |
| `session_cache_limiter` | `session_cache_limiter(?string $value = null): string\|false` | `string\|false` | Get/set cache limiter (sends Cache-Control header) |
| `session_cache_expire` | `session_cache_expire(?int $value = null): int\|false` | `int\|false` | Get/set cache expire (minutes) |
| `session_get_cookie_params` | `session_get_cookie_params(): array` | `array` | Return current session cookie parameters |
| `session_set_cookie_params` | `session_set_cookie_params(array\|int\|string ...$args): bool` | `bool` | Set session cookie parameters (array or positional form) |
| `session_register_shutdown` | `session_register_shutdown(): void` | `void` | No-op (auto via `finally`) |
| `session_set_save_handler` | `session_set_save_handler(): bool` | `bool` | Not supported — returns `false` silently |

## 3. Architecture

### 3.1 Layer overview

```
┌─────────────────────────────────────────────────────────────────────┐
│  User PHP code (--web)                                              │
│  session_start(); $_SESSION['k'] = 'v'; echo $_SESSION['k'];       │
└───────────┬─────────────────────────────────────────────────────────┘
            │ calls
┌───────────▼─────────────────────────────────────────────────────────┐
│  Web prelude (PHP source, prepended by src/web_prelude.rs)          │
│  - session_start(): calls bridge to read, decodes data into $_SESSION│
│  - session_write_close(): encodes $_SESSION, calls bridge to write  │
│  - All session_*() functions call bridge getters/setters for state  │
│  - __elephc_session_encode/decode helpers                          │
│  - define() for PHP_SESSION_* constants (guarded)                  │
│  - extern "elephc_web" declarations for bridge C-ABI functions     │
└───────────┬─────────────────────────────────────────────────────────┘
            │ C-ABI calls
┌───────────▼─────────────────────────────────────────────────────────┐
│  Bridge (crates/elephc-web/src/session.rs)                          │
│  - Session state (name, ID, status, save_path, cookie params,      │
│    cache_limiter, cache_expire, data snapshot, file fd + lock)      │
│  - Session file read/write/delete (with flock(LOCK_EX))            │
│  - Session ID generation (/dev/urandom, 32 hex chars)              │
│  - Session GC (delete expired files)                               │
│  - Session format parser (split key|serialized_value boundaries)    │
│  - Session state reset (called at start of each request)            │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.2 Bridge C-ABI interface

All functions are `#[no_mangle] extern "C"` in `crates/elephc-web/src/session.rs`.
String parameters follow elephc's extern ABI: `(ptr, len)` in registers.
String returns use a per-worker static buffer (valid until next session call;
the compiler copies immediately after the call).

```c
// ── Session state getters/setters ──
const char* elephc_web_session_get_name(void);          // → current session name
void        elephc_web_session_set_name(const char* name, int64_t len);
int64_t     elephc_web_session_get_id(char* buf, int64_t buf_len);  // → ID length, 0 if none
int64_t     elephc_web_session_set_id(const char* id, int64_t len); // → 1 on success
int64_t     elephc_web_session_get_status(void);        // → 0=disabled, 1=none, 2=active
void        elephc_web_session_set_status(int64_t status);
const char* elephc_web_session_get_save_path(void);
void        elephc_web_session_set_save_path(const char* path, int64_t len);
const char* elephc_web_session_get_cache_limiter(void);
void        elephc_web_session_set_cache_limiter(const char* v, int64_t len);
int64_t     elephc_web_session_get_cache_expire(void);
void        elephc_web_session_set_cache_expire(int64_t v);

// ── Cookie params (6 fields: lifetime, path, domain, secure, httponly, samesite) ──
int64_t     elephc_web_session_get_cookie_lifetime(void);
const char* elephc_web_session_get_cookie_path(void);
const char* elephc_web_session_get_cookie_domain(void);
int64_t     elephc_web_session_get_cookie_secure(void);
int64_t     elephc_web_session_get_cookie_httponly(void);
const char* elephc_web_session_get_cookie_samesite(void);
void        elephc_web_session_set_cookie_params(
    int64_t lifetime, const char* path, int64_t path_len,
    const char* domain, int64_t domain_len,
    int64_t secure, int64_t httponly,
    const char* samesite, int64_t samesite_len);

// ── Session file operations (with flock) ──
// read_and_close=1: read data then release lock and close fd immediately
const char* elephc_web_session_read(
    const char* id, int64_t id_len,
    const char* save_path, int64_t save_path_len,
    int64_t read_and_close);                          // → session data string (or empty)
int64_t     elephc_web_session_write(
    const char* id, int64_t id_len,
    const char* save_path, int64_t save_path_len,
    const char* data, int64_t data_len);              // → 1 on success
int64_t     elephc_web_session_destroy(
    const char* id, int64_t id_len,
    const char* save_path, int64_t save_path_len);    // → 1 on success
int64_t     elephc_web_session_abort(
    const char* id, int64_t id_len,
    const char* save_path, int64_t save_path_len);    // → 1 (release lock, discard)

// ── Session ID generation ──
const char* elephc_web_session_create_id(
    const char* prefix, int64_t prefix_len);          // → new 32-hex ID (+prefix)

// ── Garbage collection ──
int64_t     elephc_web_session_gc(
    const char* save_path, int64_t save_path_len,
    int64_t maxlifetime);                             // → deleted file count

// ── Session format parser ──
int64_t     elephc_web_session_count_entries(
    const char* data, int64_t data_len);              // → entry count
const char* elephc_web_session_entry_key(
    const char* data, int64_t data_len, int64_t idx); // → key string
const char* elephc_web_session_entry_value(
    const char* data, int64_t data_len, int64_t idx); // → serialized value string

// ── Per-request state reset (called by prelude at start of each request) ──
void        elephc_web_session_reset(void);
```

### 3.3 Session state (bridge)

All state lives in per-worker Rust process-statics (single-threaded per worker):

| State | Default | Type |
|---|---|---|
| Session name | `"PHPSESSID"` | `Option<CString>` |
| Session ID | `""` (none) | `Option<CString>` |
| Session status | `PHP_SESSION_NONE` (1) | `i64` |
| Save path | `sys_get_temp_dir()` | `Option<CString>` |
| Cache limiter | `"nocache"` | `Option<CString>` |
| Cache expire | `180` (minutes) | `i64` |
| Cookie lifetime | `0` | `i64` |
| Cookie path | `"/"` | `Option<CString>` |
| Cookie domain | `""` | `Option<CString>` |
| Cookie secure | `false` | `bool` |
| Cookie httponly | `true` | `bool` |
| Cookie samesite | `"Lax"` | `Option<CString>` |
| Data snapshot | `""` | `Vec<u8>` (for reset/abort) |
| Session file fd | `-1` (none) | `i32` (held open with flock) |

`elephc_web_session_reset()` is called by the prelude at the start of each
request. It:
1. Releases the session file lock if held (flock + close).
2. Resets all state to defaults.
3. Clears the data snapshot.

### 3.4 File locking

- `elephc_web_session_read()`: Opens the session file with `O_RDWR | O_CREAT`,
  calls `flock(LOCK_EX)`, reads the content, stores the fd. The lock is held
  until `session_write`, `session_destroy`, `session_abort`, or
  `session_reset` releases it.
- `read_and_close=1`: Read the data, then `flock(LOCK_UN)` + `close()`
  immediately. No write happens at handler end. The status is set to NONE
  after reading.
- `elephc_web_session_write()`: Writes data to the held fd (truncate + write),
  then `flock(LOCK_UN)` + `close()`.
- `elephc_web_session_destroy()`: Truncates/deletes the file, releases lock.
- `elephc_web_session_abort()`: Releases lock without writing (discard changes).
- Atomic write: write to temp file, fsync, rename over the original. This
  prevents partial-write corruption if the process crashes mid-write.

### 3.5 Session lifecycle

```
Request arrives
  │
  ▼
__rt_web_reset (clears $_SESSION, other superglobals, statics)
  │
  ▼
Web prelude executes:
  │  elephc_web_session_reset() — reset bridge session state
  │  if (!defined('PHP_SESSION_NONE')) { define(...); }
  │  function session_start() { ... }
  │  function session_write_close() { ... }
  │  ... (all session function definitions)
  ▼
User handler body:
  │  session_start();
  │    → bridge: get name, get ID from cookie or generate new
  │    → bridge: session_read(id, save_path, read_and_close=0)
  │    → bridge: count_entries / entry_key / entry_value
  │    → unserialize each value, populate $_SESSION
  │    → send session cookie via header()
  │    → bridge: set status = ACTIVE
  │    → save data snapshot in bridge (for reset/abort)
  │  $_SESSION['count'] = ($_SESSION['count'] ?? 0) + 1;
  │
  ▼
Handler body ends → finally block:
  │  if (session_status() === PHP_SESSION_ACTIVE) {
  │      session_write_close();
  │  → encode $_SESSION: foreach → key . '|' . serialize($value)
  │  → bridge: session_write(id, save_path, encoded_data)
  │  → bridge: set status = NONE
  │  }
  ▼
Bridge flushes response to client
```

### 3.6 Session format parser (bridge)

`skip_serialized_value(data, pos)` finds the end of one PHP serialized value:

```
match data[pos]:
  'N' → pos + 2 (N;)
  'b' → skip to ';' after b:0 or b:1
  'i' → skip to ';' after i:<number>
  'd' → skip to ';' after d:<number>
  's' → read s:<len>:"...", skip len bytes + closing ";
  'a' → read a:<count>:{, recursively skip count*2 values, skip }
  'O' → read O:<namelen>:"<name>":<count>:{, skip count*2 values, skip }
  'C' → read C:<namelen>:"<name>":<datalen>:{<data>}, skip datalen bytes, skip }
  default → error (invalid format)
```

Returns byte position after one complete serialized value. The session format
is split into `(key, serialized_value)` pairs: key = everything before first
`|`, value = one serialized value starting after `|`.

### 3.7 Session ID generation

- 32 hexadecimal characters (128 bits entropy), matching PHP's default
  `session.sid_length = 32`.
- Charset: `0-9a-f` (hex, matching `session.sid_bits_per_character = 4`).
- Optional prefix (for `session_create_id($prefix)`).
- Random source: `/dev/urandom` (macOS + Linux). Windows is not supported for
  `--web` mode (documented limitation).

### 3.8 Session ID validation

IDs from cookies are validated before use:
- Length: 1–128 characters
- Characters: `a-zA-Z0-9,-` (most permissive, matches PHP 6-bit mode)
- Invalid IDs are rejected (new ID generated instead)

## 4. Web Prelude Implementation

### 4.1 Extern declarations (added to `WEB_PRELUDE_SRC`)

```php
extern "elephc_web" {
    function elephc_web_session_reset(): void;
    function elephc_web_session_get_name(): string;
    function elephc_web_session_set_name(string $name): void;
    function elephc_web_session_get_id(): string;
    function elephc_web_session_set_id(string $id): int;
    function elephc_web_session_get_status(): int;
    function elephc_web_session_set_status(int $status): void;
    function elephc_web_session_get_save_path(): string;
    function elephc_web_session_set_save_path(string $path): void;
    function elephc_web_session_get_cache_limiter(): string;
    function elephc_web_session_set_cache_limiter(string $v): void;
    function elephc_web_session_get_cache_expire(): int;
    function elephc_web_session_set_cache_expire(int $v): void;
    function elephc_web_session_get_cookie_lifetime(): int;
    function elephc_web_session_get_cookie_path(): string;
    function elephc_web_session_get_cookie_domain(): string;
    function elephc_web_session_get_cookie_secure(): int;
    function elephc_web_session_get_cookie_httponly(): int;
    function elephc_web_session_get_cookie_samesite(): string;
    function elephc_web_session_set_cookie_params(
        int $lifetime, string $path, string $domain,
        int $secure, int $httponly, string $samesite
    ): void;
    function elephc_web_session_read(
        string $id, string $save_path, int $read_and_close
    ): string;
    function elephc_web_session_write(
        string $id, string $save_path, string $data
    ): int;
    function elephc_web_session_destroy(
        string $id, string $save_path
    ): int;
    function elephc_web_session_abort(
        string $id, string $save_path
    ): int;
    function elephc_web_session_create_id(string $prefix): string;
    function elephc_web_session_gc(
        string $save_path, int $maxlifetime
    ): int;
    function elephc_web_session_count_entries(string $data): int;
    function elephc_web_session_entry_key(string $data, int $idx): string;
    function elephc_web_session_entry_value(string $data, int $idx): string;
}
```

### 4.2 Constants and reset (added to start of `WEB_PRELUDE_SRC`)

```php
elephc_web_session_reset();
if (!defined('PHP_SESSION_NONE')) {
    define('PHP_SESSION_DISABLED', 0);
    define('PHP_SESSION_NONE', 1);
    define('PHP_SESSION_ACTIVE', 2);
}
```

### 4.3 Session function implementations

```php
function session_start(array $options = []): bool {
    $status = elephc_web_session_get_status();
    if ($status === 2) { return true; }  // already active

    // Apply options
    if (isset($options['name'])) {
        elephc_web_session_set_name($options['name']);
    }
    if (isset($options['save_path'])) {
        elephc_web_session_set_save_path($options['save_path']);
    }
    $read_and_close = 0;
    if (isset($options['read_and_close']) && $options['read_and_close']) {
        $read_and_close = 1;
    }
    if (isset($options['cookie_lifetime'])) {
        // Update cookie params individually
    }
    if (isset($options['cookie_path'])) { /* ... */ }
    if (isset($options['cookie_domain'])) { /* ... */ }
    if (isset($options['cookie_secure'])) { /* ... */ }
    if (isset($options['cookie_httponly'])) { /* ... */ }
    if (isset($options['cookie_samesite'])) { /* ... */ }
    if (isset($options['cache_limiter'])) {
        elephc_web_session_set_cache_limiter($options['cache_limiter']);
    }
    if (isset($options['cache_expire'])) {
        elephc_web_session_set_cache_expire($options['cache_expire']);
    }

    $name = elephc_web_session_get_name();
    $save_path = elephc_web_session_get_save_path();

    // Get session ID: check bridge first (set via session_id() before start),
    // then fall back to cookie, then generate new
    $id = elephc_web_session_get_id();
    if ($id === '') {
        if (isset($_COOKIE[$name])) {
            $id = $_COOKIE[$name];
            // Bridge validates; if invalid, generate new
        }
    }
    if ($id === '') {
        $id = elephc_web_session_create_id('');
    }
    elephc_web_session_set_id($id);

    // Read session file
    $raw = elephc_web_session_read($id, $save_path, $read_and_close);
    if ($raw !== '') {
        __elephc_session_decode($raw);
    } else {
        $_SESSION = [];
    }

    // Send session cookie (skip if read_and_close)
    if ($read_and_close === 0) {
        __elephc_session_send_cookie();
    }

    // Set status
    if ($read_and_close === 1) {
        // read_and_close: status goes back to NONE after reading
        elephc_web_session_set_status(1);  // PHP_SESSION_NONE
    } else {
        elephc_web_session_set_status(2);  // PHP_SESSION_ACTIVE
    }

    // Send cache limiter headers
    __elephc_session_send_cache_headers();

    return true;
}

function session_write_close(bool $commit = true): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }

    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    $encoded = __elephc_session_encode();
    elephc_web_session_write($id, $save_path, $encoded);
    elephc_web_session_set_status(1);  // PHP_SESSION_NONE
    return true;
}

function session_destroy(): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }

    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    elephc_web_session_destroy($id, $save_path);
    $_SESSION = [];
    elephc_web_session_set_status(1);  // PHP_SESSION_NONE
    elephc_web_session_set_id('');
    return true;
}

function session_id(?string $id = null): string|false {
    if ($id !== null && elephc_web_session_get_status() === 2) {
        return false;  // cannot set ID when session is active
    }
    $old = elephc_web_session_get_id();
    if ($id !== null) { elephc_web_session_set_id($id); }
    return $old;
}

function session_name(?string $name = null): string|false {
    if ($name !== null && elephc_web_session_get_status() === 2) {
        return false;  // cannot set name when session is active
    }
    $old = elephc_web_session_get_name();
    if ($name !== null) { elephc_web_session_set_name($name); }
    return $old;
}

function session_status(): int {
    return elephc_web_session_get_status();
}

function session_unset(): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    $_SESSION = [];
    return true;
}

function session_encode(): string|false {
    if (elephc_web_session_get_status() !== 2) { return false; }
    return __elephc_session_encode();
}

function session_decode(string $data): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    __elephc_session_decode($data);
    return true;
}

function session_save_path(?string $path = null): string|false {
    if ($path !== null && elephc_web_session_get_status() === 2) { return false; }
    $old = elephc_web_session_get_save_path();
    if ($path !== null) { elephc_web_session_set_save_path($path); }
    return $old;
}

function session_regenerate_id(bool $delete_old = false): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }

    $old_id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();

    if ($delete_old) {
        elephc_web_session_destroy($old_id, $save_path);
    }
    // Release the old file lock if not deleting
    // (bridge handles this internally on destroy/abort/write)

    $new_id = elephc_web_session_create_id('');
    elephc_web_session_set_id($new_id);

    // Re-send cookie with new ID
    __elephc_session_send_cookie();
    // Data will be written at session_write_close (finally block)
    return true;
}

function session_create_id(string $prefix = ""): string|false {
    return elephc_web_session_create_id($prefix);
}

function session_gc(): int|false {
    $save_path = elephc_web_session_get_save_path();
    return elephc_web_session_gc($save_path, 1440);
}

function session_abort(): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    elephc_web_session_abort($id, $save_path);  // release lock, discard
    $_SESSION = [];
    elephc_web_session_set_status(1);  // PHP_SESSION_NONE
    return true;
}

function session_reset(): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    // Re-read the original session data from the file
    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    // Bridge re-reads (data still locked) and returns snapshot
    $raw = elephc_web_session_read($id, $save_path, 0);
    $_SESSION = [];
    if ($raw !== '') { __elephc_session_decode($raw); }
    return true;
}

function session_cache_limiter(?string $value = null): string|false {
    if ($value !== null && elephc_web_session_get_status() === 2) { return false; }
    $old = elephc_web_session_get_cache_limiter();
    if ($value !== null) { elephc_web_session_set_cache_limiter($value); }
    return $old;
}

function session_cache_expire(?int $value = null): int|false {
    if ($value !== null && elephc_web_session_get_status() === 2) { return false; }
    $old = elephc_web_session_get_cache_expire();
    if ($value !== null) { elephc_web_session_set_cache_expire($value); }
    return $old;
}

function session_get_cookie_params(): array {
    return [
        'lifetime' => elephc_web_session_get_cookie_lifetime(),
        'path' => elephc_web_session_get_cookie_path(),
        'domain' => elephc_web_session_get_cookie_domain(),
        'secure' => (bool)elephc_web_session_get_cookie_secure(),
        'httponly' => (bool)elephc_web_session_get_cookie_httponly(),
        'samesite' => elephc_web_session_get_cookie_samesite(),
    ];
}

function session_set_cookie_params(...$args): bool {
    if (count($args) === 1 && is_array($args[0])) {
        $o = $args[0];
        elephc_web_session_set_cookie_params(
            $o['lifetime'] ?? 0,
            $o['path'] ?? '/',
            $o['domain'] ?? '',
            (int)($o['secure'] ?? false),
            (int)($o['httponly'] ?? true),
            $o['samesite'] ?? 'Lax'
        );
    } else {
        // Legacy positional: lifetime, path, domain, secure, httponly, samesite
        elephc_web_session_set_cookie_params(
            $args[0] ?? 0,
            $args[1] ?? '/',
            $args[2] ?? '',
            (int)($args[3] ?? false),
            (int)($args[4] ?? true),
            $args[5] ?? 'Lax'
        );
    }
    return true;
}

function session_commit(): bool {
    return session_write_close();
}

function session_register_shutdown(): void {
    // No-op: auto-close via the finally block
}

function session_module_name(?string $module = null): string|false {
    if ($module !== null && $module !== 'files') { return false; }
    return 'files';
}

function session_set_save_handler(): bool {
    // Not supported in elephc (file handler only). trigger_error() is not
    // available, so we silently return false.
    return false;
}
```

### 4.4 Session encode/decode helpers

```php
function __elephc_session_encode(): string {
    $out = '';
    foreach ($_SESSION as $k => $v) {
        $out .= $k . '|' . serialize($v);
    }
    return $out;
}

function __elephc_session_decode(string $raw): void {
    $count = elephc_web_session_count_entries($raw);
    for ($i = 0; $i < $count; $i++) {
        $key = elephc_web_session_entry_key($raw, $i);
        $val = elephc_web_session_entry_value($raw, $i);
        $_SESSION[$key] = unserialize($val);
    }
}
```

### 4.5 Cookie emission helper

```php
function __elephc_session_send_cookie(): void {
    $name = elephc_web_session_get_name();
    $id = elephc_web_session_get_id();
    $lifetime = elephc_web_session_get_cookie_lifetime();
    $path = elephc_web_session_get_cookie_path();
    $domain = elephc_web_session_get_cookie_domain();
    $secure = (bool)elephc_web_session_get_cookie_secure();
    $httponly = (bool)elephc_web_session_get_cookie_httponly();
    $samesite = elephc_web_session_get_cookie_samesite();

    $cookie = $name . '=' . $id;
    if ($lifetime > 0) {
        $cookie .= '; expires=' . gmdate('D, d-M-Y H:i:s', time() + $lifetime) . ' GMT';
        $cookie .= '; Max-Age=' . $lifetime;
    }
    if ($path !== '') { $cookie .= '; path=' . $path; }
    if ($domain !== '') { $cookie .= '; domain=' . $domain; }
    if ($secure) { $cookie .= '; secure'; }
    if ($httponly) { $cookie .= '; HttpOnly'; }
    if ($samesite !== '') { $cookie .= '; SameSite=' . $samesite; }
    header('Set-Cookie: ' . $cookie, false);
}
```

### 4.6 Cache headers helper

```php
function __elephc_session_send_cache_headers(): void {
    $limiter = elephc_web_session_get_cache_limiter();
    if ($limiter === 'nocache') {
        header('Cache-Control: no-store, no-cache, must-revalidate');
        header('Expires: Thu, 19 Nov 1981 08:52:00 GMT');
    } elseif ($limiter === 'public') {
        $expire = elephc_web_session_get_cache_expire();
        header('Cache-Control: public, max-age=' . ($expire * 60));
    } elseif ($limiter === 'private') {
        $expire = elephc_web_session_get_cache_expire();
        header('Cache-Control: private, max-age=' . ($expire * 60));
    } elseif ($limiter === 'private_no_expire') {
        header('Cache-Control: private, max-age=' . (elephc_web_session_get_cache_expire() * 60));
    }
    // '' (empty string) = no cache headers sent
}
```

### 4.7 Catch-all wrapper modification

The existing `WEB_WRAP_SRC` is extended with a `finally` block:

```php
<?php try { $__elephc_wrap = 0; } catch (\Throwable $__elephc_exc) {
    http_response_code(500);
} finally {
    if (elephc_web_session_get_status() === 2) { session_write_close(); }
}
```

## 5. Files to Create or Modify

### 5.1 New files

| File | Description |
|---|---|
| `crates/elephc-web/src/session.rs` | Bridge session module (~500 LOC) |
| `tests/codegen/web/session_basic.rs` | Codegen tests: basic session operations |
| `tests/codegen/web/session_encode.rs` | Codegen tests: encode/decode round-trip |
| `tests/codegen/web/session_lifecycle.rs` | Codegen tests: destroy, unset, regenerate, abort, reset |
| `tests/web_session_tests.rs` | End-to-end web tests: cookie round-trip, persistence, locking |
| `examples/web-session/main.php` | Example: session counter |
| `examples/web-session/.gitignore` | `*.s`, `*.o`, `main` |

### 5.2 Modified files

| File | Change |
|---|---|
| `src/superglobals.rs` | Add `"_SESSION"` to `SUPERGLOBALS` |
| `src/web_prelude.rs` | Add session externs, constants, functions; add `finally` to wrapper |
| `crates/elephc-web/src/lib.rs` | Add `mod session;` and re-exports |
| `docs/beyond-php/web.md` | Remove "No sessions" limitation; add session documentation |
| `docs/php/sessions.md` | New page: session function reference |
| `docs/README.md` | Add sessions page to index |
| `ROADMAP.md` | Mark session support as completed |

## 6. Test Plan

### 6.1 Codegen tests

| Test | What it verifies |
|---|---|
| `session_start_basic` | `session_start()` returns `true`, `session_status()` is ACTIVE |
| `session_id_get_set` | `session_id()` returns ID, setting before start works |
| `session_name_get_set` | `session_name()` returns `"PHPSESSID"`, custom name works |
| `session_encode_decode` | Encode/decode round-trips correctly |
| `session_unset` | `session_unset()` clears `$_SESSION`, keeps session active |
| `session_destroy` | `session_destroy()` clears session, status → NONE |
| `session_write_close` | `session_write_close()` writes, status → NONE |
| `session_regenerate_id` | `session_regenerate_id()` generates new ID |
| `session_create_id` | `session_create_id()` returns valid ID |
| `session_save_path` | `session_save_path()` get/set |
| `session_abort` | `session_abort()` discards changes |
| `session_reset` | `session_reset()` re-loads original data |
| `session_cookie_params` | `session_get_cookie_params()` defaults, `session_set_cookie_params()` |
| `session_cache_limiter` | `session_cache_limiter()` get/set |
| `session_constants` | All 3 `PHP_SESSION_*` constants defined |
| `session_commit_alias` | `session_commit()` = `session_write_close()` |
| `session_module_name` | Returns `"files"` |
| `session_set_save_handler_unsupported` | Returns `false`, emits warning |
| `session_cli_mode_error` | Error test: calling `session_start()` in CLI mode (no prelude) |

### 6.2 End-to-end web tests

| Test | What it verifies |
|---|---|
| `session_cookie_round_trip` | First request sends `Set-Cookie`; second request with cookie persists data |
| `session_counter_persists` | Counter in `$_SESSION` increments across requests |
| `session_destroy_clears` | `session_destroy()` stops cookie and clears data |
| `session_regenerate_id` | New cookie with new ID |
| `session_name_custom` | Custom session name in cookie |
| `session_concurrent_locking` | Two simultaneous requests with same session ID don't lose data |

### 6.3 Example

`examples/web-session/main.php`:
```php
<?php
session_start();
if (!isset($_SESSION['count'])) {
    $_SESSION['count'] = 0;
}
$_SESSION['count']++;
header('Content-Type: text/plain');
echo "Visits: " . $_SESSION['count'] . "\n";
echo "Session ID: " . session_id() . "\n";
```

## 7. Implementation Order (for sub-agent delegation)

### Phase 1: Bridge (`crates/elephc-web/src/session.rs`)
1. Session state (Rust statics) + `elephc_web_session_reset()`
2. State getters/setters (name, ID, status, save_path, cache, cookie params)
3. Session ID generation (`/dev/urandom`, 32 hex)
4. Session ID validation
5. Session file read with `flock(LOCK_EX)` + fd management
6. Session file write (atomic: temp + rename) + lock release
7. Session destroy + lock release
8. Session abort (lock release, no write)
9. Session GC
10. Session format parser (`skip_serialized_value`)
11. Count/key/value entry functions
12. C-ABI exports and `lib.rs` wiring
13. Unit tests

### Phase 2: Compiler infrastructure
1. Add `"_SESSION"` to `src/superglobals.rs::SUPERGLOBALS`
2. Verify `__rt_web_reset` handles `$_SESSION` (automatic)

### Phase 3: Web prelude (`src/web_prelude.rs`)
1. Add `extern "elephc_web"` declarations
2. Add `elephc_web_session_reset()` call and guarded `define()` constants
3. Implement all 23 session functions + helpers
4. Add `finally` block to `WEB_WRAP_SRC`

### Phase 4: Tests
1. Codegen tests
2. End-to-end web tests

### Phase 5: Docs and examples
1. Update `docs/beyond-php/web.md`
2. Create `docs/php/sessions.md`
3. Update `docs/README.md`
4. Create `examples/web-session/`
5. Update `ROADMAP.md`

## 8. Known Limitations

1. **File handler only**: `session_set_save_handler()` not supported (returns `false` silently; `trigger_error()` is not available in elephc).
2. **No `ini_get`/`ini_set`**: Configuration via functions only.
3. **`--web` mode only**: Session functions not defined in CLI mode.
4. **`SID` constant**: Not defined (deprecated in PHP 8.4).
5. **Session upload progress**: Not supported.
6. **No automatic GC**: Call `session_gc()` explicitly. (PHP's gc_probability
   can be added later by calling `session_gc()` with probability in the prelude.)
7. **Windows**: `--web` mode not supported on `windows-x86_64`; sessions
   inherit this limitation.
8. **No effects modeling needed**: Session functions are user-defined PHP
   functions (not builtins), so the optimizer treats them conservatively (impure)
   automatically. No `catalog.rs` or `effects/builtins.rs` changes needed.
