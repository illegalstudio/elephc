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
| `session_id()` | `session_id(?string $id = null): string\|false` | Get or set the current session ID. With no argument, returns the current ID (or `false` if no session is active). With a string, sets the ID before `session_start()`. |
| `session_name()` | `session_name(?string $name = null): string\|false` | Get or set the session name (used as the cookie name). Default is `PHPSESSID`. Must be called before `session_start()`. |
| `session_status()` | `session_status(): int` | Returns `PHP_SESSION_NONE` or `PHP_SESSION_ACTIVE`. |
| `session_save_path()` | `session_save_path(?string $path = null): string\|false` | Get or set the directory where session files are stored. Defaults to `sys_get_temp_dir()`. |
| `session_write_close()` | `session_write_close(): bool` | Write session data and end the session. Called automatically at handler end. |
| `session_regenerate_id()` | `session_regenerate_id(bool $delete_old = false): bool` | Generate a new session ID, optionally deleting the old session file. |
| `session_unset()` | `session_unset(): bool` | Unset all `$_SESSION` variables (the array stays, values are removed). |
| `session_destroy()` | `session_destroy(): bool` | Destroy the session. Does not unset `$_SESSION` — call `session_unset()` first if needed. |
| `session_set_cookie_params()` | `session_set_cookie_params(array $options): bool` | Set session cookie parameters (lifetime, path, domain, secure, httponly, samesite). |
| `session_get_cookie_params()` | `session_get_cookie_params(): array` | Returns the current session cookie parameters as an associative array. |
| `session_start()` options | | The `$options` array accepts `cookie_lifetime`, `cookie_path`, `cookie_domain`, `cookie_secure`, `cookie_httponly`, `cookie_samesite`, and `read_and_close`. |

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
    'cookie_httponly' => true,
    'cookie_samesite' => 'Lax',
]);
```

## File storage

Session data is stored in files using PHP's default `session.save_handler = files`
model. Each session is a single file named `sess_<id>` containing the serialized
`$_SESSION` array.

By default, session files are written to `sys_get_temp_dir()`. Change the
directory with `session_save_path()`:

```php
<?php
session_save_path('/var/www/sessions');
session_start();
```

The directory must exist and be writable by the server process.

File locking (`flock`) is used during `session_start()` and
`session_write_close()` to prevent concurrent requests on the same session ID
from corrupting each other's data.

## Limitations

- **File handler only** — `session_set_save_handler()` is not supported. Custom
  save handlers (database, Redis, etc.) cannot be registered.
- **`--web` only** — sessions require the HTTP request lifecycle. A CLI-compiled
  binary has no session context.
- **`session.use_strict_mode`** — strict mode (rejecting uninitialized session
  IDs) is not enforced; `session_start()` always loads or creates a session for
  the provided ID.
- **`session_cache_limiter()`** — not provided; manage cache headers manually
  with `header()`.
