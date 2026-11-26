---
title: libssl TLS session backend foundation
author: Guillaume
date: 2026-11-26
branch: feat/php-8.6-support
touches:
  - Cargo.lock
  - Cargo.toml
  - docs/internals/the-runtime.md
  - docs/php/php-8.6.md
  - crates/elephc-tls-openssl/Cargo.toml
  - crates/elephc-tls-openssl/build.rs
  - crates/elephc-tls-openssl/src/lib.rs
---

## Decided
- Add `crates/elephc-tls-openssl` as a separate libssl-backed client backend instead of replacing the rustls `elephc-tls` bridge. It keeps the default https path unchanged while giving session options a real `SSL_SESSION` implementation.
- The handle owns an `SSL`/`SSL_CTX` pair plus the `TcpStream`; `SSL_set_fd` uses OpenSSL's no-close socket BIO, and `SSL_free` runs before dropping the stream.
- `connect_session` imports a PEM session and installs it before `SSL_connect`; the crate test runs a local TLS 1.2 server with a server session cache and asserts `SSL_session_reused() == 1` on both peers.

## Tried and ruled out
- Importing an OpenSSL session through rustls: `ClientSessionStore` only accepts opaque `Tls12ClientSessionValue`/`Tls13ClientSessionValue` with `pub(crate)` constructors, and rustls 0.23 has no public external-PSK API.
- Replacing `elephc-tls` wholesale: it would force the managed OpenSSL package onto every https program and rewrite the existing rustls behaviour before the session wiring exists.

## Watch out
- The backend is NOT wired into the AOT `https://` wrapper yet: the emitter still selects the rustls bridge, and `session_data` still needs a bridge choice plus PEM extraction from the `Openssl\Session` value. PSK/early-data are not implemented.
- The crate has no native link in production; the final program must resolve `libssl.a`/`libcrypto.a` from the managed `openssl-crypto` package, while tests use `ELEPHC_OPENSSL_LIB_DIR`.
