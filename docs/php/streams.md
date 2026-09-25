---
title: "Streams"
description: "Stream resources, wrappers, contexts, filters, sockets, TLS, and process pipes."
sidebar:
  order: 14
---

## Resource model

Streams are PHP `resource` values. File handles, standard streams, directory
streams, socket streams, process pipes, stream contexts, and stream filters all
use the resource runtime tag instead of plain integers.

`fopen()` returns `resource|false`: successful opens produce a stream resource,
while failed opens emit a suppressible runtime warning and return `false`.
Passing that `false` value to a stream builtin is a fatal runtime TypeError, so
code should guard failed opens before using the handle.

`STDIN`, `STDOUT`, and `STDERR` are stream resources. `gettype(STDIN)` returns
`"resource"`, `is_resource(STDIN)` returns `true`, and
`get_resource_type(STDIN)` returns `"stream"`.

## Basic stream I/O

| Function | Signature | Description |
|---|---|---|
| `fopen()` | `fopen($filename, $mode, $use_include_path = false, $context = null): resource\|false` | Open a file, wrapper URL, socket-like wrapper, or temporary/memory stream. Modes `r`, `w`, `a`, `r+`, `w+`, and `a+` are supported. The optional args are evaluated in source order; v1 consumers still read options from the single global stream-context slot. |
| `fclose()` | `fclose(resource $handle): bool` | Close a stream. Closing a `phar://` write stream finalizes the archive, and closing a filtered stream runs pending filter cleanup such as user-filter `onClose()`. |
| `fread()` | `fread(resource $handle, $length): string` | Read up to `$length` bytes. Attached read filters and user-wrapper `stream_read()` methods are honored. |
| `fwrite()` | `fwrite(resource $handle, $data): int` | Write bytes and return the byte count. Attached write filters and user-wrapper `stream_write()` methods are honored. |
| `fprintf()` | `fprintf(resource $handle, string $format, ...$values): int` | Format like `sprintf()` and write to the stream. |
| `vfprintf()` | `vfprintf(resource $handle, string $format, array $values): int` | Like `fprintf()`, with format values supplied as an array. |
| `fscanf()` | `fscanf(resource $handle, string $format): array` | Read one line and parse it with the `sscanf()` engine. v1 supports the two-argument array-returning form and conversions `%d`, `%f`, `%s`, and `%%`. |
| `fgets()` | `fgets(resource $handle, int $length = null): string\|false` | Read a line until newline, EOF, or `$length`; returns `false` at EOF. |
| `fgetc()` | `fgetc(resource $handle): string\|false` | Read one byte, or `false` at EOF/failure. |
| `feof()` | `feof(resource $handle): bool` | Report whether the stream is at EOF. |
| `fseek()` | `fseek(resource $handle, $offset [, $whence]): int` | Seek a stream. User wrappers route through `stream_seek()`. |
| `ftell()` | `ftell(resource $handle): int` | Return the current stream position. User wrappers route through `stream_tell()`. |
| `rewind()` | `rewind(resource $handle): bool` | Seek to the start of the stream. |
| `fgetcsv()` | `fgetcsv(resource $handle [, $sep]): array\|false` | Read and parse one CSV line, or `false` at end of file — the arm that terminates the manual's `while (($row = fgetcsv($h)) !== false)` loop; an empty CSV line still returns an array. User wrappers are read through `stream_read()`. |
| `fputcsv()` | `fputcsv(resource $handle, $fields [, $sep]): int` | Format and write one CSV line. User wrappers are written through `stream_write()`. |
| `readline()` | `readline([$prompt]): string` | Read a line from standard input. |
| `readfile()` | `readfile($filename): int\|false` | Open a path or wrapper URL, stream it to stdout, and return copied bytes; returns `false` when open fails. |
| `fpassthru()` | `fpassthru(resource $handle): int` | Stream the remaining bytes of an open handle to stdout, returning `-1` on read failure. |
| `stream_get_contents()` | `stream_get_contents(resource $handle, ?int $length = null, int $offset = -1): string\|false` | Read remaining bytes from the stream. `$offset >= 0` seeks there first (seekable streams / user wrappers via `stream_seek()`) and returns `false` if that seek fails; a finite `$length` reads at most that many bytes (a `null`/negative `$length` reads to EOF). The bounded form loops through `fread` until it fills `$length`, reaches EOF, or receives an empty read. |
| `stream_copy_to_stream()` | `stream_copy_to_stream(resource $from, resource $to, ?int $length = null, int $offset = -1): int\|false` | Copy bytes from one stream to another, returning the count. `$offset >= 0` seeks the source first (seekable streams / user wrappers via `stream_seek()`) and returns `false` if that seek fails; a finite `$length` copies at most that many bytes (a `null`/negative `$length` copies to EOF). The bounded form drives a chunked read/write loop and clamps wrapper chunks that exceed the requested count. |
| `stream_get_line()` | `stream_get_line(resource $handle, int $length [, string $ending]): string` | Read up to `$length` bytes, stopping at and consuming `$ending` when supplied. |
| `flock()` | `flock(resource $handle, int $op, &$would_block = null): bool` | Advisory locking. `LOCK_SH`, `LOCK_EX`, `LOCK_UN`, and `LOCK_NB` are supported; user wrappers route through `stream_lock(int $operation)`. |
| `tmpfile()` | `tmpfile(): resource\|false` | Create an anonymous temporary stream backed by a `/tmp/elephc-XXXXXX` file that is immediately unlinked. |
| `fstat()` | `fstat(resource $handle): array\|false` | Return the same stat shape as `stat()`, but for an open stream. User wrappers route through `stream_stat()`. |
| `ftruncate()` | `ftruncate(resource $handle, $size): bool` | Truncate or extend a stream. User wrappers route through `stream_truncate(int $new_size)`. |
| `fflush()` | `fflush(resource $handle): bool` | Flush buffered output. elephc streams are unbuffered, so this maps to `fsync()`. |
| `fsync()` | `fsync(resource $handle): bool` | Flush data and metadata to durable storage. |
| `fdatasync()` | `fdatasync(resource $handle): bool` | Flush data only. On macOS, this falls back to `fsync()`. |

`stream_set_chunk_size($stream, $size)` returns the previous chunk size for the
stream. The first call reports the default `8192`; later calls report the value
set by the previous call. v1 tracks the value per fd but does not yet change read
granularity.

`stream_set_read_buffer()` and `stream_set_write_buffer()` return `0`. elephc
streams are unbuffered, so the accepted buffer size does not change behavior.

## Built-in wrappers

| Wrapper | Description |
|---|---|
| `file` | Normal filesystem streams. |
| `php://stdin`, `php://stdout`, `php://stderr` | Standard descriptors 0, 1, and 2. `php://input` aliases stdin, and `php://output` aliases stdout. |
| `php://memory`, `php://temp` | Seekable in-memory streams backed by an anonymous temporary buffer. `php://temp/maxmemory:N` is accepted and ignored. |
| `php://filter` | Opens an underlying resource and attaches one built-in filter at open time, for example `php://filter/read=string.toupper/resource=php://temp`. |
| `data://` | RFC 2397 inline payload streams. Base64 and percent-decoded payloads are supported. The URI must be a string literal. |
| `phar://` | Read or write PHAR entries. Literal reads happen at compile time and embed the entry in the binary; non-literal reads happen at runtime. Native PHAR, tar-based PHAR, and zip-based PHAR containers are readable; native PHAR gzip/bzip2 entries and ZIP deflate entries are decoded transparently. |
| `ftp://` | Anonymous binary passive FTP read streams. `fopen()` requires a literal URL; `file_get_contents()` also accepts runtime string URLs. Credentials in the URL are ignored in v1. |
| `ftps://` | Explicit FTP over TLS using `AUTH TLS`, with TLS on both control and data channels. `fopen()` requires a literal URL; `file_get_contents()` also accepts runtime string URLs. |
| `http://` | HTTP/1.0 `GET` read streams. `fopen()` requires a literal URL; `file_get_contents()` also accepts runtime string URLs. v1 does not follow redirects and buffers up to 1 MiB. |
| `https://` | Same as `http://`, but over TLS through the `elephc-tls` static library. Programs using it auto-link `-lelephc_tls`; programs that do not use TLS pay no extra link cost. |
| `compress.zlib://` | Read-only wrapper that opens the underlying file and applies `zlib.inflate`. |
| `compress.bzip2://` | Read-only wrapper that opens the underlying file and decompresses it through libbz2. |
| `glob://` | Directory-style wrapper for iterating paths matching a glob pattern through `opendir()` / `readdir()`. |

`phar://` write streams buffer one uncompressed entry in memory. `fclose()` and
`file_put_contents("phar://archive.phar/entry", $data)` insert or replace that
entry in a SHA1-signed PHAR archive while preserving existing entries.
`file_put_contents()` and write-mode `fopen()` also accept runtime-built
`phar://` URLs. Native PHAR, tar-based PHAR, and zip-based PHAR containers are
writable; ZIP writes preserve stored/deflated entries and compression controls
can rewrite ZIP entries between stored and deflated forms. ZIP entries written
with a streaming data descriptor are read transparently, and ZIP64 archives
(over 65535 entries, or sizes/offsets over 4 GiB) are both read and written.
Traditional-PKWARE (ZipCrypto) encrypted ZIP entries can be read and written
after calling the `setZipPassword(string $password)` compiler extension on a
`Phar`/`PharData` object: once a password is set, encrypted entries are decrypted
on read and zip entries are encrypted on write (the stub is encrypted too; the
`.phar/signature.bin` entry stays in the clear). ZipCrypto is cryptographically
weak — it is kept for compatibility with legacy archives, not as a real
confidentiality mechanism. Its per-entry encryption headers use operating-system
entropy; on Windows the bridge obtains those bytes from `BCryptGenRandom` and
fails the archive write if the system provider is unavailable. `Phar` and
`PharData` expose a
baseline OOP surface with constructors, format/compression/signature constants,
`addFromString()`, `delete()`, `compressFiles()`, `decompressFiles()`,
mixed metadata/string stub accessors, path helpers, and ArrayAccess read/write/isset
over the same `phar://` paths. ArrayAccess reads return `PharFileInfo` objects
with `getContent()` for payload reads and
`setMetadata()`/`getMetadata()`/`hasMetadata()`/`delMetadata()` for per-file
metadata. `foreach` over a `Phar` / `PharData`
object visits entries scanned from the archive at construction time plus entries
written through that object, yielding `entryName => PharFileInfo`.
`unlink("phar://archive/entry")` and `unset($phar["entry"])` remove entries
while preserving sibling entries. Native PHAR compression controls support
`Phar::GZ`, `Phar::BZ2`, and `Phar::NONE`; ZIP compression controls support
`Phar::GZ` and `Phar::NONE`.

`setMetadata()`/`getMetadata()`/`hasMetadata()`/`delMetadata()` and
`setStub()`/`getStub()` **persist into the archive file** for all three families,
so the global metadata and stub round-trip across fresh `Phar`/`PharData` objects
and across processes (and are interchangeable with the PHP interpreter). Metadata
is stored PHP-`serialize()`d — in the manifest metadata field for native PHAR, in a
`.phar/.metadata.bin` entry for tar, and in the ZIP archive comment for zip; the stub
is stored as the byte prefix for native PHAR and as a `.phar/stub.php` entry for
tar/zip. `setStub()` requires the stub to contain `__HALT_COMPILER();` (matching PHP).
The reserved `.phar/*` control entries are hidden from the entry listing and iteration.

Per-file metadata persists the same way through the `PharFileInfo` returned by
ArrayAccess: `$phar["entry"]->setMetadata(...)`, `getMetadata()`, `hasMetadata()`,
and `delMetadata()` round-trip across fresh objects and the PHP interpreter. It is
stored in the per-entry manifest field for native PHAR, in a
`.phar/.metadata/<entry>/.metadata.bin` side entry for tar, and in the per-entry ZIP
central-directory file comment for zip.

Whole-archive compression is supported on tar-based `PharData`: `compress(Phar::GZ)`
and `compress(Phar::BZ2)` write a sibling `.tar.gz` / `.tar.bz2` and return a fresh
`PharData` for it, while `decompress()` writes the plain `.tar` back; the compressed
archives are read transparently (and are interchangeable with the PHP interpreter).
When reading a whole-archive gzip or bzip2 wrapper, Elephc limits decompressed output
to the smaller of 1024x the compressed size and 64 MiB. The same 1024x ratio and
64 MiB absolute ceiling apply independently to every compressed native PHAR or ZIP
entry. Native PHAR, tar, and ZIP lookup scan and authenticate the container but materialize
only the requested entry, so unrelated payloads are not copied or decompressed. These
Elephc-specific safety ceilings intentionally diverge
from PHP: PHP may accept a highly expanding or larger archive that Elephc rejects.
Per-entry compression for native PHAR / zip stays on `compressFiles()` /
`decompressFiles()`.

Signatures are supported through `setSignatureAlgorithm()` / `getSignature()` across
native PHAR, tar, and zip phars. `setSignatureAlgorithm(Phar::MD5|Phar::SHA1|Phar::SHA256|Phar::SHA512)`
applies a hash signature, and `setSignatureAlgorithm(Phar::OPENSSL, $privateKey)` signs
with RSA-SHA1 using a PEM private key (PKCS#1 or PKCS#8). Native PHARs store the signature
in their trailer; tar and zip phars store it in a `.phar/signature.bin` control entry. The
resulting signature is verifiable by the PHP interpreter (for OpenSSL, place the matching
public key in `<archive>.pubkey`; signing does not create this sidecar). Elephc also
requires that sidecar when opening an
OpenSSL-signed archive: reads, entry listings, metadata access, mutations, compression,
and `getSignature()` fail closed when the key is missing, malformed, or does not verify
the archive. PEM public keys may use SubjectPublicKeyInfo (`BEGIN PUBLIC KEY`) or PKCS#1
(`BEGIN RSA PUBLIC KEY`) encoding. APIs that receive archive bytes without a filesystem
path cannot locate a sidecar and therefore reject OpenSSL-signed input.
`getSignature()` returns `['hash' => <uppercase hex>,
'hash_type' => 'MD5'|'SHA-1'|'SHA-256'|'SHA-512'|'OpenSSL']`.

> **Security note on OpenSSL signing.** The RSA implementation behind
> `setSignatureAlgorithm(Phar::OPENSSL, …)` carries a known timing side-channel on
> private-key operations ([RUSTSEC-2023-0071], the Marvin attack), and no fixed release of
> that crate exists. Recovering a key through it requires an attacker who can submit many
> chosen inputs to the signing operation **and** measure how long each one takes. Signing a
> phar is an authoring-time operation on a key the program supplies itself, so elephc
> exposes no such oracle on its own — but a program that turns phar signing into a remote,
> attacker-triggerable service would. Sign during a build, not in response to a request.
> Verification (`getSignature()`, opening a signed archive) uses only the **public** key and
> is unaffected.

[RUSTSEC-2023-0071]: https://rustsec.org/advisories/RUSTSEC-2023-0071

Metadata persistence covers the same scalar+array subset as
[`serialize()`/`unserialize()`](system-and-io.md#serialization); object metadata is not
serialized.

`file_get_contents($url)` recognizes runtime `http://`, `https://`, `ftp://`,
and `ftps://` strings before falling back to `phar://`/filesystem handling.
Because the scheme is not known statically, non-literal `file_get_contents()`
conservatively links `elephc-tls`, `elephc-phar`, zlib, and libbz2.

`https://`, `ftps://`, and `stream_socket_enable_crypto()` use `elephc-tls`
(rustls, the `ring` crypto provider, and Mozilla webpki roots). TLS contexts can
override trust with `ssl.cafile` or `ssl.capath`, set `ssl.peer_name`, or relax
verification with `ssl.verify_peer = "0"`, `ssl.allow_self_signed`, or
`ssl.verify_peer_name = "0"`. Client certificates are supported when both
`ssl.local_cert` and `ssl.local_pk` point at readable PEM files; `local_pk` may
be omitted when the certificate file also contains the key. `ssl.passphrase`
decrypts PKCS#8 `ENCRYPTED PRIVATE KEY` blocks and OpenSSL's legacy encrypted
RSA/EC PEM envelopes (AES-CBC, DES-CBC, or 2/3-key 3DES-CBC), without a system
OpenSSL dependency.

The rustls backend supports a strict, fail-closed subset of OpenSSL's
`ssl.ciphers` surface. A colon-separated list may use the exact, case-sensitive
OpenSSL names for rustls-backed ECDHE AES-GCM or ChaCha20 TLS 1.2 suites. PHP's
`SSL_CTX_set_cipher_list()` leaves TLS 1.3 suites at their defaults; elephc does
the same and ignores their canonical names only when a supported TLS 1.2 suite
also remains. OpenSSL aliases/operators such as `DEFAULT`, `HIGH`, exclusions,
and `@SECLEVEL=` are not approximated: the TLS connection fails instead of
silently broadening the requested policy. An explicit `ssl.security_level`
also fails because rustls has no equivalent certificate/signature/key-size
policy. `ssl.disable_compression` is satisfied when absent or true (rustls has
no TLS compression); an explicit false fails rather than pretending compression
was enabled.

## Stream contexts

| Function | Signature | Description |
|---|---|---|
| `stream_context_create()` | `stream_context_create(array $options = [], array $params = []): resource` | Create a distinct stream-context resource, retain its own `$options` hash, and make those options active for compiled wrapper/TLS consumers. A literal `['notification' => <closure>]` in `$params` is captured for HTTP notification callbacks. |
| `stream_context_get_default()` | `stream_context_get_default(array $options = []): resource` | Return the default context resource. The optional arg is evaluated for side effects; v1 does not apply it. |
| `stream_context_set_default()` | `stream_context_set_default(array $options): resource` | Return the default context resource. v1 evaluates `$options` for side effects but does not yet walk and persist the array; use `stream_context_set_option(stream_context_get_default(), ...)` when code needs options stored. |
| `stream_context_set_option()` | `stream_context_set_option(resource $context, ...): bool` | Accepts PHP's two forms: `(ctx, options_array)` replaces that resource's persisted options hash, while `(ctx, wrapper, option, value)` selects that resource, sets one nested option, and publishes any grown hash back to the same registry slot. In the four-arg form, values are stored as strings in v1. |
| `stream_context_set_params()` | `stream_context_set_params(resource $context, array $params): bool` | Captures a literal `notification` closure or first-class callable into the global notification slot and returns `true`. |
| `stream_context_get_options()` | `stream_context_get_options(resource $context): array` | Return that context resource's persisted options hash, or an empty hash when it has no options. |
| `stream_context_get_params()` | `stream_context_get_params(resource $context): array` | v1 stub: returns an empty associative array. |
| `stream_resolve_include_path()` | `stream_resolve_include_path(string $filename): string\|false` | elephc has no runtime `include_path`, so this is equivalent to `realpath($filename)`: canonical path on success, `false` otherwise. |

Active stream-context consumers:

- `fopen("http://...")` reads `http.method`, `http.header`, and `http.content`.
- `fopen("https://...")` reads the complete supported `ssl` policy above,
  including client identity, fingerprints, ALPN, ticket/SNI switches, and the
  fail-closed cipher/security/compression options.
- `fopen("ftp://...")` reads `ftp.resume_pos`.
- `file_get_contents()` over `https://` reads the same `ssl` options; over
  `ftp://` or `ftps://` it reads `ftp.resume_pos`.
- `stream_socket_server()` reads `socket.backlog`.
- `stream_socket_enable_crypto()` reads the same TLS peer, client-certificate,
  fingerprint, cipher, security, compression, ALPN, SNI, and ticket options.

Context resources keep independent option hashes. For compatibility with the
existing compiled wrapper/TLS helpers, creating a context or using the
four-argument setter also selects its hash as the active context used by a
subsequent consumer that does not yet bind its explicit `$context` argument.
Notification callbacks remain a single active global slot.

## Notification callbacks

HTTP streams can fire a context notification callback:

```php
$ctx = stream_context_create([], [
    'notification' => function (int $code, int $severity, ?string $message,
                                int $message_code, int $bytes_transferred,
                                int $bytes_max): void {
        if ($code === STREAM_NOTIFY_CONNECT)   { echo "connected\n"; }
        if ($code === STREAM_NOTIFY_COMPLETED) { echo "done\n"; }
        if ($code === STREAM_NOTIFY_FAILURE)   { echo "failed\n"; }
    },
]);

$body = fopen('http://example.com/', 'r');
```

`http://` fires `STREAM_NOTIFY_CONNECT`, `STREAM_NOTIFY_COMPLETED`, and
`STREAM_NOTIFY_FAILURE`. v1 captures only literal closure or first-class
callable entries. String function names, `[object, method]` arrays, variable
callbacks, HTTPS/FTP notifications, progress/file-size/mime/redirect/auth
milestones, `$message`, `$message_code`, and `$bytes_max` are deferred.

## Filters and buckets

| Function | Signature | Description |
|---|---|---|
| `stream_get_filters()` | `stream_get_filters(): array` | Return built-in filters: `string.toupper`, `string.tolower`, `string.rot13`, `string.strip_tags`, `convert.base64-encode`, `convert.base64-decode`, `convert.quoted-printable-encode`, `convert.quoted-printable-decode`, `convert.iconv.*`, `dechunk`, `zlib.deflate`, `zlib.inflate`, `bzip2.compress`, and `bzip2.decompress`. User filters are not enumerated in v1. |
| `stream_filter_append()` | `stream_filter_append(resource $stream, string $filtername, int $read_write = STREAM_FILTER_ALL, mixed $params = null): resource\|false` | Attach a built-in or user-registered filter. `STREAM_FILTER_READ`, `STREAM_FILTER_WRITE`, and `STREAM_FILTER_ALL` select directions. |
| `stream_filter_prepend()` | `stream_filter_prepend(resource $stream, string $filtername, int $read_write = STREAM_FILTER_ALL, mixed $params = null): resource` | Same behavior as append in v1's one-filter-per-direction model. |
| `stream_filter_remove()` | `stream_filter_remove(resource $filter): bool` | Remove the attached filter and return `true`. |
| `stream_filter_register()` | `stream_filter_register(string $filter_name, string $class): bool` | Register a user filter class. Up to 128 registrations are stored; a literal class name is validated at compile time. |
| `stream_bucket_new()` | `stream_bucket_new(resource $stream, string $data): object` | Create a stdClass-backed bucket with public `data` and `datalen` properties. |
| `stream_bucket_make_writeable()` | `stream_bucket_make_writeable(resource $brigade): object\|null` | Pop the next bucket from a brigade. |
| `stream_bucket_append()` | `stream_bucket_append(resource $brigade, object $bucket): void` | Push a bucket to the end of a brigade. |
| `stream_bucket_prepend()` | `stream_bucket_prepend(resource $brigade, object $bucket): void` | Push a bucket to the beginning of a brigade. |

`zlib.deflate` and `gzcompress()` use system `libz`. `bzip2.compress`,
`bzip2.decompress`, and `compress.bzip2://` use `libbz2`. `convert.iconv.*`
uses libc `iconv` and auto-links `-liconv` on macOS.

Compression filter params can be a bare integer or a literal array. `zlib.deflate`
reads `level` (`-1..9`). `bzip2.compress` reads `blocks` (`1..9`) and `work`
(`0..250`). Non-literal params keep defaults.

User filters can implement either `filter(string $data): string` or PHP's
four-argument `filter($in, $out, &$consumed, $closing): int` bucket form.
Classes may extend PHP's `php_user_filter` base class; the fourth
`stream_filter_append`/`prepend` `$params` argument is available as
`$this->params` before `onCreate()` runs. Optional `onCreate(): bool` and
`onClose(): void` hooks are honored. v1 seeds one input bucket per dispatch;
`PSFS_FEED_ME` does not request more input, and
`PSFS_ERR_FATAL` does not propagate as a stream error.

## User stream wrappers

| Function | Signature | Description |
|---|---|---|
| `stream_get_wrappers()` | `stream_get_wrappers(): array` | Return built-in wrappers: `file`, `php`, `data`, `ftp`, `http`, `https`, `ftps`, `compress.zlib`, `compress.bzip2`, `phar`, and `glob`. User wrappers are not enumerated in v1. |
| `stream_wrapper_register()` | `stream_wrapper_register(string $protocol, string $class, int $flags = 0): bool` | Register a userspace wrapper class for `$protocol://` URLs. Up to 16 registrations are stored. |
| `stream_wrapper_unregister()` | `stream_wrapper_unregister(string $protocol): bool` | Remove a user-registered wrapper; built-in wrappers cannot be unregistered in v1. |
| `stream_wrapper_restore()` | `stream_wrapper_restore(string $protocol): bool` | v1 no-op that reports success because built-in wrappers are always present. |

When `fopen("$protocol://...")` matches a registered wrapper, elephc creates an
instance through the runtime class registry. Declared property defaults are
applied, but `__construct` is not invoked on this path.

Supported wrapper methods include `stream_open`, `stream_read`, `stream_write`,
`stream_close`, `stream_eof`, `stream_seek`, `stream_tell`, `stream_flush`,
`stream_stat`, `stream_lock`, `stream_truncate`, `stream_metadata`,
`stream_set_option`, `stream_cast`, `url_stat`, and the directory methods
`dir_opendir`, `dir_readdir`, `dir_rewinddir`, and `dir_closedir`.

`url_stat` is consulted by the whole stat family — `stat()`, `lstat()`,
`file_exists()`, `filesize()`, `filemtime()`, `is_file()`, `is_dir()`,
`is_readable()`, `is_writable()`, `is_writeable()`, and `is_executable()` — each
handing the wrapper the same `STREAM_URL_STAT_*` flag values reference PHP
passes. See [System & I/O](system-and-io.md) for the per-builtin semantics.

Wrapper methods should declare return types that match their PHP contracts.
`stream_stat()` and `url_stat()` are exceptions: declare them without a return
type, or as `mixed`, when returning associative stat arrays with string keys.

## Sockets and process streams

| Function | Signature | Description |
|---|---|---|
| `stream_get_transports()` | `stream_get_transports(): array` | Return recognized socket transports: `tcp`, `udp`, `tls`, `ssl`, `tlsv1.0`, `tlsv1.1`, `tlsv1.2`, and `tlsv1.3`, plus `unix` and `udg` on non-Windows targets. SSLv2/SSLv3 are omitted like a modern PHP/OpenSSL build; TLS-version names use the rustls-supported policy. |
| `stream_socket_server()` | `stream_socket_server($address): resource\|false` | Bind a server socket for `[tcp://]host:port` or `udp://host:port`, and on non-Windows targets `unix:///path` or `udg:///path`. TCP and Unix-stream sockets listen; UDP and Unix-datagram sockets only bind. PHP does not register Unix-domain transports on Windows, so those addresses return `false` there. |
| `stream_socket_client()` | `stream_socket_client($address): resource\|false` | Open a client stream for `[tcp://]host:port` or `udp://host:port`, and on non-Windows targets `unix:///path` or `udg:///path`. PHP does not register Unix-domain transports on Windows, so those addresses return `false` there. |
| `stream_socket_accept()` | `stream_socket_accept($socket): resource\|false` | Accept the next pending connection from a listening stream. |
| `stream_socket_enable_crypto()` | `stream_socket_enable_crypto(resource $stream, bool $enable, int $crypto_method = null, resource $session_stream = null): bool` | Attach TLS to an already-connected TCP fd. `$enable=false` unwinds the session (sends `close_notify` and clears the per-fd TLS handle), leaving the fd a plain TCP socket, then reports `true`; it is a no-op when no session is attached. |
| `fsockopen()` | `fsockopen(string $hostname, int $port, int &$error_code = null, string &$error_message = null, float $timeout = null): resource\|false` | Open a TCP connection to `$hostname:$port`, writing optional by-reference error outputs. The timeout arg is evaluated but the OS default connect timeout is used in v1. |
| `pfsockopen()` | `pfsockopen(string $hostname, int $port, int &$error_code = null, string &$error_message = null, float $timeout = null): resource\|false` | Alias of `fsockopen()`; persistent connections are not meaningful for standalone native binaries. |
| `stream_set_blocking()` | `stream_set_blocking($stream, bool $enable): bool` | Toggle `O_NONBLOCK`. Non-blocking read misses return an empty `fread()` result or `false` from `fgetc()`/`fgets()` without setting EOF. User wrappers route through `stream_set_option(STREAM_OPTION_BLOCKING, ...)`. |
| `stream_set_timeout()` | `stream_set_timeout($stream, int $seconds, int $microseconds = 0): bool` | Set `SO_RCVTIMEO` on socket streams. User wrappers route through `stream_set_option(STREAM_OPTION_READ_TIMEOUT, ...)`. |
| `stream_select()` | `stream_select(array &$read, array &$write, array &$except, int $seconds, int $microseconds = 0): int` | Wait until stream arrays are ready and rewrite each array to its ready subset. Word-0 only: descriptors must be `0..63`. User wrappers are selectable when `stream_cast(STREAM_CAST_FOR_SELECT)` returns a real stream resource. |
| `stream_socket_shutdown()` | `stream_socket_shutdown($stream, int $mode): bool` | Shut down socket reads (`0`), writes (`1`), or both (`2`). |
| `stream_socket_sendto()` | `stream_socket_sendto($socket, string $data, int $flags = 0, string $address = ""): int\|false` | Send bytes to the connected peer or to an explicit datagram address. |
| `stream_socket_recvfrom()` | `stream_socket_recvfrom($socket, int $length, int $flags = 0, string &$address = ""): string\|false` | Receive bytes and optionally write back the sender address as `host:port`. |
| `stream_socket_get_name()` | `stream_socket_get_name($socket, bool $remote): string\|false` | Return local or remote socket name as `host:port`. |
| `stream_socket_pair()` | `stream_socket_pair(int $domain, int $type, int $protocol): array` | Create a pair of connected socket streams, for example `STREAM_PF_UNIX`, `STREAM_SOCK_STREAM`, `0`. |
| `popen()` | `popen(string $command, string $mode): resource\|false` | Open a pipe to a process in read (`"r"`) or write (`"w"`) mode. |
| `pclose()` | `pclose($handle): int` | Close a process pipe and return its termination status. |

Socket addresses use `[tcp://]host:port` or `udp://host:port`. Non-Windows
targets also support `unix:///path` and `udg:///path`; matching php-src,
Windows neither reports nor accepts these Unix-domain transports. Host names
are resolved through the system resolver to IPv4.

## Directory streams

Directory handles are stream resources too. `opendir()`, `readdir()`,
`rewinddir()`, and `closedir()` are documented with filesystem functions in
[System & I/O](system-and-io.md). Registered userspace wrappers can implement
`dir_opendir`, `dir_readdir`, `dir_rewinddir`, and `dir_closedir`; the `glob://`
wrapper exposes glob matches through the same directory-stream API.

## Stream metadata and introspection

| Function | Signature | Description |
|---|---|---|
| `get_resource_type()` | `get_resource_type(resource $handle): string` | Return `"stream"` for every OPEN resource elephc produces, and `"Unknown"` once the handle has been closed. |
| `get_resource_id()` | `get_resource_id(resource $handle): int` | Return the numeric id shown in `Resource id #N`. |
| `stream_isatty()` | `stream_isatty(resource $stream): bool` | Report whether the stream is connected to an interactive terminal. |
| `stream_is_local()` | `stream_is_local(resource\|string $stream): bool` | Return `true` for local streams. |
| `stream_supports_lock()` | `stream_supports_lock(resource $stream): bool` | Return `true` when a stream supports `flock()`. |
| `stream_get_meta_data()` | `stream_get_meta_data(resource $stream): array` | Return metadata keys `timed_out`, `blocked`, `eof`, `unread_bytes`, `stream_type`, `wrapper_type`, `mode`, `seekable`, and `uri`. |

Closing a handle changes its reported type but not its id. After `fclose()`,
`pclose()` or `closedir()`, `get_resource_type($handle)` returns `"Unknown"` and
`var_dump($handle)` prints `resource(N) of type (Unknown)` — matching PHP 8.5.6,
which renames every closed resource that way regardless of what it was. The id
`N` is unchanged, `get_resource_id()` still answers it, and `"$handle"` still
renders `Resource id #N`, because php-src leaves `zend_resource.handle` alone on
close. elephc still reports the single open type name `"stream"`; PHP's further
names (`"stream-context"`, `"stream filter"`) are not distinguished yet.

`stream_get_meta_data()` derives `eof`, `seekable`, `blocked`, and `mode` from
the live descriptor. `stream_type` is `"STDIO"` for seekable streams and
`"tcp_socket"` for non-seekable streams. `wrapper_type` is reported as
`"plainfile"` and `uri` as the empty string because elephc does not track
per-resource open paths in v1.
