---
title: "Sessions"
description: "PHP session functions for persistent state across HTTP requests under --web."
sidebar:
  order: 12
---

elephc provides PHP session support when compiling with `--web`. Sessions allow
your application to persist state across HTTP requests using the standard PHP
session functions and the `$_SESSION` superglobal.

Sessions are only available under `--web` — they require the HTTP request/response
lifecycle provided by the web server binary. A CLI-compiled program has no session
context.

Select the PHP minor whose observable session behavior should be compiled with
`--php-version=8.2`, `8.3`, `8.4`, or `8.5`. The default is `8.5`. PHP 8.4
enables the upstream session deprecations and stricter GC configuration checks;
PHP 8.5 additionally exposes CHIPS (`cookie_partitioned`) and the hardened
`session_start()` option-map rules.

## Session lifecycle

A session follows three phases within each request:

1. **Start** — `session_start()` loads (or creates) the session identified by the
   `PHPSESSID` cookie and populates `$_SESSION`.
2. **Use** — read and write `$_SESSION` like any other array.
3. **Write close** — `session_write_close()` flushes `$_SESSION` back to the
   session file and releases the lock. elephc calls this automatically at handler
   end via a finally block, so you rarely need to call it yourself.

```php
<?php
session_start();

$_SESSION['user'] = 'alice';
$_SESSION['last_seen'] = time();

echo "Hello, " . $_SESSION['user'] . "!\n";
```

## The `$_SESSION` superglobal

`$_SESSION` is a superglobal associative array, readable in any function scope
without a `global` declaration. It is populated by `session_start()` and flushed
by `session_write_close()`.

```php
<?php
session_start();

function greet() {
    // No `global` needed — $_SESSION is a superglobal
    if (isset($_SESSION['name'])) {
        echo "Welcome back, " . $_SESSION['name'] . "!\n";
    }
}
```

## Session status constants

| Constant | Value | Meaning |
|---|---|---|
| `PHP_SESSION_DISABLED` | `0` | Sessions are disabled (never returned by elephc). |
| `PHP_SESSION_NONE` | `1` | Sessions are enabled but none is active. |
| `PHP_SESSION_ACTIVE` | `2` | A session is active (`session_start()` has been called). |

Use `session_status()` to check the current state.

## Function reference

| Function | Signature | Description |
|---|---|---|
| `session_start()` | `session_start(array $options = []): bool` | Start a new or resume an existing session. Returns `true` on success. |
| `session_id()` | `session_id(?string $id = null): string\|false` | Get or set the current session ID. With no active session the getter normally returns an empty string. With a string, sets the ID before `session_start()`. |
| `session_name()` | `session_name(?string $name = null): string\|false` | Get or set the session name (used as the cookie name). Default is `PHPSESSID`. Must be called before `session_start()`. |
| `session_status()` | `session_status(): int` | Returns `PHP_SESSION_NONE` or `PHP_SESSION_ACTIVE`. |
| `session_save_path()` | `session_save_path(?string $path = null): string\|false` | Get or set the configured files-handler path. An empty configured value resolves to `sys_get_temp_dir()` when opened. |
| `session_write_close()` | `session_write_close(): bool` | Write session data and end the session. Called automatically at handler end. |
| `session_regenerate_id()` | `session_regenerate_id(bool $delete_old = false): bool` | Generate a new session ID, optionally deleting the old session file. |
| `session_unset()` | `session_unset(): bool` | Unset all `$_SESSION` variables (the array stays, values are removed). |
| `session_destroy()` | `session_destroy(): bool` | Destroy the session. Does not unset `$_SESSION` — call `session_unset()` first if needed. |
| `session_set_cookie_params()` | `session_set_cookie_params(array\|int $options_or_lifetime, ...): bool` | Set session cookie parameters. The array form supports lifetime, path, domain, secure, httponly, and samesite, plus partitioned on PHP 8.5; the positional form matches PHP's five arguments. |
| `session_get_cookie_params()` | `session_get_cookie_params(): array` | Returns the current session cookie parameters as an associative array. |
| `session_start()` options | | Accepts the supported `session.*` runtime directives without the `session.` prefix, plus `read_and_close`. |

## Cookie handling

On the first request (no `PHPSESSID` cookie), `session_start()` generates a
random session ID and sends a `Set-Cookie: PHPSESSID=<id>` header. On subsequent
requests the browser sends the cookie back and `session_start()` loads the
matching session file.

Customize the cookie with `session_set_cookie_params()` or by passing an
`$options` array to `session_start()`:

```php
<?php
session_start([
    'cookie_lifetime' => 3600,
    'cookie_secure' => true,
    'cookie_partitioned' => true,
    'cookie_httponly' => true,
    'cookie_samesite' => 'Lax',
]);
```

`cookie_partitioned` requires the PHP 8.5 profile and also requires
`cookie_secure=true`. Across every maintained profile, PHP's defaults are
`cookie_httponly=0`, an empty SameSite value, and `use_strict_mode=0`.
Those defaults are preserved for compatibility, not presented as secure
deployment guidance. Internet-facing applications should normally enable
`cookie_httponly`, `use_strict_mode`, `cookie_secure` on HTTPS, and an
appropriate SameSite policy explicitly.

## File storage

Session data is stored in files using PHP's default `session.save_handler = files`
model. Each session is a single file named `sess_<id>` containing the serialized
`$_SESSION` array.

The configured default is the empty string, which the files handler resolves to
`sys_get_temp_dir()`. Change the directory with `session_save_path()`:

```php
<?php
session_save_path('/var/www/sessions');
session_start();
```

The directory must exist and be writable by the server process.

Like php-src's files handler, the path may use `[depth;[mode;]]path`. For
example, `2;0640;/var/www/sessions` stores ID `abcdef` at
`/var/www/sessions/a/b/sess_abcdef` with creation mode `0640`. Sharding
directories must already exist. Expiry GC follows the configured directory
depth recursively.

File locking (`flock`) is used during `session_start()` and
`session_write_close()` to prevent concurrent requests on the same session ID
from corrupting each other's data.

## Custom save handlers

Register a custom storage backend (database, Redis, etc.) with
`session_set_save_handler()` using the object form — a class implementing
`SessionHandlerInterface`:

```php
<?php
class RedisSessionHandler implements SessionHandlerInterface {
    public function open(string $path, string $name): bool { /* connect */ return true; }
    public function close(): bool { return true; }
    public function read(string $id): string|false { /* fetch */ return ''; }
    public function write(string $id, string $data): bool { /* store */ return true; }
    public function destroy(string $id): bool { /* delete */ return true; }
    public function gc(int $max_lifetime): int|false { /* purge */ return 0; }
}
session_set_save_handler(new RedisSessionHandler());
session_start();
```

A handler that returns `false` from `read()` aborts `session_start()` (it returns
`false` and the status stays `PHP_SESSION_NONE`), matching PHP. A handler may
also implement `SessionIdInterface` (`create_sid()`) to customize session ID
generation, and `SessionUpdateTimestampHandlerInterface` (`validateId()`,
`updateTimestamp()`) to participate in strict-mode validation and `lazy_write`.
`session_module_name()` returns `"user"` once a handler is registered. The
built-in `SessionHandler` class wraps the default file storage and can be
subclassed to decorate it (e.g. encrypt on `write()`, decrypt on `read()`).

The legacy 6-callable form (deprecated in PHP 8.4) is also supported: pass six
callables — `$open`, `$close`, `$read`, `$write`, `$destroy`, `$gc` — plus the
optional `$create_sid`, `$validate_id`, `$update_timestamp`:

```php
<?php
function my_open(string $path, string $name): bool { /* connect */ return true; }
function my_close(): bool { return true; }
function my_read(string $id): string|false { /* fetch */ return ''; }
function my_write(string $id, string $data): bool { /* store */ return true; }
function my_destroy(string $id): bool { /* delete */ return true; }
function my_gc(int $max_lifetime): int|false { /* purge */ return 0; }
session_set_save_handler('my_open', 'my_close', 'my_read', 'my_write', 'my_destroy', 'my_gc');
session_start();
```

Each callable may be a function-name string, a closure, an invokable object, an
instance array callable (`[$object, 'method']`), or a static array callable
(`['Class', 'method']`).

## Serialize handlers, strict mode, and GC

- **Serialize handler** — `session.serialize_handler` (`php`, `php_serialize`,
  `php_binary`) is selectable via the `session_start(['serialize_handler' => …])`
  option. The default `php` handler rejects `$_SESSION` keys containing `|`;
  `php_serialize` has no key restrictions.
- **Strict mode** — `session.use_strict_mode` defaults to off, matching PHP. When enabled it rejects
  client-supplied session IDs that do not already exist server-side and
  generates a fresh ID instead (session-fixation defense).
- **Garbage collection** — `session_start()` runs GC probabilistically per
  `gc_probability`/`gc_divisor` (defaults `1`/`100`); `gc_maxlifetime` (default
  `1440` s) sets the expiry. All three are settable via `session_start()`
  options. `session_gc()` runs it manually.
- **Cache limiter** — `session_cache_limiter()` / the `cache_limiter` option
  control client-cache headers (`nocache`, `public`, `private`,
  `private_no_expire`, `''`); `session_cache_expire()` sets the lifetime in
  minutes.

## Runtime configuration (`ini_get` / `ini_set`)

Under `--web`, elephc exposes the `session.*` directive surface through
`ini_get()`, `ini_set()`, and `ini_get_all()`. The layer is scoped to
`session.*`: any other directive returns `false` from `ini_get()`/`ini_set()`
and is omitted from `ini_get_all()`. Values follow PHP's `ini_get` string
convention — integers as decimals, booleans as `'1'`/`''`, strings verbatim —
and `ini_set()` returns the previous value.

```php
ini_set('session.gc_maxlifetime', '3600');
$ttl = ini_get('session.gc_maxlifetime');   // "3600"
$all = ini_get_all('session');              // details array of every session.* key
```

Directives cover `name`, `save_path`, `save_handler`, `cache_limiter`,
`cache_expire`, the `cookie_*` parameters (`cookie_partitioned` on PHP 8.5),
`use_cookies`, `use_strict_mode`, `use_only_cookies`, `lazy_write`,
`serialize_handler`, `gc_probability`,
`gc_divisor`, `gc_maxlifetime`, `sid_length`, `sid_bits_per_character`, plus:

- **`session.auto_start`** — the `php.ini`-`PERDIR` analog. Because a compiled
  binary has no `php.ini`, it is seeded once per worker process from the
  `ELEPHC_SESSION_AUTO_START` environment variable (`1`/`on`/`true` → on). When
  on, the request bootstrap calls `session_start()` automatically before your
  handler body runs. Matching PHP's `PERDIR` semantics,
  `ini_set('session.auto_start', …)` returns `false`; the same applies to the
  `session.upload_progress.*` PERDIR directives.
- **`session.referer_check`** — matching php-src, this legacy check is evaluated
  only when `session.use_only_cookies=0`. In that mode, a supplied session ID
  (including one found in a cookie) is discarded when a non-empty request
  `Referer` does not contain the configured substring. With PHP's default
  `use_only_cookies=1` it is inert, so it must not be treated as a replacement
  for strict mode or modern cookie controls. PHP 8.4 deprecates the directive.

## Upload progress

Under `--web`, elephc implements PHP's `session.upload_progress` as real
streaming progress. When a `multipart/form-data` upload arrives with a session
ID supplied by a cookie, query parameter, or earlier multipart field, the
server reads the body frame-by-frame and, before
your handler runs, writes a progress array into the session file so a concurrent
poll request can observe how far the upload has advanced.

Tracking activates when `session.upload_progress.enabled` is on, the request is
a multipart upload, and it supplies a valid session ID. Non-cookie IDs are
accepted when `session.use_only_cookies` is off. The progress key is
taken from a form field named `session.upload_progress.name` (default
`PHP_SESSION_UPLOAD_PROGRESS`), which must appear **before** the file fields, as
in PHP. The value is stored under `$_SESSION[session.upload_progress.prefix . $value]`
(default prefix `upload_progress_`):

```php
$_SESSION["upload_progress_" . $value] = [
  "start_time"      => 1234567890,   // request time
  "content_length"  => 57343257,     // request Content-Length
  "bytes_processed" => 453489,       // POST bytes received so far
  "done"            => false,        // true once the body is fully received
  "files"           => [
    0 => [
      "field_name"      => "file1",
      "name"            => "foo.avi",
      "tmp_name"        => "",
      "error"           => 0,
      "done"            => false,
      "start_time"      => 1234567890,
      "bytes_processed" => 68767,
    ],
  ],
];
```

Writes are throttled by `session.upload_progress.freq` (a byte count, or a
percentage of `content_length` such as `"1%"`) and `session.upload_progress.min_freq`
(minimum seconds between writes), and use short independent file-lock cycles so
the lock is never held across the whole upload. When the body is fully received,
every file and the whole entry are marked `done => true`. If
`session.upload_progress.cleanup` is on (the default), the entry is then removed,
so the upload request's own handler never sees it — matching PHP, where only a
concurrent poll request observes the intermediate states.

Because elephc has no `php.ini`, the `enabled` and `cleanup` directives can be
seeded per worker process from the `ELEPHC_SESSION_UPLOAD_PROGRESS_ENABLED` /
`ELEPHC_SESSION_UPLOAD_PROGRESS_CLEANUP` environment variables (mirroring
`session.auto_start`); otherwise they use the PHP defaults. Configuration needed
before the body drain can also be supplied through `ELEPHC_SESSION_NAME`,
`ELEPHC_SESSION_SAVE_PATH`, `ELEPHC_SESSION_SERIALIZE_HANDLER`, and
`ELEPHC_SESSION_USE_ONLY_COOKIES`. These deployment values seed both the
pre-handler upload tracker and the request's later `session_start()`, so the
cookie name, file path, serializer, and transport policy cannot diverge between
the two phases.

Notes and limitations specific to elephc:

- Progress writes support `php`, `php_serialize`, and `php_binary` session
  serialization.
- `tmp_name` is always the empty string: uploads are buffered in memory rather
  than spooled to a temporary file, so there is no temp path to report.
- The prefork server buffers a request's body before its own handler runs, so an
  upload request cannot observe its **own** intermediate progress; a separate
  concurrent request must poll the session to read live progress.

## URL rewriting (`session.use_trans_sid`)

When cookies are unavailable, PHP can propagate the session id through the page
itself by appending it to same-origin URLs and injecting a hidden field into
forms. elephc implements this output rewriting under `--web`.

It is **off by default** and only activates when **all** of the following hold:

- `session.use_trans_sid` is `1`, **and**
- `session.use_only_cookies` is `0` (its default `1` disables URL propagation), **and**
- a session is active (`session_start()` ran) with a non-empty id, **and**
- the request did **not** resume through an accepted session cookie (a Cookie
  header is ignored when `session.use_cookies=0`).

```php
<?php
session_start(['use_trans_sid' => 1, 'use_only_cookies' => 0]);
echo '<a href="/next">next</a><form action="/post"></form>';
// cookie-less response body becomes:
// <a href="/next?PHPSESSID=<id>">next</a>
// <form action="/post"><input type="hidden" name="PHPSESSID" value="<id>" /></form>
```

Behavior:

- Only `text/html` responses (or responses with no `Content-Type`) have their
  body rewritten; other content types are never touched.
- The tags/attributes rewritten come from `session.trans_sid_tags` (default
  `"a=href,area=href,frame=src,form="`). An entry with an empty attribute
  (`form=`) injects the hidden SID field instead of rewriting an attribute.
- Only **same-origin** URLs are rewritten: relative URLs, protocol-relative URLs
  matching the request host, and absolute URLs whose host is the request host or
  is listed in `session.trans_sid_hosts` (comma-separated). Off-host URLs are
  never rewritten, so the id cannot leak to third-party hosts.
- `mailto:`, `javascript:`, fragment-only (`#…`) links, and URLs that already
  carry the session name are left untouched. The SID is inserted before any
  `#fragment`, using `?` when the URL has no query or `&` otherwise.
- A same-origin `Location:` redirect header is rewritten the same way.
- The `SID` constant intentionally stays empty in elephc; with automatic URL
  rewriting there is no need to append `SID` manually.

## Examples

Three runnable `--web` examples live under `examples/`:

- **`examples/web-session`** — a per-visitor counter that also demonstrates the
  `ini_set()` / `ini_get()` runtime configuration layer.
- **`examples/web-session-upload`** — streaming `session.upload_progress`: an
  upload form whose progress bar is driven by a concurrent `/progress` poll
  reading the live entry the server writes into the session file.
- **`examples/web-session-trans-sid`** — cookieless `session.use_trans_sid` URL
  rewriting: same-origin links, forms, and a `Location:` redirect gain the
  session id while off-host and `mailto:` URLs are left untouched.

Each compiles with `cargo run -- --web examples/<name>/main.php` and runs as a
standalone server binary.

## Limitations

- **`--web` only** — sessions require the HTTP request lifecycle. A CLI-compiled
  binary has no session context.
- **Warning surface** — session misuse now emits the real PHP `E_WARNING`/`E_NOTICE`
  text (e.g. `session_start(): Ignoring session_start() because a session is already
  active`, `session_id(): Session ID cannot be changed when a session is active`) to
  the worker's stderr via `trigger_error()`. `error_log()` and `trigger_error()` are
  available under `--web`: `trigger_error()` renders `"<Prefix>: <message>"` to stderr,
  and `error_log()` supports the stderr channel (`message_type` 0) and file appends
  (`message_type` 3); the mail channel (`message_type` 1) is unsupported and returns
  `false`. There is no `error_reporting`/`display_errors` layer, so these messages are
  always written. PHP's "headers already sent" warning genuinely cannot occur because
  output is buffered until the request completes.
