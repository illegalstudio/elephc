---
title: "cURL"
description: "PHP's ext/curl on a statically pinned libcurl 8.21.0: easy, multi and share interfaces, uploads, callbacks, the protocol matrix, and every documented difference from PHP."
sidebar:
  order: 18
---

elephc implements PHP's `ext/curl` — all 35 functions, all 6 classes, and all 689
constants — on top of a **statically pinned libcurl 8.21.0** built by elephc's own
managed-native-package system. There is no dependency on a system, Homebrew, or
distro libcurl: the archive is compiled from a checksum-pinned tarball, linked
into your binary, and travels with it.

The pinned stack is:

| Library | Version | Role |
|---|---|---|
| libcurl | 8.21.0 | The transfer engine behind every `curl_*` function |
| OpenSSL | 3.5.7 (LTS) | libcurl's TLS backend — **and nothing else** (see below) |
| zlib | 1.3.2 | `Accept-Encoding` / `CURLOPT_ACCEPT_ENCODING` decompression |

Everything is pay-for-use. A program that never mentions a `curl_*` function, a
`Curl*`/`CURL*` class, or a `CURLOPT_*` / `CURLINFO_*` / `CURLE_*` / `CURL_*`
constant does not declare the classes, does not link the bridge, and does not
require the native `curl` package at all.

The inverse holds just as literally: merely *referencing* one of those constant
prefixes is enough to opt in, even with no other curl usage anywhere in the
program. `<?php echo CURLOPT_URL;` alone injects the curl prelude, links the
bridge, and requires the native `curl` package — reading the constant is all
detection needs to see, whether or not the value is ever passed to
`curl_setopt()`. Detection matches the prefix, not the specific name, and it is
triggered by a *reference* to the constant (`CURLOPT_URL` used as a value), not
by a string that merely spells the name — a program that `define()`s its
**own** `CURL_`-prefixed constant and never reads it back stays curl-free, but
the moment that constant is *read* anywhere (`echo CURL_MY_TIMEOUT;`,
`CURL_MY_TIMEOUT + 1`, …) detection fires identically, even though the
constant has nothing to do with `ext/curl`. If that surprises you and you
don't actually want curl, rename the constant; there is no way to opt out
while keeping a `CURL_`-prefixed name that the program ever reads.

## Enabling curl

curl needs a native package, so it is opt-in at the project level even though
usage detection is automatic. Declare it once:

```bash
elephc native add curl
```

`curl` is the first catalog package with dependencies of its own: adding it also
declares `openssl` and `zlib`. See
[Native dependencies](../compiling/native-dependencies.md) for the full `elephc
native` workflow (lock file, cache, `elephc native build`).

After that, an ordinary compile is enough — naming any part of the curl surface
links the bridge:

```bash
elephc app.php
```

To force-link the whole `elephc_curl` archive and force-inject the PHP prelude
even when the compiler sees no curl usage (for example when curl only ever
appears inside an `eval()` string), pass the bridge flag:

```bash
elephc app.php --with-curl
```

`extension_loaded('curl')` answers `true` exactly when the bridge is linked, and
`false` for a curl-free program. AOT code and `eval()` inside the same binary can
never disagree about it.

## Protocol matrix

The pinned libcurl is configured for a deliberately small protocol set. What is
built in:

| Scheme | Status |
|---|---|
| `file://` | Supported |
| `http://` | Supported |
| `https://` | Supported (see [TLS](#tls-and-the-openssl-split)) |
| `ftp://` | Supported |
| `ftps://` | Supported |
| `ws://`, `wss://` | Compiled in by libcurl's default, but PHP exposes no WebSocket API — see below |

`curl_version()['protocols']` on this build reports exactly
`file, ftp, ftps, http, https, ws, wss`.

Everything else is **disabled at build time and fails loudly** — never silently,
and never with a fabricated success. A URL in a disabled scheme fails at
`curl_exec()` / `curl_multi_exec()` with `CURLE_UNSUPPORTED_PROTOCOL` (`1`):

```php
$ch = curl_init("sftp://example.invalid/x");
curl_exec($ch);
echo curl_errno($ch);   // 1  (CURLE_UNSUPPORTED_PROTOCOL)
echo curl_error($ch);   // Protocol "sftp" is disabled
```

The disabled set is `dict`, `gopher`, `gophers`, `imap`, `imaps`, `ldap`,
`ldaps`, `mqtt`, `pop3`, `pop3s`, `rtsp`, `scp`, `sftp`, `smb`, `smbs`, `smtp`,
`smtps`, `telnet` and `tftp` — all reporting `Protocol "…" is disabled`.

`rtmp`, `rtmps` and `rtmpe` report `Protocol "…" not supported` instead: librtmp
was never linked, so libcurl does not know those schemes at all rather than
knowing them and refusing. A scheme that is not a cURL scheme in the first place
(`xyzzy://`) gets the same message. All of these are errno `1`.

Also not built in, and therefore not available even over supported schemes:

- **HTTP/2 and HTTP/3.** The build has no nghttp2 and no QUIC library, so
  transfers are HTTP/1.1. This is visible at `curl_setopt()` rather than
  silently: `CURLOPT_HTTP_VERSION` accepts `CURL_HTTP_VERSION_NONE`,
  `CURL_HTTP_VERSION_1_0` and `CURL_HTTP_VERSION_1_1`, and returns `false` for
  `CURL_HTTP_VERSION_2_0`, `CURL_HTTP_VERSION_2TLS` and `CURL_HTTP_VERSION_3`.
- **Brotli and zstd** content encodings. gzip and deflate work through zlib.
- **libssh2**, so SCP/SFTP cannot be re-enabled by an option.
- **libpsl** (public-suffix cookie checks) and **libidn2** (IDN host names).

`ws://` and `wss://` deserve a note: libcurl 8.21 builds its WebSocket support
by default, so the schemes appear in `curl_version()['protocols']`, but PHP's
`ext/curl` has never exposed `curl_ws_send()` / `curl_ws_recv()` and neither does
elephc. A `ws://` URL therefore performs the HTTP upgrade handshake through
`curl_exec()` and then has no API to drive the resulting connection. Treat
WebSockets as unsupported.

## TLS and the OpenSSL split

**The managed OpenSSL is libcurl's TLS backend and nothing else.** This is worth
being explicit about, because elephc links an OpenSSL archive into curl-using
binaries but does not route any other PHP surface through it:

| PHP surface | Backed by |
|---|---|
| `https://` through `curl_*` | Managed OpenSSL 3.5.7, as libcurl's SSL backend |
| `openssl_encrypt()` / `openssl_decrypt()` / the rest of `openssl_*` | RustCrypto (`elephc-crypto`), unchanged |
| `hash()`, `hash_hmac()`, `md5()`, `sha1()` | RustCrypto (`elephc-crypto`), unchanged |
| `https://` through `file_get_contents()` / `fopen()` stream wrappers | `elephc-tls` (rustls), unchanged |

Adding curl to a project therefore does not change the behavior, output, or
constant-time properties of any existing hashing or encryption call. The two
stacks never meet.

`curl_version()` reports the TLS backend honestly:

```php
$v = curl_version();
echo $v["version"];       // 8.21.0
echo $v["ssl_version"];   // OpenSSL/3.5.7
echo $v["libz_version"];  // 1.3.2
```

Note that these are **elephc's** pinned versions. They will differ from
`php -r "print_r(curl_version());"` on the same machine, which reports whatever
libcurl the host PHP was built against.

### The CA trust store caveat

**This is the one thing to check before shipping an HTTPS client.** The managed
curl recipe does not bundle a CA certificate store and does not pass
`--with-ca-bundle` / `--with-ca-path`. libcurl's `configure` therefore autodetects
a CA path on the **build machine** and bakes that absolute path into the archive.
On a macOS build host, for instance:

```php
$info = curl_getinfo(curl_init());
echo $info["cainfo"];   // /etc/ssl/cert.pem   — a path from the BUILD machine
```

That path travels into every binary built from that cache. If the machine
*running* the binary has no file there, HTTPS certificate verification fails with
`CURLE_SSL_CACERT_BADFILE` (`77`) even though the transfer itself is fine. Two
ways to make this deterministic:

```php
// 1. Point at a bundle you ship with the application.
curl_setopt($ch, CURLOPT_CAINFO, __DIR__ . "/ca-bundle.pem");

// 2. Or a directory of hashed certificates.
curl_setopt($ch, CURLOPT_CAPATH, "/etc/ssl/certs");
```

Both options reach libcurl directly and are honored. Disabling verification with
`CURLOPT_SSL_VERIFYPEER => false` also "works" and is, as always, a bad idea.

A bundled, hermetic CA story is a follow-up; until it lands, treat
`CURLOPT_CAINFO` as required configuration for any HTTPS client that has to run
on a machine other than the one that built it.

## The function and class surface

Every function PHP 8.5 exposes is implemented:

| Group | Functions |
|---|---|
| Easy | `curl_init`, `curl_setopt`, `curl_setopt_array`, `curl_exec`, `curl_close`, `curl_copy_handle`, `curl_errno`, `curl_error`, `curl_escape`, `curl_unescape`, `curl_getinfo`, `curl_pause`, `curl_reset`, `curl_upkeep`, `curl_version`, `curl_strerror` |
| Multi | `curl_multi_init`, `curl_multi_add_handle`, `curl_multi_remove_handle`, `curl_multi_exec`, `curl_multi_select`, `curl_multi_info_read`, `curl_multi_close`, `curl_multi_getcontent`, `curl_multi_setopt`, `curl_multi_strerror`, `curl_multi_errno`, `curl_multi_get_handles` |
| Share | `curl_share_init`, `curl_share_setopt`, `curl_share_close`, `curl_share_errno`, `curl_share_strerror`, `curl_share_init_persistent` |
| Files | `curl_file_create` |

And every class:

| Class | Notes |
|---|---|
| `CurlHandle` | `final`, minted only by `curl_init()` / `curl_copy_handle()` |
| `CurlMultiHandle` | `final`, minted only by `curl_multi_init()` |
| `CurlShareHandle` | `final`, minted only by `curl_share_init()` |
| `CurlSharePersistentHandle` | `final`, PHP 8.5 only |
| `CURLFile` | User-constructible; `name`, `mime`, `postname` plus the five getters/setters |
| `CURLStringFile` | User-constructible; `data`, `postname`, `mime` |

### PHP version profile

elephc targets PHP 8.5 by default. `curl_multi_get_handles()`,
`curl_share_init_persistent()` and `CurlSharePersistentHandle` are 8.5 additions,
so compiling with `--php-version 8.4` (or 8.3/8.2) leaves them **undeclared**,
and calling one fails as an undefined function — exactly as it would on that
runtime. Everything else is identical across the four profiles.

**Constants are not version-fenced.** Only functions and classes are. Every one of
the 689 `CURLOPT_*` / `CURLINFO_*` / `CURLE_*` / `CURL_*` names is declared at every
target version, the same way `JSON_*` is — including the PHP 8.5 additions
(`CURLINFO_CONN_ID`, `CURLINFO_QUEUE_TIME_T`, `CURLINFO_USED_PROXY`,
`CURLINFO_HTTPAUTH_USED`, `CURLINFO_PROXYAUTH_USED`,
`CURLOPT_SSL_SIGNATURE_ALGORITHMS`, `CURLFOLLOW_*`, `CURLOPT_INFILESIZE_LARGE`).
Their values are frozen from the pinned libcurl 8.21.0 headers, which understands
all of them at every profile, so `--php-version 8.4` still resolves them.

`curl_share_init_persistent()` deserves one note: the native share it creates is
**process-lifetime**. elephc has no PHP-FPM-worker-restart boundary to key a
shorter lifetime off, so the share is never freed.

## Options

`curl_setopt()` classifies every option number against a frozen table generated
from the pinned libcurl headers, then reads the PHP value according to that
option's real C type. **260 of PHP's 271 `CURLOPT_*` names are implemented.**

An option this build cannot carry returns `false` and emits PHP's warning — never
an inert `true`:

```php
var_dump(curl_setopt($ch, CURLOPT_SSLCERT_BLOB, $pem));
// Warning: curl_setopt(): Option 40291 is not supported by this build
// bool(false)
```

An option number that is not a cURL option at all raises php-src's own error:

```php
curl_setopt($ch, 987654, 1);
// ValueError: curl_setopt(): Argument #2 ($option) is not a valid cURL option
```

### The 11 rejected options

This list is pinned by a test
(`elephc_curl::tests::option_table::the_documented_rejection_set_is_exactly_this`),
so it cannot drift from the code:

| Options | Why |
|---|---|
| `CURLOPT_CAINFO_BLOB`, `CURLOPT_ISSUERCERT_BLOB`, `CURLOPT_PROXY_CAINFO_BLOB`, `CURLOPT_PROXY_ISSUERCERT_BLOB`, `CURLOPT_PROXY_SSLCERT_BLOB`, `CURLOPT_PROXY_SSLKEY_BLOB`, `CURLOPT_SSLCERT_BLOB`, `CURLOPT_SSLKEY_BLOB` | In-memory certificate/key blobs. The **file-path** forms (`CURLOPT_SSLCERT`, `CURLOPT_CAINFO`, …) all work — write the material to a file. |
| `CURLOPT_FNMATCH_FUNCTION`, `CURLOPT_PREREQFUNCTION`, `CURLOPT_SSH_HOSTKEYFUNCTION` | Callbacks outside the six implemented ones (below). `SSH_HOSTKEYFUNCTION` is moot anyway — SSH is not built in. |

`CURLINFO_HEADER_OUT` — which is a `curl_setopt()` option despite its name — is
rejected the same way, so `curl_getinfo($ch, CURLINFO_HEADER_OUT)` has nothing to
report.

`curl_multi_setopt()` implements 8 of PHP's 9 `CURLMOPT_*` options;
`CURLMOPT_PUSHFUNCTION` is rejected (it is an HTTP/2 server-push hook, and HTTP/2
is not built in). `curl_share_setopt()` implements `CURLSHOPT_SHARE` and
`CURLSHOPT_UNSHARE` over the five `CURL_LOCK_DATA_*` values PHP exposes.

### PHP-layer options

These are implemented by elephc's PHP layer rather than forwarded to libcurl,
matching php-src:

| Option | Behavior |
|---|---|
| `CURLOPT_RETURNTRANSFER` | Capture the body; `curl_exec()` returns a `string` |
| `CURLOPT_HEADER` | Prepend response headers to the body |
| `CURLOPT_BINARYTRANSFER` | No-op, as in modern PHP |
| `CURLOPT_SAFE_UPLOAD` | Always on; `@file` strings in `CURLOPT_POSTFIELDS` stay literal. Disabling it raises `ValueError`, as in PHP 8. |
| `CURLOPT_PRIVATE` | Stores an arbitrary PHP value, read back verbatim by `curl_getinfo($ch, CURLINFO_PRIVATE)` |

With no `CURLOPT_RETURNTRANSFER`, the body is written to stdout and `curl_exec()`
returns `true` — the PHP CLI behavior.

> **Divergence:** that stdout write goes straight to file descriptor 1, so
> `ob_start()` does **not** capture it the way php's does. Wrap the transfer in
> `CURLOPT_RETURNTRANSFER` (or a `CURLOPT_WRITEFUNCTION`) if you need the body as a
> string.

### Stream options

`CURLOPT_FILE`, `CURLOPT_WRITEHEADER`, `CURLOPT_INFILE` (a.k.a. `CURLOPT_READDATA`)
and `CURLOPT_STDERR` take a stream — whatever `fopen()` returned — and work as they
do in php:

```php
$sink = fopen('/tmp/body.txt', 'wb');
$ch = curl_init('https://example.com/');
curl_setopt($ch, CURLOPT_FILE, $sink);
curl_exec($ch);              // true; the body is in /tmp/body.txt
```

| Option | Behavior |
|---|---|
| `CURLOPT_FILE` | Response **body** is written to the stream; `curl_exec()` returns `true` |
| `CURLOPT_WRITEHEADER` | Response **headers** are written to the stream |
| `CURLOPT_INFILE` | Upload **source** for `CURLOPT_UPLOAD`/`CURLOPT_PUT`; pair it with `CURLOPT_INFILESIZE` |
| `CURLOPT_STDERR` | libcurl's **verbose trace**, in libcurl's own format (`* `, `> `, `< ` prefixes). Requires `CURLOPT_VERBOSE`. |

Three precedence rules, all matching php:

- **The body has one sink.** `CURLOPT_FILE`, `CURLOPT_RETURNTRANSFER` and
  `CURLOPT_WRITEFUNCTION` select the same mode, and **the last one set wins**.
  Setting any of them to `null`/`false` falls back to stdout, *not* to a sibling
  set earlier. `CURLOPT_WRITEHEADER`/`CURLOPT_HEADERFUNCTION` pair up the same
  way, except their default is to discard headers rather than print them.
- **A read callback outranks `CURLOPT_INFILE`, in either order.** Setting
  `CURLOPT_INFILE` after a `CURLOPT_READFUNCTION` does *not* displace the
  callback; clearing the callback with `null` falls back to the stream. A read
  callback also receives the `CURLOPT_INFILE` stream as its `$fd` argument, so
  `fread($fd, $length)` inside it works.
- **`CURLOPT_STDERR` is a fallback, not a mode.** A `CURLOPT_DEBUGFUNCTION` always
  wins over it, whichever order they are set in — and once `CURLOPT_DEBUGFUNCTION`
  has been touched *at all*, even with `null`, `CURLOPT_STDERR` stays shadowed for
  the rest of that handle's life.

`curl_reset()` clears all four; `curl_copy_handle()` carries them onto the copy,
which then reads from and writes to the same streams. Keep the stream open for as
long as the handle uses it.

A non-stream value raises `TypeError`, and a read-only stream given to
`CURLOPT_FILE`/`CURLOPT_WRITEHEADER`/`CURLOPT_STDERR` raises `ValueError`, both
with php's messages. `null` is accepted and clears the option.

> **Divergence:** php answers a distinct `TypeError` for a stream that has already
> been `fclose()`d ("supplied **resource** is not a valid File-Handle resource").
> elephc's `is_resource()` still reports `true` for a closed stream, so it cannot
> tell that case apart and reports the ordinary "supplied **argument**" message.

## `curl_getinfo()`

Called without an option, `curl_getinfo()` returns PHP's associative array. This
build reports all 41 keys PHP 8.4 does, in PHP's own key order — `url`,
`http_code`, `content_type`, `total_time`, `redirect_count`, `primary_ip`,
`scheme`, `protocol`, `http_version` and the microsecond timers, ending with
`effective_method`, `capath`, `cainfo`. Keys for features that are not compiled in
are omitted rather than faked.

`http_connectcode`, `num_connects` and `appconnect_time` are **not** in that array
— php does not put them there either. Read them through the option form:
`curl_getinfo($ch, CURLINFO_HTTP_CONNECTCODE)`, `CURLINFO_NUM_CONNECTS`,
`CURLINFO_APPCONNECT_TIME`.

Called with a `CURLINFO_*` option, the read dispatches on the option's type mask
(string / long / double / slist / off_t), exactly as php-src does. Three options
are special-cased before the mask: `CURLINFO_PRIVATE` (the PHP value you stored),
`CURLINFO_CERTINFO`, and `CURLINFO_HEADER_OUT`. Anything else answers `false` —
never a fabricated value.

## The multi interface

The full multi interface works, including the canonical drive loop:

```php
$mh = curl_multi_init();
foreach ($urls as $url) {
    $ch = curl_init($url);
    curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
    curl_multi_add_handle($mh, $ch);
}

$running = 0;
do {
    $status = curl_multi_exec($mh, $running);
    if ($running > 0) {
        curl_multi_select($mh, 1.0);
    }
} while ($running > 0 && $status === CURLM_OK);

// Note the loop shape: assign first, then test. php.net's
// `while ($info = curl_multi_info_read($mh))` works, but an assignment used as a
// loop condition leaks the assigned value on every iteration in this compiler —
// a pre-existing defect with nothing to do with curl. See "Memory and lifetime".
while (true) {
    $info = curl_multi_info_read($mh);
    if ($info === false) {
        break;
    }
    echo curl_multi_getcontent($info["handle"]);
}
```

`curl_multi_get_handles()` (PHP 8.5) returns the attached handles in attachment
order.

## Uploads: `CURLFile` and `CURLStringFile`

`CURLOPT_POSTFIELDS` with an array posts a real `multipart/form-data` body built
through libcurl's MIME API:

```php
curl_setopt($ch, CURLOPT_POSTFIELDS, [
    "field"  => "value",
    "upload" => new CURLFile("/path/to/photo.png", "image/png", "photo.png"),
    "inline" => new CURLStringFile("raw bytes", "data.bin", "application/octet-stream"),
    "tags"   => ["a", "b"],           // flattens to two parts both named "tags"
]);
```

An **empty** array is php-src's own special case and posts an empty
`application/x-www-form-urlencoded` body — exactly what
`CURLOPT_POSTFIELDS => ""` posts — never an empty multipart with a boundary.

A string `CURLOPT_POSTFIELDS` still posts that string verbatim, unchanged.
Nested-array flattening matches php-src exactly: one part per inner element, all
sharing the outer key. When no `postname` is given, `CURLFile` sends the full
local path as the filename — this is php-src's actual behavior, measured on the
wire, not `basename()`.

## Callbacks

Six libcurl callbacks invoke real PHP callables (closures, `'function_name'`
strings, `[$obj, 'method']` arrays, first-class callables):

| Option | Signature |
|---|---|
| `CURLOPT_WRITEFUNCTION` | `fn(CurlHandle $ch, string $data): int` |
| `CURLOPT_HEADERFUNCTION` | `fn(CurlHandle $ch, string $header): int` |
| `CURLOPT_READFUNCTION` | `fn(CurlHandle $ch, $fd, int $length): string` — `$fd` is the `CURLOPT_INFILE` stream, or `null` when none is set |
| `CURLOPT_PROGRESSFUNCTION` | `fn(CurlHandle $ch, int $dlTotal, int $dlNow, int $ulTotal, int $ulNow): int` |
| `CURLOPT_XFERINFOFUNCTION` | same as `CURLOPT_PROGRESSFUNCTION` |
| `CURLOPT_DEBUGFUNCTION` | `fn(CurlHandle $ch, int $type, string $data): int` |

An exception thrown inside a callback propagates out of `curl_exec()` /
`curl_multi_exec()` and can be caught normally; the transfer is aborted and the
handle stays reusable afterwards.

`CURLOPT_FNMATCH_FUNCTION`, `CURLOPT_PREREQFUNCTION`,
`CURLOPT_SSH_HOSTKEYFUNCTION` and `CURLMOPT_PUSHFUNCTION` are rejected with the
standard warning.

## curl inside `eval()`

`eval()` reaches the same pinned libcurl through the same C ABI, but the
interpreter ships a **narrower** surface than compiled code:

**Available in `eval()`:** the complete easy interface (16 functions) and all 689
constants, with `curl_setopt()`'s full table-driven option dispatch — every
LONG / STRING / SLIST / OFF_T / PHP-layer option works, not a hand-picked subset.

**Not available in `eval()`:**

- The multi interface (`curl_multi_*`) and the share interface (`curl_share_*`).
- `CURLFile` / `CURLStringFile` / `curl_file_create()`, and with them the array
  (`multipart/form-data`) form of `CURLOPT_POSTFIELDS`. A **string**
  `CURLOPT_POSTFIELDS` body works.
- Callback options, `CURLOPT_SHARE`, and the four **stream** options
  (`CURLOPT_FILE`, `CURLOPT_WRITEHEADER`, `CURLOPT_INFILE`/`CURLOPT_READDATA`,
  `CURLOPT_STDERR`) — rejected with the same `false` + warning path a genuinely
  unsupported option uses. Compiled code implements all four; `eval()` does not.

Further differences inside `eval()`:

- **A handle is not a `CurlHandle` object.** It is a resource-like cell, so
  `gettype($ch)` reports `"resource"` and `$ch instanceof CurlHandle` is false.
  This mirrors `hash_init()`'s long-standing `HashContext` behavior in eval.
  Every `curl_*` function still works on it.
- **Handles cannot cross the eval boundary.** An eval-created handle passed out
  to compiled code is an opaque cell with no `CurlHandle` class instance behind
  it, and an AOT `CurlHandle` passed into an `eval()` string is not accepted by
  eval's curl functions.
- **An invalid option number is a fatal, not a `ValueError`.** The
  [Options](#options) section above promises a catchable `ValueError` for an
  option number that is not a cURL option; inside `eval()` the same call is a
  **non-catchable runtime fatal** that ends the program. The interpreter's
  internals have no path for raising a catchable PHP exception, so this is a hard
  fault — the same tradeoff `hash_final()` on an already-finalized context makes.
  `try`/`catch` around the `eval()` will not save you.
- **A non-array value for a `CURLOPT_*` string-list option returns `false`
  silently.** Compiled code raises `TypeError: curl_setopt(): Argument #3
  ($value) must be of type array, … given`; eval returns `false` with no warning
  and no exception. Check the return value of `curl_setopt()` inside `eval()`.
- **`--with-curl` is required** when curl appears *only* inside an `eval()`
  string, because usage detection reads the compiled source, not the string.

## Differences from PHP

Nothing here is a silent difference — every one of these either matches php-src's
observable behavior or fails loudly. They are collected in one place rather than
scattered through the page above.

### Signatures that diverge

Several functions declare a narrower return type than php-src, because elephc's
checker does not accept a `T|false` union where a `T` is expected and does not
narrow one through a `=== false` guard. In every case the runtime behavior on the
success path is PHP's:

| Function | php-src | elephc | Effect |
|---|---|---|---|
| `curl_init()` | `CurlHandle\|false` | `CurlHandle` | **Throws** instead of returning `false` on allocation failure |
| `curl_copy_handle()` | `CurlHandle\|false` | `CurlHandle` | Throws instead of returning `false` |
| `curl_escape()`, `curl_unescape()` | `string\|false` | `string` | Throw instead of returning `false` |
| `curl_version()` | `array\|false` | *(undeclared)* | Returns the array; the type is left undeclared rather than wrong |
| `curl_multi_info_read()` | `array\|false` | *(undeclared)* | Returns the array or `false` as PHP does; only the declared type differs |
| `curl_strerror()`, `curl_multi_strerror()`, `curl_share_strerror()` | `?string` | `string` | Never null in practice |

`curl_multi_init()`, `curl_share_init()` and `curl_share_init_persistent()`
declare the same non-union return types php-src does; where the underlying
allocation fails they throw rather than returning `false`, which php-src's
signatures do not allow either.

Three multi functions take `mixed $handle` instead of `CurlHandle $handle`, with
a runtime `instanceof` guard that raises php-src's own `TypeError`:
`curl_multi_add_handle()`, `curl_multi_remove_handle()`,
`curl_multi_getcontent()`. This is deliberate and load-bearing: a handle read out
of `curl_multi_info_read()`'s array or `curl_multi_get_handles()`'s list arrives
as a `mixed`, and passing a `mixed`-sourced object to a *typed* object parameter
is a known miscompile (documented in
[PHP compatibility](compatibility.md)). Without the `mixed` parameter, the
canonical multi loop would either be a compile error or silently wrong.

### Errors raised where PHP is silent

elephc raises a catchable `TypeError` in three places where php-src accepts the
value or returns `true`:

- `curl_setopt($ch, CURLOPT_SHARE, $notAShare)` — php-src silently returns `true`.
- `curl_share_setopt()` with an array, `null`, or an object value — php-src
  returns `false`.
- A non-`CURLFile` / non-`CURLStringFile` object inside a `CURLOPT_POSTFIELDS`
  array — php-src casts it to a string. This one is a **safety** requirement, not
  a style preference: elephc's object-to-string cast on an object without
  `__toString()` is an uncatchable process exit, so relying on the cast would let
  a bad argument kill the process instead of raising something you can catch.

Two array shapes inside `CURLOPT_POSTFIELDS` also raise `TypeError` instead of
reproducing php-src's recursion limit: a **doubly**-nested value (an inner element
that is itself an array), and an object inside a nested array.

### Timing differences

- **A missing upload file fails earlier.** `curl_setopt($ch, CURLOPT_POSTFIELDS,
  [... new CURLFile('/nope')])` fails at `curl_setopt()` time in elephc (libcurl's
  MIME builder stats the file eagerly); php-src fails later, at `curl_exec()`.
  The failure is reported either way.
- **Callback errno on the multi path.** When a callback throws during
  `curl_multi_exec()`, the exception propagates and `curl_errno($ch)` reports `0`
  (matching php-src's easy-interface behavior), but that transfer's
  `curl_multi_info_read()` entry reports `result === 23` (`CURLE_WRITE_ERROR`).
  php-src's global exception gate makes both report the same code.

### Memory and lifetime

- **A callback that captures its own handle leaks it.** `curl_setopt($ch,
  CURLOPT_WRITEFUNCTION, function () use ($ch) { ... })` closes a reference cycle.
  elephc's memory model is refcount-only with no cycle collector, so the handle —
  and its libcurl session, socket, and DNS cache — lives until the process exits.
  php-src has the identical cycle and survives it only because Zend has a cycle
  collector. Pass the handle the callback already receives as its first argument
  instead of capturing it.
- **`curl_version()` and the array form of `curl_getinfo()` leak on every call.**
  Both build their array by decoding a JSON blob with the ordinary `json_decode()`
  builtin, and `json_decode()` never releases the value it decodes — a
  **pre-existing defect in a shared builtin, not something curl introduces**
  (measured with `--gc-stats`: `json_decode('{"a":1,"b":2}', true)` leaks 10
  blocks per call on its own, with no curl involved). Every `curl_version()` call
  therefore leaks, as does every `curl_getinfo($ch)` called without an option.
  Calling either once at startup is fine; calling one per request in a
  long-running process is not. `curl_getinfo($ch, CURLINFO_*)` with an explicit
  option takes a different path and does not leak.
- **An assignment used as a loop condition leaks the assigned value every
  iteration.** `while ($info = curl_multi_info_read($mh))` — php.net's own drain
  loop — leaks one array per iteration. This too is a pre-existing, curl-free
  compiler defect (a plain `while ($x = f())` over any array-returning `f()`
  behaves the same). Write the assignment as its own statement, as the
  [multi example](#the-multi-interface) above does.
- **`curl_close()`, `curl_multi_close()` and `curl_share_close()` are no-ops**, as
  in PHP 8. A handle stays usable until its object is destroyed; `unset()` is what
  actually frees the libcurl session.
- **A `CurlHandle` cannot be serialized.** `serialize($ch)` throws, matching
  php-src (`Serialization of 'CurlHandle' is not allowed`).
- **Freeing a share before its easy handles is safe.** libcurl refcounts shares,
  and elephc's bridge defers the real `curl_share_cleanup()` until the last
  attached easy handle detaches, so `unset()` order does not matter and nothing
  leaks.

## See also

- [Native dependencies](../compiling/native-dependencies.md) — `elephc native add curl`
- [Linking and conditional compilation](../compiling/linking-and-conditional-compilation.md) — `--with-curl`
- [Streams](streams.md) — `file_get_contents('https://…')` and `fopen()` HTTP(S) wrappers, which stay on rustls and are unaffected by curl
- [Eval](eval.md) — the interpreter bridge that hosts curl inside `eval()`
- `examples/curl-get/main.php` — a documented GET with error handling
