---
title: "cURL"
description: "PHP's ext/curl on a statically pinned libcurl 8.21.0 with HTTP/2 and 25 protocols: easy, multi and share interfaces, uploads, callbacks, the protocol matrix, and every documented difference from PHP."
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

The pinned libcurl carries 25 schemes — everything a stock distribution libcurl
of this vintage offers except LDAP. `curl_version()['protocols']` on this build
reports exactly:

```
dict file ftp ftps gopher gophers http https imap imaps mqtt mqtts pop3 pop3s
rtsp scp sftp smb smbs smtp smtps telnet tftp ws wss
```

That list is pinned by a test
(`codegen::curl::easy_handle::curl_version_reports_the_full_pinned_protocol_set`),
so it cannot drift from the build.

| Family | Schemes | Backed by |
|---|---|---|
| Web | `http`, `https`, `ws`, `wss` | built-in + OpenSSL; **HTTP/2** through nghttp2 |
| File transfer | `file`, `ftp`, `ftps`, `tftp`, `sftp`, `scp` | built-in + OpenSSL; SFTP/SCP through libssh2 |
| Mail | `smtp`, `smtps`, `imap`, `imaps`, `pop3`, `pop3s` | built-in + OpenSSL |
| Messaging / streaming | `mqtt`, `mqtts`, `rtsp` | built-in |
| SMB | `smb`, `smbs` | built-in, via NTLM (`--enable-smb --enable-ntlm`) |
| Legacy | `dict`, `gopher`, `gophers`, `telnet` | built-in |

Three outcomes stay distinguishable, and none of them is ever a fabricated
success:

```php
// A built-in scheme really connects (or fails to, honestly):
$ch = curl_init("sftp://127.0.0.1:1/x");
curl_exec($ch);
echo curl_errno($ch);   // 7  (CURLE_COULDNT_CONNECT)

// A scheme libcurl knows but this build lacks:
$ch = curl_init("ldap://example.invalid/x");
curl_exec($ch);
echo curl_error($ch);   // Protocol "ldap" is disabled          (errno 1)

// A scheme libcurl has no handler for at all:
$ch = curl_init("rtmp://example.invalid/x");
curl_exec($ch);
echo curl_error($ch);   // Protocol "rtmp" not supported        (errno 1)
```

`ldap` and `ldaps` are the only *disabled* schemes. `rtmp`, `rtmps`, `rtmpe` —
and any string that is not a cURL scheme at all, like `xyzzy://` — report
`not supported`, because **curl removed RTMP entirely in 8.20.0** (see
`docs/DEPRECATE.md` in the tarball). It is not a build choice elephc makes and
not a library elephc declined to link: there is no RTMP code in libcurl 8.21.0
to enable, so libcurl does not know those schemes rather than knowing them and
refusing.

### What is deliberately not built in

- **LDAP and LDAPS.** They need OpenLDAP, which elephc does not pin. Measured on
  OpenLDAP 2.6.14: a client-only static build against our own OpenSSL works, but
  it yields three archives (~2.3 MB, larger than libcurl itself) that still leave
  `pthread_*` and resolver symbols to the final link — and elephc's catalog has no
  way to declare a system library a managed package needs. Pinning LDAP is
  therefore a change to the package model, not just another recipe.
- **HTTP/3.** curl 8.21.0 removed the standalone `openssl-quic` backend, so the
  only non-experimental QUIC path in this pin is `--with-ngtcp2 --with-nghttp3`
  (quiche is still marked EXPERIMENTAL in the tarball's own
  `docs/EXPERIMENTAL.md`). That is two further pinned packages plus an
  `ngtcp2_crypto_ossl` build; until they are taken, `CURLOPT_HTTP_VERSION`
  returns `false` for `CURL_HTTP_VERSION_3` and `CURL_HTTP_VERSION_3ONLY`, and
  `curl_version()['feature_list']['HTTP3']` is `false`.
- **RTMP.** Removed from curl itself in 8.20.0, so it is not available in this pin
  at any build setting.
- **Brotli and zstd** content encodings. gzip and deflate work through zlib.
- **libpsl** (public-suffix cookie checks) and **libidn2** (IDN host names).

**HTTP/2 works.** `CURLOPT_HTTP_VERSION` accepts `CURL_HTTP_VERSION_2_0`,
`CURL_HTTP_VERSION_2TLS` and `CURL_HTTP_VERSION_2_PRIOR_KNOWLEDGE` alongside the
1.x values, `curl_version()['feature_list']['HTTP2']` is `true`, and
`curl_getinfo($ch, CURLINFO_HTTP_VERSION)` reports what was actually negotiated.

`ws://` and `wss://` deserve a note: libcurl 8.21 builds its WebSocket support
by default, so the schemes appear in `curl_version()['protocols']`, but PHP's
`ext/curl` has never exposed `curl_ws_send()` / `curl_ws_recv()` and neither does
elephc. A `ws://` URL therefore performs the HTTP upgrade handshake through
`curl_exec()` and then has no API to drive the resulting connection. Treat
WebSockets as unsupported.

### `curl_version()` and the sub-libraries

`curl_version()` reports this build's real dependency set, so the keys PHP
age-gates are populated rather than empty:

| Key | Value on this build |
|---|---|
| `version` | `8.21.0` |
| `ssl_version` | `OpenSSL/3.5.7` |
| `libz_version` | `1.3.2` |
| `libssh_version` | `libssh2/1.11.1` |
| `libidn`, `brotli_version` | `""` — not built in |
| `ares`, `ares_num`, `iconv_ver_num`, `brotli_ver_num` | `""` / `0` — not built in |

The `feature_list` entries this build reports `true` are `AsynchDNS`, `IPv6`,
`Largefile`, `libz`, `NTLM`, `SSL`, `TLS-SRP`, `HTTP2`, `UNIX_SOCKETS`,
`HTTPS_PROXY`, `ALTSVC` and `HSTS`. Every other name php publishes is present and
`false` — php never omits a key.

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

### The CA trust store

**HTTPS verifies out of the box, including on a machine other than the one that
compiled the binary.** That is not something libcurl does on its own, so it is
worth knowing how it works.

The managed curl recipe passes no `--with-ca-bundle` / `--with-ca-path`, so
libcurl's `configure` autodetects a CA bundle on the **build machine** and bakes
that absolute path into the archive (`/etc/ssl/cert.pem` on a macOS build host,
`/etc/ssl/certs/ca-certificates.crt` on a Debian one — and *nothing at all* when
the recipe cross-compiles, because `configure` skips the detection entirely in
that case). Left alone, a binary shipped anywhere else fails every verified HTTPS
transfer with `CURLE_SSL_CACERT_BADFILE` (`77`).

elephc's curl bridge therefore resolves a CA bundle at **run** time and sets it
as an ordinary `CURLOPT_CAINFO` — and `CURLOPT_PROXY_CAINFO` — on each handle.
The order, applied once per process at the first `curl_init()`:

1. **`$CURL_CA_BUNDLE`**, if it names an absolute path. Not checked for
   existence — naming a bundle is an instruction, and a wrong one fails loudly
   rather than being silently replaced by a guess.
2. **Nothing at all**, if the baked-in path exists on this machine. A binary
   running on its own build host, or on any system with the same layout, behaves
   exactly as it did before this feature existed.
3. **The first of these that exists**, otherwise:
   `/etc/ssl/certs/ca-certificates.crt` (Debian, Ubuntu, Arch, Alpine, NixOS) ·
   `/etc/pki/tls/certs/ca-bundle.crt` (Fedora, RHEL 6) ·
   `/etc/ssl/ca-bundle.pem` (openSUSE) · `/etc/pki/tls/cacert.pem` ·
   `/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem` (RHEL 7+) ·
   `/etc/ssl/cert.pem` (macOS, Alpine, FreeBSD/OpenBSD) ·
   `/usr/share/ssl/certs/ca-bundle.crt`.
4. **Nothing**, if none of them exists. The handle is left exactly as libcurl
   configured it and libcurl reports its own error. Discovery never relaxes
   verification to make a transfer succeed.

The list is fixed, absolute, and made of root-owned locations a distribution's
own `ca-certificates` package installs. Nothing is derived from the working
directory, `$HOME`, or `$PATH`, and no certificate is ever downloaded. curl's own
`configure` also probes `/usr/local/share/certs/ca-root-nss.crt`; elephc
deliberately does not, because `/usr/local` is the Homebrew prefix on Intel macOS
and is left admin-writable there. On a FreeBSD host that has only that file, set
`CURLOPT_CAINFO` or `$CURL_CA_BUNDLE` explicitly.

**`CURLOPT_CAINFO` always wins.** Setting it replaces the discovered bundle on
that handle, exactly as it would replace libcurl's baked-in default:

```php
curl_setopt($ch, CURLOPT_CAINFO, __DIR__ . "/ca-bundle.pem");
```

**`CURLOPT_CAPATH` composes with it** rather than replacing it, so the handle
verifies against *your directory plus the discovered bundle*:

```php
curl_setopt($ch, CURLOPT_CAPATH, "/etc/ssl/certs");
```

That is the same shape a stock libcurl gives — there, a capath composes with the
*baked-in* bundle — and it is not a choice elephc could avoid: clearing
`CURLOPT_CAINFO` makes libcurl re-inject its compile-time default, which on a
machine where that path does not exist fails the whole transfer with `77` before
your capath is ever read. Set `CURLOPT_CAINFO` to your own bundle alongside the
capath if you need the trust set to be exactly yours.

**HTTPS proxies get the same bundle.** libcurl verifies the TLS connection to a
proxy against a completely separate set of options, with its own copy of the
baked-in default, so tunnelling through one used to fail on a foreign machine for
exactly the reason a direct transfer did:

```php
curl_setopt($ch, CURLOPT_PROXY, "https://proxy.internal:3128");
// The proxy's certificate is verified against the discovered bundle too.
```

`CURLOPT_PROXY_CAINFO` and `CURLOPT_PROXY_CAPATH` behave on that hop exactly as
their non-proxy counterparts do on the origin hop — the first replaces the
discovered bundle, the second composes with it. There is only ever **one**
resolution per process: whether the baked-in path is usable is a fact about the
machine, not about which hop is being verified. A plain `http://` proxy involves
no TLS to the proxy and is unaffected.

`curl_reset($ch)` puts the discovered bundle back (it restores libcurl's own
defaults, which would otherwise mean the dead baked-in path), and
`curl_copy_handle($ch)` carries whichever bundle the original had. Share handles
and the multi interface do not interact with any of this: `CURLOPT_CAINFO` is a
per-handle option, and a `CurlShareHandle` shares DNS, SSL sessions, cookies and
connections — never options. Disabling verification with
`CURLOPT_SSL_VERIFYPEER => false` also "works" and is, as always, a bad idea.

#### Where this differs from php

| | php 8.4 | elephc |
|---|---|---|
| Portability comes from | the distro's own libcurl, built for that distro | runtime discovery, above |
| `curl.cainfo` / `openssl.cafile` php.ini | honored | **not implemented** — elephc has no php.ini for curl |
| `$CURL_CA_BUNDLE` | ignored | **honored** (parity with the `curl` command-line tool, which reads the same variable) |
| `$SSL_CERT_FILE` / `$SSL_CERT_DIR` | ignored | ignored — this build has no OpenSSL default-verify-paths fallback compiled in, so they were never consulted |

Honoring `$CURL_CA_BUNDLE` is the deliberate part: it is the only process-wide
hook available when the handles are created by library code you do not control,
which is exactly the situation `curl.cainfo` exists to solve in php — and for an
AOT-compiled binary the alternative is recompiling. The `curl` command-line tool
resolves it the same way, ahead of its own baked-in default.

**What that costs, stated plainly.** `$CURL_CA_BUNDLE` never disables
verification — there is no value of it that turns peer verification off, and an
unreadable one fails closed with `77` — **but it does decide which roots you
trust**, and it is ranked above a baked-in path that works. Two consequences
worth designing around:

- **A stale value breaks a working deployment.** A `CURL_CA_BUNDLE` left in a CI
  image, a base container, or a shell profile applies to every elephc binary that
  inherits it. If it points somewhere that does not exist on *this* host, every
  HTTPS transfer in the process fails with `77` rather than the variable being
  quietly ignored. That is deliberate — silently substituting a different trust
  store for the one an operator named is worse — but it means the variable should
  be set per-service, not exported globally.
- **Anyone who can set the process environment chooses the trust anchors.** In a
  deployment where the environment is less trusted than the binary (a shared
  runner, a `Passenger`/CGI-style spawner whose environment is assembled from
  configuration you do not own), that is a real capability. It is the same
  capability `curl`, `git` (`GIT_SSL_CAINFO`) and OpenSSL (`SSL_CERT_FILE`) hand
  out, and it is strictly weaker than the ability to replace the binary or
  preload a library — but if your threat model excludes it, `unset CURL_CA_BUNDLE`
  in the service's launcher and set `CURLOPT_CAINFO` in code instead.

Both consequences apply **identically to HTTPS-proxy trust**: the same one
resolution feeds `CURLOPT_PROXY_CAINFO`, so a stale `$CURL_CA_BUNDLE` breaks the
proxy hop too, and whoever sets it chooses the roots the *proxy* certificate is
verified against as well as the origin's. Pinning one hop in code and leaving the
other to the environment is the mistake to avoid — set `CURLOPT_PROXY_CAINFO`
alongside `CURLOPT_CAINFO` when you pin.

One thing discovery does **not** do: it never picks a `CURLOPT_CAPATH` (or
`CURLOPT_PROXY_CAPATH`) directory. A hashed-certificate directory needs an
OpenSSL `c_rehash` layout that a filesystem check cannot confirm, so only bundle
*files* are ever discovered.

```php
// This reports the path libcurl was BUILT with, not the one in force.
$info = curl_getinfo(curl_init());
echo $info["cainfo"];   // /etc/ssl/cert.pem
```

That is not an elephc quirk: libcurl answers `CURLINFO_CAINFO` from a
compile-time constant, so it is unchanged by `curl_setopt($ch, CURLOPT_CAINFO,
…)` too, in php exactly as here.

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
| `CURLOPT_FNMATCH_FUNCTION`, `CURLOPT_PREREQFUNCTION`, `CURLOPT_SSH_HOSTKEYFUNCTION` | Callbacks outside the six implemented ones (below). SSH itself *is* built in, so `CURLOPT_SSH_KNOWNHOSTS` and the rest of the `CURLOPT_SSH_*` family work — it is only the host-key **callback** that is unimplemented. |

`CURLINFO_HEADER_OUT` — which is a `curl_setopt()` option despite its name — is
rejected the same way, so `curl_getinfo($ch, CURLINFO_HEADER_OUT)` has nothing to
report.

`curl_multi_setopt()` implements 8 of PHP's 9 `CURLMOPT_*` options;
`CURLMOPT_PUSHFUNCTION` is rejected because it is a callback, and this build
carries no callback machinery on the multi handle — not because of HTTP/2, which
*is* built in. `curl_share_setopt()` implements `CURLSHOPT_SHARE` and
`CURLSHOPT_UNSHARE` over the five `CURL_LOCK_DATA_*` values PHP exposes.

### PHP-layer options

These are implemented by elephc's PHP layer rather than forwarded to libcurl,
matching php-src:

| Option | Behavior |
|---|---|
| `CURLOPT_RETURNTRANSFER` | Capture the body; `curl_exec()` returns a `string` |
| `CURLOPT_BINARYTRANSFER` | No-op, as in modern PHP |
| `CURLOPT_SAFE_UPLOAD` | Always on; `@file` strings in `CURLOPT_POSTFIELDS` stay literal. Disabling it raises `ValueError`, as in PHP 8. |
| `CURLOPT_PRIVATE` | Stores an arbitrary PHP value, read back verbatim by `curl_getinfo($ch, CURLINFO_PRIVATE)` |

With no `CURLOPT_RETURNTRANSFER`, the body is written to stdout and `curl_exec()`
returns `true` — the PHP CLI behavior.

`CURLOPT_HEADER` (prepend response headers to the body) is **not** one of these —
despite the name, php-src forwards it straight to libcurl unchanged, and elephc does
the same: it is an ordinary `long` option, and real libcurl implements the
header-in-body behavior on its own.

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
> elephc also accepts any resource where php requires a *stream* resource — narrow
> in practice, since PHP 8 has made almost every other resource type an object.

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

`eval()` reaches the same pinned libcurl through the same C ABI, and — as of the
R3-C work — ships **almost the whole compiled surface**:

**Available in `eval()`:**

- The complete **easy** interface (16 functions) and all 689 constants, with
  `curl_setopt()`'s full table-driven option dispatch — every LONG / STRING /
  SLIST / OFF_T / PHP-layer option works, not a hand-picked subset.
- The **multi** interface: `curl_multi_init`, `_add_handle`, `_remove_handle`,
  `_exec`, `_select`, `_info_read`, `_getcontent`, `_setopt`, `_errno`,
  `_strerror`, `_close`, plus PHP 8.5's `curl_multi_get_handles`.
  `curl_multi_exec()`'s `$still_running` and `curl_multi_info_read()`'s
  `$queued_messages` are written back on every call shape a PHP program normally
  writes — see "By-reference parameters" below for the one that is not.
- The **share** interface: `curl_share_init`, `_setopt`, `_errno`, `_strerror`,
  `_close`, plus PHP 8.5's `curl_share_init_persistent` — and
  `curl_setopt($ch, CURLOPT_SHARE, $sh)`.
- `CURLFile` / `CURLStringFile` / `curl_file_create()`, and with them the array
  (`multipart/form-data`) form of `CURLOPT_POSTFIELDS`, walked into the same
  `curl_mime` structure compiled code builds.
- All six **callback** options: `CURLOPT_WRITEFUNCTION`, `_HEADERFUNCTION`,
  `_READFUNCTION`, `_PROGRESSFUNCTION`, `_XFERINFOFUNCTION`, `_DEBUGFUNCTION`.
  A PHP exception thrown inside one aborts the transfer and surfaces as an
  ordinary catchable throwable *after* `curl_exec()` returns, with
  `curl_errno()` answering `0` — exactly what compiled code and php-src do.

```php
curl_version(); // links elephc_curl into this program at all
$bodies = eval('
    $mh = curl_multi_init();
    $a = curl_init("https://example.com/a");
    curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
    curl_multi_add_handle($mh, $a);
    $still = 0;
    do {
        $code = curl_multi_exec($mh, $still);
        if ($still > 0) { curl_multi_select($mh, 1.0); }
    } while ($still > 0 && $code == CURLM_OK);
    return curl_multi_getcontent($a);
');
```

**Not available in `eval()`:** the four **stream** options — `CURLOPT_FILE`,
`CURLOPT_WRITEHEADER`, `CURLOPT_INFILE`/`CURLOPT_READDATA`, `CURLOPT_STDERR`.
They answer `false` plus the same honest "option … is not supported by this
build" warning a genuinely uncarryable option gets, never a fatal. Compiled
code implements all four.

**Why the stream options specifically, when callbacks now work.** Compiled code
implements them by *composing* its callback slots with internal PHP closures
declared in the curl prelude, which `fwrite()` to (or `fread()` from) a stream
held on the `CurlHandle` object. `eval()` can install a callback but cannot
install one of *those*: they are AOT prelude closures over an AOT object the
interpreter has neither of. What is left is a native re-implementation, which is
a different piece of work — the four options are not one rule but three
interacting ones, each branch measured against PHP 8.4.20:

- the write and header sinks share a single **last-set-wins** mode with
  `CURLOPT_RETURNTRANSFER` and `CURLOPT_WRITEFUNCTION`, and `null` on any of them
  falls back to stdout rather than to a previously selected sibling;
- the read source is a **fixed precedence** in which `CURLOPT_READFUNCTION`
  outranks `CURLOPT_INFILE` in *both* setting orders;
- the debug sink is a **one-way shadow**: touching `CURLOPT_DEBUGFUNCTION` at
  all, even with `null`, permanently disables `CURLOPT_STDERR`.

Shipping three of those correctly and one subtly wrong would be worse than the
current honest refusal, so they stay refused.

**No curl name is intercepted any more.** Earlier releases routed
`curl_multi_*`/`curl_share_*`/`curl_file_create` and `new CURLFile(...)` away
from the interpreter's native-function/native-class fallbacks and rejected them
with an "eval() fragment uses an unsupported construct" fatal. That guard existed
because the fallback resolved those names to the *real* compiled implementation
whenever the host program also linked `elephc_curl`, handing back a genuine AOT
`CurlMultiHandle` that looked like it worked — until it was mixed with an
eval-owned easy handle and failed confusingly, because the two object spaces do
not interoperate. It is gone: the multi and share names now have real eval
implementations answered before any fallback, and `CURLFile`/`CURLStringFile`
are deliberately served *by* that fallback, because — unlike every other curl
class — they wrap no native handle and the real compiled data class works
correctly inside `eval()`.

Further differences inside `eval()`:

- **A handle is not a `CurlHandle` object.** It is a resource-like cell, so
  `gettype($ch)` reports `"resource"` and `$ch instanceof CurlHandle` is false.
  This mirrors `hash_init()`'s long-standing `HashContext` behavior in eval.
  Every `curl_*` function still works on it.
- **Handles cannot cross the eval boundary.** An eval-created handle passed out
  to compiled code is an opaque cell with no `CurlHandle` class instance behind
  it, and an AOT `CurlHandle` passed into an `eval()` string is not accepted by
  eval's curl functions.
- **`--with-curl` is required** when curl appears *only* inside an `eval()`
  string, because usage detection reads the compiled source, not the string.

- **By-reference parameters are written back on the ordinary call shapes only.**
  `curl_multi_exec($mh, $still)` and `curl_multi_info_read($mh, $queued)` assign
  their out-parameter when called normally, through a variable function, or via
  `call_user_func_array()` with a referenceable argument. Every *other* shape —
  notably `call_user_func('curl_multi_exec', $mh, $n)`, which passes its
  arguments by value — cannot write back, and eval emits
  `curl_multi_exec(): Argument #2 ($still_running) must be passed by reference,
  value given` and continues. Real PHP throws
  `Error: curl_multi_exec(): Argument #2 ($still_running) could not be passed by
  reference` for a non-referenceable argument, and compiled elephc rejects the
  same call at *compile* time against the prelude's `int &$still_running`. The
  warning is what every by-reference builtin in the interpreter does
  (`preg_match()`'s `$matches`, `flock()`'s `$would_block`, `settype()`'s
  `$var`), so this is an interpreter-wide shape rather than a curl one.

- **`$still_running` is not assigned when a callback throws.** If a
  `curl_multi_exec()` call is aborted by an exception from a curl callback, the
  by-reference count keeps its previous value; PHP assigns it (measured: `0`).
  Compiled elephc has the identical gap, so eval and AOT agree — this is a
  divergence from PHP, not between the two elephc backends.

- **`curl_multi_info_read()`'s `handle` is the same *handle*, not the same
  *object*.** PHP guarantees `$info['handle'] === $ch`. Inside `eval()` a curl
  handle is a resource-like cell rather than an object (see the first bullet
  above), so the reported value addresses the same underlying handle — every
  `curl_*` function, `curl_multi_getcontent()` included, works on it — but is not
  guaranteed to be the identical cell, and `===` against the original is
  therefore not a supported test. Compiled code keeps real object identity.

- **`CURLOPT_READFUNCTION`'s `$fd` argument is always `null`.** PHP passes the
  `CURLOPT_INFILE` stream there. `eval()` does not implement the four stream
  options at all, so there is never a stream to pass — and `null` is exactly what
  PHP itself passes for a handle with no `CURLOPT_INFILE`. Compiled code passes
  the real stream.

- **Callback arguments are allocated per invocation and released with the eval
  context, not after each call.** Every callback invocation builds fresh runtime
  cells for `$ch` and for its data arguments (2 cells for write/header, 3 for
  read and debug, 5 for progress/xferinfo). They are bound into the callback's
  scope as borrowed cells, so the callback's own frame teardown does not free
  them, and — like every other callback-taking builtin in the interpreter
  (`preg_replace_callback()`, `array_map()`, `array_filter()`, `usort()`) — the
  builtin does not free them either. They are reclaimed when the eval context's
  heap is torn down. This is bounded by the number of callback invocations in one
  eval context and matters most for `CURLOPT_WRITEFUNCTION`, which fires once per
  received chunk.

**Aligned with AOT (previously diverged):**

- **A non-`CurlHandle` value passed to any curl handle function throws.**
  `curl_close()`, `curl_escape()`, `curl_setopt()`, and every other function
  that takes `$handle` first throw a catchable `TypeError` for a value that
  is not a live curl easy handle — `curl_close()` used to accept literally
  anything with no check at all:
  ```php
  eval('curl_close("not a handle");');
  // TypeError: curl_close(): Argument #1 ($handle) must be of type CurlHandle, string given
  ```
  The "given" type name matches AOT's own `gettype()`-based wording exactly
  (including AOT's own pre-existing divergence from real php-src: `gettype()`
  says `"integer"`, not php-src's newer `"int"` — eval mirrors *AOT*, not
  php-src, here).
- **An invalid option number throws a catchable `ValueError`, not a fatal.**
  Matches the [Options](#options) section above:
  ```php
  eval('$ch = curl_init(); curl_setopt($ch, 987654, 1);');
  // ValueError: curl_setopt(): Argument #2 ($option) is not a valid cURL option
  ```
- **`CURLOPT_SAFE_UPLOAD` set falsy throws a catchable `ValueError`,** matching
  AOT's `"curl_setopt(): Disabling safe uploads is no longer supported"`.
- **A non-scalar `$value` for an ordinary option throws a catchable
  `TypeError`,** matching AOT's `"curl_setopt(): Argument #3 ($value) must be
  of type string|int|float|bool, … given"`.
- **A non-array value for a `CURLOPT_*` string-list option throws a catchable
  `TypeError`** (`"...Argument #3 ($value) must be of type array, … given"`),
  and **a non-scalar item inside the array also throws** (`"...must be an
  array of strings for this option"`) instead of being silently
  `(string)`-cast. Compiled code has always thrown both; eval used to answer
  `false` for the first and cast the second.
- **`curl_escape()`/`curl_unescape()` throw a catchable `RuntimeException` on
  a genuine libcurl encode/decode failure,** matching AOT's
  `"curl_escape(): libcurl could not URL-encode the string"` /
  `"curl_unescape(): libcurl could not URL-decode the string"` — eval used to
  answer `false` instead.

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
- [Builtin reference — Network](builtins/network.md) — the generated per-function pages for the 34 `curl_*` builtins, with their AOT and `eval()` availability
- `examples/curl-get/main.php` — a documented GET with error handling
