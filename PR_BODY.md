# PHP streams / sockets subsystem

Implements the PHP streams, sockets, and network I/O subsystem for the native
compiler. The everyday PHP stream surface is covered end-to-end; a set of
genuinely-large or upstream-blocked items are deferred and tracked in `ROADMAP.md`
(the subsystem is intentionally landed as **partially complete**).

Rebased cleanly onto current `main` and squashed into 6 logical commits.

## What's included

- **Core stream model** — `PhpType` stream resource; `fopen`/`fread`/`fwrite`/
  `fclose`/`fgets`/`fseek`/`ftell`/`feof`/`fflush`/`fstat`, directory iteration,
  `popen`/`pclose`.
- **Sockets** — `stream_socket_server`/`client`/`accept`/`sendto`/`recvfrom`/
  `get_name`/`pair`/`shutdown`, `fsockopen`/`pfsockopen`, TCP/UDP/Unix transports,
  IPv4 + IPv6, DNS (`gethostby*`).
- **TLS** — `https://` honoring `ssl.*` context options, `stream_socket_enable_crypto`
  on a live TCP fd, client certificates (mutual TLS), `ftps://` — via a new
  `elephc-tls` rustls staticlib that keeps the shared runtime libc-only through
  function-pointer indirection.
- **Wrappers** — `data://`, `http://`, `ftp://`, `compress.zlib://`,
  `compress.bzip2://`, `phar://` (read + signed single-entry write).
- **Filters & contexts** — `stream_filter_append`/`prepend`/`remove` with the
  built-in transforms (`string.*`, `convert.*`, `zlib.*`, `bzip2.*`, `dechunk`,
  base64/quoted-printable) and the `$params` argument, user filters
  (`stream_filter_register`), `stream_bucket_*`, `stream_context_*`, the stream
  notification callback.
- **Userspace wrappers** — `stream_wrapper_register` with a full method vtable
  (open/read/write/close/seek/eof/flush/lock/truncate/cast + path & directory ops).

## Notable infrastructure interaction

Adopts `main`'s shared-runtime-surface change (`849d0d39`): x86_64 now emits the
same runtime helpers as AArch64, so the old per-target `x86_minimal.rs` allowlist
is removed and every streams `__rt_*` helper is wired once.

## Validation

- macOS arm64 — full `cargo test`: **3773 passed, 0 failed** (9 ignored: SDL2).
- Docker Linux **x86_64** — streams suite + bzip2: green.
- Docker Linux **arm64** — streams suite + bzip2: green.
- `cargo build` clean (0 warnings); `git diff --check` clean.

## Deferred (tracked in `ROADMAP.md` → "Streams — remaining work")

Userspace-filter `$this->params`; `phar://` compressed runtime reads, advanced/
multi-entry/OOP `Phar`/`PharData`, tar/zip variants; TLS `ciphers`/`security_level`
(rustls has no equivalent — honest no-op); some low-level gaps (true non-blocking,
`realpath_cache_*`, `lchown`/`lchgrp`).
