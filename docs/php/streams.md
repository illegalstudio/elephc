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

### Resource ids

Ids follow PHP's reference numbering:

| id | holder |
|---|---|
| 1, 2, 3 | `STDIN`, `STDOUT`, `STDERR` |
| 4 | the request's default stream context |
| 5 and up | resources the program opens |

Id 4 is not reserved by the runtime at startup: PHP creates the default stream
context lazily, at the **first stream open of any kind**, and keeps it for the
rest of the request. `stream_context_get_default()` therefore reports a LOWER id
than a stream opened before it, and a program that never opens a stream never
mints it.

Ids are never reused. Closing a handle and opening another gives the new one the
next number, even when the operating system hands back the same descriptor.
`var_dump()`, `get_resource_id()`, an `(int)` cast, and `"$handle"` all report the
same id, and streams opened inside `eval()` draw from the same counter, so a mixed
program numbers its resources exactly as PHP does.

A `php://stdout` (or `php://stdin`, `php://stderr`, `php://fd/N`) handle is a
DUPLICATE of the descriptor, as in php-src, so `fclose()` on it closes only that
copy and the program's own output keeps working.

The table backing these ids costs no runtime heap: its first slots are reserved
statically, so a program compiled with a small `--heap-size` still starts, and one
that never opens a resource reports `allocs=0` under `--heap-debug`. Opening more
resources than the initial reservation holds grows the table on the heap as usual.

## Basic stream I/O

| Function | Signature | Description |
|---|---|---|
| `fopen()` | `fopen($filename, $mode, $use_include_path = false, $context = null): resource\|false` | Open a file, wrapper URL, socket-like wrapper, or temporary/memory stream. PHP's modes are supported: `r`, `w`, `a`, `c` and `x`, each with an optional `+`, and the `b`/`t` flags anywhere in the string. The optional args are evaluated in source order. `fopen()`, `file_get_contents()` and `readfile()` each publish their own `$context` for the duration of the call and restore the previous one afterwards. |
| `fclose()` | `fclose(resource $handle): bool` | Close a stream. Closing a `phar://` write stream finalizes the archive, and closing a filtered stream runs pending filter cleanup such as user-filter `onClose()`. |
| `fread()` | `fread(resource $handle, $length): string` | Read up to `$length` bytes. Attached read filters and user-wrapper `stream_read()` methods are honored. On a filtered stream the result is capped at `$length` and the filter's remainder is kept for the next read. |
| `fwrite()` | `fwrite(resource $handle, string $data, ?int $length = null): int\|false` | Write bytes and return the byte count. `$length` caps the write at `max(0, min($length, strlen($data)))`; a non-positive cap writes nothing and returns `0` without raising, and `null` (or an omitted argument) writes everything. Attached write filters and user-wrapper `stream_write()` methods are honored, and a filter sees only the capped bytes. |
| `fputs()` | `fputs(resource $handle, string $data, ?int $length = null): int\|false` | Alias of `fwrite()`, third argument included. |
| `fprintf()` | `fprintf(resource $handle, string $format, ...$values): int` | Format like `sprintf()` and write to the stream. |
| `vfprintf()` | `vfprintf(resource $handle, string $format, array $values): int` | Like `fprintf()`, with format values supplied as an array. |
| `fscanf()` | `fscanf(resource $handle, string $format): array\|false\|null` | Read one line — newline included, as `php_stream_get_line()` does — and parse it with the `sscanf()` engine, which implements php's whole scanner: widths, `*` suppression, `%[...]` character classes, `%c`/`%d`/`%D`/`%e`/`%E`/`%f`/`%g`/`%i`/`%n`/`%o`/`%s`/`%u`/`%x`/`%X`, the `l`/`h`/`L` modifiers, and php's `ValueError` wording for a format it refuses. End of file is `false`; an empty line is `null`, since scanning `"\n"` reaches end of input without assigning. php's by-ref `...$vars` form is supported for up to eight variables: it assigns each field through the reference and answers the number of conversions that consumed input — the SUPPRESSED ones included, so `%d %*d %d` over `1 2 3` answers 3 while filling two variables — or `-1` when input ran out before any conversion succeeded, or `false` for a stream already at end of file. More variables than the format has conversions raises `ValueError: Variable is not assigned by any conversion specifiers`, fewer raises `Different numbers of variable names and field specifiers`, and more than eight is refused at compile time. |
| `fgets()` | `fgets(resource $handle, int $length = null): string\|false` | Read a line until newline, EOF, or `$length`; returns `false` at EOF. `$length` bounds the line at `$length - 1` bytes and leaves the remainder for the next read; a non-positive `$length` raises `ValueError`. |
| `fgetc()` | `fgetc(resource $handle): string\|false` | Read one byte, or `false` at EOF/failure. |
| `feof()` | `feof(resource $handle): bool` | Report whether the stream is at EOF. |
| `fseek()` | `fseek(resource $handle, $offset [, $whence]): int` | Seek a stream. User wrappers route through `stream_seek()`. |
| `ftell()` | `ftell(resource $handle): int` | Return the current stream position. User wrappers route through `stream_tell()`. |
| `rewind()` | `rewind(resource $handle): bool` | Seek to the start of the stream. |
| `fgetcsv()` | `fgetcsv(resource $handle, ?int $length = null, string $separator = ',', string $enclosure = '"', string $escape = ''): array` | Read and parse one CSV line. Custom separator, enclosure, and escape (PHP 8.4 default `''` = RFC 4180 doubling mode) are honored. User wrappers are read through `stream_read()`. |
| `fputcsv()` | `fputcsv(resource $handle, array $fields, string $separator = ',', string $enclosure = '"', string $escape = '\\', string $eol = "\n"): int` | Format and write one CSV line with custom separator, enclosure, escape, and end-of-line (PHP 8.1+ `eol`). User wrappers are written through `stream_write()`. |
| `readline()` | `readline([$prompt]): string` | Read a line from standard input. |
| `readfile()` | `readfile($filename, bool $use_include_path = false, $context = null): int\|false` | Open a path or wrapper URL, stream it to stdout, and return copied bytes; returns `false` when open fails. Remote URLs are read through the same wrapper `file_get_contents()` uses, and `$context` is honored. |
| `fpassthru()` | `fpassthru(resource $handle): int` | Stream the remaining bytes of an open handle to stdout, returning `-1` on read failure. |
| `stream_get_contents()` | `stream_get_contents(resource $handle, ?int $length = null, int $offset = -1): string\|false` | Read remaining bytes from the stream. `$offset >= 0` seeks there first (seekable streams / user wrappers via `stream_seek()`) and returns `false` if that seek fails; a finite `$length` reads at most that many bytes (a `null`/negative `$length` reads to EOF). The bounded form loops through `fread` until it fills `$length`, reaches EOF, or receives an empty read. |
| `stream_copy_to_stream()` | `stream_copy_to_stream(resource $from, resource $to, ?int $length = null, int $offset = -1): int\|false` | Copy bytes from one stream to another, returning the count. `$offset >= 0` seeks the source first (seekable streams / user wrappers via `stream_seek()`) and returns `false` if that seek fails; a finite `$length` copies at most that many bytes (a `null`/negative `$length` copies to EOF). The bounded form drives a chunked read/write loop and clamps wrapper chunks that exceed the requested count. |
| `stream_get_line()` | `stream_get_line(resource $handle, int $length [, string $ending]): string` | Read up to `$length` bytes, stopping at and consuming `$ending` when supplied. |
| `flock()` | `flock(resource $handle, int $op, int &$would_block = null): bool` | Advisory locking. `LOCK_SH`, `LOCK_EX`, `LOCK_UN`, and `LOCK_NB` are supported; user wrappers route through `stream_lock(int $operation)`. `&$would_block` may be passed undeclared, as in PHP — the call defines it as `int`. |
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

`fopen()` resolves a wrapper scheme from a literal path at compile time, and from the bytes when
the path is built at run time — `php://` (including `php://filter`), `data://` and `http://` —
so the common shape works:

```php
function readIt(string $path) { return fopen($path, "r"); }   // "php://memory" opens
```

A run-time `php://filter/...` URL opens the resource it names and attaches the filter afterwards,
so the resource may itself be any scheme — `resource=php://temp` works. As with the literal form,
only the first filter of a `|`-separated list is applied, an unrecognised filter name opens the
resource unfiltered, and a resource that is itself a filter URL is refused.

`data://` validates its media type as php-src does: the type is empty or carries a `/`, every
parameter is `name=value` whatever the name, and `base64` counts only as the LAST parameter and
only in lower case. So `data://text,…`, `data://text/plain;,…` and
`data://text/plain;base64;charset=utf-8,…` all answer `false`, while `;bogus=1` is accepted and
`;charset=utf-8;base64` decodes.

| Wrapper | Description |
|---|---|
| `file` | Normal filesystem streams. |
| `php://stdin`, `php://stdout`, `php://stderr` | Standard descriptors 0, 1, and 2. `php://input` aliases stdin, and `php://output` aliases stdout. |
| `php://memory`, `php://temp` | Seekable in-memory streams backed by an anonymous temporary buffer. `php://temp/maxmemory:N` is accepted and ignored. |
| `php://filter` | Opens an underlying resource and attaches one built-in filter at open time, for example `php://filter/read=string.toupper/resource=php://temp`. The resource may be any path or wrapper URL, and the URL may be built at run time. |
| `data://` | RFC 2397 inline payload streams. Base64 and percent-decoded payloads are supported, and the URI may be built at run time. |
| `phar://` | Read or write PHAR entries. Literal reads happen at compile time and embed the entry in the binary; non-literal reads happen at runtime. Native PHAR, tar-based PHAR, and zip-based PHAR containers are readable; native PHAR gzip/bzip2 entries and ZIP deflate entries are decoded transparently. |
| `ftp://` | Anonymous binary passive FTP read streams. `fopen()` requires a literal URL; `file_get_contents()` also accepts runtime string URLs. Credentials in the URL are ignored in v1. |
| `ftps://` | Explicit FTP over TLS using `AUTH TLS`, with TLS on both control and data channels. `fopen()` requires a literal URL; `file_get_contents()` also accepts runtime string URLs. |
| `http://` | HTTP read streams. `fopen()` requires a literal URL; `file_get_contents()` also accepts runtime string URLs. Redirects are followed as PHP does — `follow_location` defaults to on and `max_redirects` to 20 — and a response buffers up to 1 MiB. |
| `https://` | Same as `http://`, but over TLS through the `elephc-tls` static library. Programs using it auto-link `-lelephc_tls`; programs that do not use TLS pay no extra link cost. |
| `compress.zlib://` | Opens the underlying file and applies `zlib.inflate` when reading or `zlib.deflate` when writing, so `fopen("compress.zlib://out.gz", "w")` compresses. The URL may be a literal or built at run time — both reach the wrapper and produce the same bytes. |
| `compress.bzip2://` | Opens the underlying file and decompresses it through libbz2 when reading, or compresses through it when writing, so `fopen("compress.bzip2://out.bz2", "w")` writes a real bzip2 stream. php exposes no context option for the block size, so the wrapper uses its defaults — block size 9, work factor 0. The URL may be a literal or built at run time. |
| `glob://` | Directory-style wrapper for iterating paths matching a glob pattern through `opendir()` / `readdir()`. |
| `zip://` | Read-only wrapper for one entry of a plain ZIP archive, addressed as `zip://archive.zip#entry`. The archive is read when the program runs — even for a literal URL — so an archive the program just wrote is visible. Stored and deflated entries are decoded, including ZIP64; the entry name is matched EXACTLY after the first `#`, with no leading-slash stripping and no directory resolution. Every failure — missing archive, missing entry, no `#`, an encrypted entry — reports PHP's one wording, `Failed to open stream: operation failed`. |

`zip://` is read-only in PHP: a write mode is refused with the same failed-open
line rather than falling through to the filesystem. The stream reports
`wrapper_type` `zip wrapper` and `stream_type` `zip`, and — like PHP, whose
`ext/zip` stream ops define no `seek` — it is not seekable: `fseek()` warns
`Stream does not support seeking` and returns `-1`, `rewind()` warns and returns
`false`, and the read position does not move. `zip://` entries are decoded by the
same `elephc-phar` bridge that reads zip-based PHAR containers, so a program that
reads one links `-lelephc_phar` and nothing else.

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
confidentiality mechanism. `Phar` and `PharData` expose a
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

Metadata persistence covers the same scalar+array subset as
[`serialize()`/`unserialize()`](system-and-io.md#serialization); object metadata is not
serialized.

### `ZipArchive` (reading)

`ZipArchive` reads plain ZIP archives through the same bridge the `zip://` wrapper
uses. It implements `Countable`, publishes every `ZipArchive::*` constant PHP does
(the open flags, the `ER_*` error codes, the `FL_*` lookup flags, the `CM_*`
compression methods and the `EM_*` encryption methods), and exposes `filename`,
`numFiles`, `status`, `statusSys`, `comment` and `lastId` as readable properties.

| Member | Behaviour |
|---|---|
| `open($filename, $flags = 0)` | `true` on success. `ZipArchive::ER_NOENT` for a missing archive without `CREATE`/`OVERWRITE`, `ER_EXISTS` when `EXCL` meets an existing file, `ER_NOZIP` for a file that is not a ZIP. An empty `$filename` throws `ValueError`, as in PHP. |
| `close()` | `true`; `numFiles` returns to `0` and `filename` to `""`. An archive opened with `OVERWRITE` and closed without additions is REMOVED, which is what libzip does with an archive that would hold nothing. |
| `count()`, `numFiles` | The entry count, directory members included. |
| `getNameIndex()`, `locateName()` | The stored name / its index, or `false`. `ZipArchive::FL_NOCASE` matches case-insensitively. |
| `statIndex()`, `statName()` | PHP's eight keys in PHP's order — `name`, `index`, `crc`, `size`, `mtime`, `comp_size`, `comp_method`, `encryption_method` — or `false`. `mtime` is built from the entry's MS-DOS date/time in the PROCESS timezone, exactly as libzip's `mktime()` does. |
| `getFromName()`, `getFromIndex()` | The entry bytes, or `false`. |
| `getStream()`, `getStreamName()`, `getStreamIndex()` | A readable stream over the entry, or `false`. |
| `setPassword()` | Arms the ZipCrypto password, so an encrypted entry becomes readable — the same password `PharData::setZipPassword()` uses. |
| `extractTo($directory, $files = null)` | Extracts the whole archive, one named entry, or a list of them. `false` when the destination cannot be created, when a requested entry does not exist, or for an empty destination/selection. Existing files are overwritten, each extracted file carries the ENTRY's mtime, and a directory member becomes a directory. |

Every accessor is SILENT on failure: only `open()` reports anything, and it does so
through its return value. That matches PHP, where `getFromName("nope")` answers a
bare `false` while `file_get_contents("zip://…#nope")` warns.

`extractTo()` cannot write outside the destination. PHP does not reject a hostile
entry name, it normalizes it, and elephc reproduces that walk exactly: a `/`-split
where an empty or `.` segment is dropped and a whole `..` segment pops one segment
(popping nothing when there is nothing to pop). So `../up.txt` extracts to
`up.txt`, `a/b/../c.txt` to `a/c.txt`, `a/b/../../../d.txt` to `d.txt`, and
`/abs.txt` to `abs.txt` — all inside the destination. Only a whole `..` segment
counts (`f..g.txt` and `a/..b/h.txt` are ordinary names) and a backslash is not a
separator.

Archive MUTATION is not implemented: `addFile()`, `addFromString()`,
`deleteName()`, `deleteIndex()`, `renameName()`, `renameIndex()`,
`setArchiveComment()`, `setCompressionName()` and the rest of the write API are
absent rather than present and inert, so a program that needs them fails to
compile instead of silently doing nothing. The single write PHP performs on the
supported path IS implemented, because leaving it out would diverge silently: an
archive opened with `OVERWRITE` and closed without additions is removed from disk.

`file_get_contents($url)` recognizes runtime `http://`, `https://`, `ftp://`,
and `ftps://` strings before falling back to `phar://`/filesystem handling.
Because the scheme is not known statically, non-literal `file_get_contents()`
conservatively links `elephc-tls`, `elephc-phar`, zlib, and libbz2.

`https://`, `ftps://`, and `stream_socket_enable_crypto()` use `elephc-tls`
(rustls, the `ring` crypto provider, and Mozilla webpki roots). TLS contexts can
override trust with `ssl.cafile` or `ssl.capath`, set `ssl.peer_name`, or relax
verification with `ssl.verify_peer = "0"`, `ssl.allow_self_signed`, or
`ssl.verify_peer_name = "0"`. Client certificates are supported when both
`ssl.local_cert` and `ssl.local_pk` point at readable PEM files; encrypted keys
and `ssl.passphrase` are not supported.

`ssl.peer_fingerprint` pins the peer's leaf certificate. It is checked after the
handshake, against the certificate the peer actually presented, so it composes
with every trust setting above — relaxing chain verification with
`verify_peer => "0"` does not relax the pin. As in PHP, a BARE hexadecimal string
is matched by its LENGTH: 32 characters mean MD5 and 40 mean SHA-1, the
comparison is case-insensitive, and any other length is a mismatch (a 64-character
SHA-256 hex string written bare fails in PHP too — that digest is spelled through
the array form). A mismatch prints `Warning: peer_fingerprint match failure` and
the open returns `false`. Two divergences from PHP: elephc's runtime does not know
which builtin is on the stack, so the `<callee>(): ` prefix PHP puts in front of
that sentence is missing, and the array form `['sha256' => '…']` is not read yet —
only the bare-string spelling is.

### `ssl` context options rustls cannot honour

elephc's TLS is rustls, not OpenSSL, so part of PHP's `ssl` context surface has
no equivalent. Every option below is **accepted and ignored** — that is a
deliberate choice, not an oversight, and the reason differs per group. PHP itself
accepts unknown `ssl` options silently, so the accept-and-ignore shape is what a
program sees either way; what changes is whether the option does anything.

Options PHP itself accepts without any observable effect on a TLS 1.3 client
connection (measured on `php -n` 8.5.6 against a public TLS 1.3 endpoint: each of
them completes the request unchanged, including a deliberately invalid value):

| Option | Why it is inert in PHP too |
|---|---|
| `disable_compression` | TLS compression is gone; TLS 1.3 has no compression to disable. |
| `no_ticket` | Session-ticket policy, not part of a single-shot client handshake. |
| `reneg_limit`, `reneg_window`, `reneg_limit_callback` | Renegotiation was removed in TLS 1.3; rustls never implemented it. |
| `dh_param`, `single_dh_use`, `ecdh_curve`, `rsa_key_size` | Server-side key-exchange parameters. `ecdh_curve => 'not-a-curve'` still connects. |
| `honor_cipher_order` | A server preference. |
| `passphrase` | Decrypts an encrypted `local_pk`; `rustls-pemfile` reads unencrypted PEM keys only, so an encrypted key fails the connect instead. |
| `ciphers`, `security_level` | rustls does not consume OpenSSL cipher-list strings and picks its TLS 1.2/1.3 policy internally. |

Options PHP **does** enforce and elephc does not. These are real behavioural
gaps: a program that relies on them is less strict under elephc than under PHP.

| Option | Measured PHP behaviour | Status in elephc |
|---|---|---|
| `peer_fingerprint` (array form) | The array form `['sha256' => '…']` is matched case-insensitively, and is how SHA-256 is spelled. | Only the bare-string form is read; the array form is ignored. See above. |
| `SNI_enabled` | Enforced. `false` suppresses SNI, and a host that requires SNI then fails the handshake. | Not checked; rustls `ClientConfig::enable_sni` would express it. |
| `verify_depth` | Enforced. `verify_depth => 1` refuses a two-link chain. | Not checked; rustls' webpki verifier has a fixed internal path budget it does not expose. |
| `min_proto_version`, `max_proto_version` | Enforced. `max_proto_version => STREAM_CRYPTO_PROTO_TLSv1_1` refuses a TLS 1.3 peer; `min_proto_version => STREAM_CRYPTO_PROTO_TLSv1_3` lets one through. | Not checked. rustls supports TLS 1.2 and 1.3 only, so a TLS 1.0/1.1 bound has no meaning; the 1.2/1.3 pair is expressible through `builder_with_protocol_versions`. |
| `crypto_method` | Selects the method for `stream_socket_enable_crypto()`. On the `https://` wrapper it had no measured effect (`STREAM_CRYPTO_METHOD_TLSv1_2_CLIENT` still completed a TLS 1.3 request). | Not read. |
| `alpn_protocols` | Sent on the wire; the negotiated protocol surfaces in `stream_get_meta_data()['crypto']['alpn_protocol']`. | Not sent. rustls supports ALPN natively, but elephc's `stream_get_meta_data()` has no `crypto` key to report it through. |
| `capture_peer_cert`, `capture_peer_cert_chain`, `peer_certificate`, `peer_certificate_chain` | The certificate is written back into the context as an `OpenSSLCertificate` object. | Not captured. rustls exposes the peer chain as DER, but elephc has no `OpenSSLCertificate` value to hand back. |

### `http` and `ftp` context options

`http.auto_decode` is **not** an `http` wrapper option in PHP. Measured on
`php -n` 8.5.6 against a local server that answers `Content-Encoding: gzip`, the
body comes back still gzip-compressed with the option set to `true`, to `false`,
to `1`, and with the option absent — PHP's `http` wrapper never decodes a
compressed response. elephc behaves the same way, so no work is owed here.

`ftp.overwrite` governs FTP *writes*. elephc's `ftp://` wrapper is read-only
(`RETR` only), so the option has nothing to act on and is ignored.

`ftp.proxy` **is** honoured by PHP: with `['ftp' => ['proxy' => 'tcp://host:port']]`
the connect target changes to the proxy (measured — the failure moves from
`operation failed` to `Connection refused` at the proxy address). elephc ignores
it and always connects to the URL's host. Only `ftp.resume_pos` is read.

## Stream contexts

| Function | Signature | Description |
|---|---|---|
| `stream_context_create()` | `stream_context_create(array $options = [], array $params = []): resource` | Create a stream-context resource and persist `$options` in the single global context slot. A literal `['notification' => <closure>]` in `$params` is captured for HTTP notification callbacks. |
| `stream_context_get_default()` | `stream_context_get_default(array $options = []): resource` | Return the default context resource. The optional arg is evaluated for side effects; v1 does not apply it. |
| `stream_context_set_default()` | `stream_context_set_default(array $options): resource` | Merge `$options` into the request's default context and return it. Later opens without an explicit context read them. |
| `stream_context_set_option()` | `stream_context_set_option(resource $context, ...): bool` | Accepts PHP's two forms: `(ctx, options_array)` replaces the persisted options hash, while `(ctx, wrapper, option, value)` sets one nested option. In the four-arg form, values are stored as strings in v1. |
| `stream_context_set_options()` | `stream_context_set_options(resource $context, array $options): bool` | The two-argument spelling PHP 8.3 added for the array form above. |
| `stream_context_set_params()` | `stream_context_set_params(resource $context, array $params): bool` | Captures a literal `notification` closure or first-class callable into the global notification slot and returns `true`. |
| `stream_context_get_options()` | `stream_context_get_options(resource $context): array` | Return the persisted options hash, or an empty hash when no context has been created. |
| `stream_context_get_params()` | `stream_context_get_params(resource $context): array` | Return the context's `options` (and `notification` when one is set). |
| `stream_resolve_include_path()` | `stream_resolve_include_path(string $filename): string\|false` | elephc has no runtime `include_path`, so this is equivalent to `realpath($filename)`: canonical path on success, `false` otherwise. |

Active stream-context consumers:

- `fopen("http://...")` reads `http.method`, `http.header`, `http.content`,
  `http.user_agent`, `http.protocol_version`, `http.request_fulluri`,
  `http.ignore_errors`, `http.proxy`, `http.follow_location`,
  `http.max_redirects`, and `http.timeout`. `http.timeout` follows PHP's
  documented FLOAT contract: `2`, `2.5`, `"2.5"` and `true` are all read, the
  sub-second part survives as microseconds, `0` fails the open immediately, and a
  negative value means "wait forever".
- `fopen("https://...")` reads the `ssl` trust and peer-name options, plus
  `ssl.peer_fingerprint`.
- `fopen("ftp://...")` reads `ftp.resume_pos`.
- `file_get_contents()` over `https://` reads the same `ssl` options; over
  `ftp://` or `ftps://` it reads `ftp.resume_pos`.
- `stream_socket_server()` reads `socket.backlog`.
- `stream_socket_enable_crypto()` reads TLS peer and client-certificate options.
  `ssl.peer_name` becomes the SNI the handshake sends and the name the certificate
  is checked against; without it the connection host is used, and a host that is an
  IP address means no SNI is sent at all. `ssl.peer_fingerprint` is read by the
  `https://` wrapper only — an fd promoted to TLS through
  `stream_socket_enable_crypto()` is not pinned.

Contexts are independent values: creating or modifying one does not disturb another.
`fopen()`, `file_get_contents()` and `readfile()` publish their own `$context` for the
duration of the call and restore the previous one, so a context passed to one call
cannot leak into the next.

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
| `stream_get_filters()` | `stream_get_filters(): array` | Return PHP's built-in filter families in PHP's own order — `zlib.*`, `bzip2.*`, `convert.iconv.*`, `string.rot13`, `string.toupper`, `string.tolower`, `convert.*`, `consumed`, `dechunk` — followed by every name `stream_filter_register()` added. `string.strip_tags` is not listed and cannot be attached: PHP removed that filter in 8.0. |
| `stream_filter_append()` | `stream_filter_append(resource $stream, string $filter_name, int $mode = 0, mixed $params = null): resource\|false` | Attach a built-in or user-registered filter. `STREAM_FILTER_READ`, `STREAM_FILTER_WRITE`, and `STREAM_FILTER_ALL` select directions. The default `0` (and any mode with no direction bit set) is resolved from the stream's own open mode, as PHP does: `r` → read, `w`/`a` → write, any `+` mode → both. An unknown filter name warns `Unable to locate filter "…"` and returns `false`. |
| `stream_filter_prepend()` | `stream_filter_prepend(resource $stream, string $filter_name, int $mode = 0, mixed $params = null): resource\|false` | Same rules as append; the node joins the head of each selected chain. |
| `stream_filter_remove()` | `stream_filter_remove(resource $filter): bool` | Detach the filter from its chain. The filter is flushed first with `$closing = true`; if that flush answers `PSFS_ERR_FATAL`, the filter stays attached and the call returns `false`. |
| `stream_filter_register()` | `stream_filter_register(string $filter_name, string $class): bool` | Register a user filter class. Answers `false` — without replacing anything — for a name that is already taken, whether PHP owns it (`string.toupper`, `string.tolower`, `string.rot13`, `dechunk`, `consumed`) or an earlier call registered it; the `convert.*`, `zlib.*` and `bzip2.*` families are wildcard factories in PHP, so those names ARE registrable. An empty `$filter_name` or `$class` raises PHP's `ValueError`. Up to 128 registrations are stored, and the name is copied rather than borrowed. The class may be named by a string literal or by `Foo::class`; a class name only known at run time registers the name but leaves the class unusable. A literal class name is validated at compile time. |
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

`convert.quoted-printable-encode` implements PHP's DEFAULT rules: `=` becomes
`=3D`, bytes outside 33..126 become `=XX`, and a SPACE or TAB stays literal, so
`a b=c d` encodes to `a b=3Dc d`. Its `binary`, `line-length` and
`line-break-chars` parameters are **not** honoured — the filter always applies
the default rules, so a call that asks for binary mode still passes whitespace
through, and one that asks for a line length does not soft-wrap.
`convert.base64-encode` likewise ignores `line-length` and never wraps.

A filter name held in a variable resolves against the string, conversion and
`consumed` filters, against user-registered names, and against `zlib.*`,
`bzip2.*` and `convert.iconv.*` — `$n = "zlib.deflate";
stream_filter_append($s, $n)` compresses exactly as the literal spelling does.
Those five are not table entries: each installs a per-fd handle and a
program-local helper thunk, so the call site emits the attach sequences and picks
between them by comparing the run-time name. For `convert.iconv.*` the charset
pair is split out of the name at run time into program-local buffers, which is
what a literal splits during compilation.

A `convert.iconv.` name with no `/` is not a filter — `convert.iconv.` and
`convert.iconv.UTF-8` return `false` and warn `Unable to create or locate filter`,
PHP's wording for a name a factory claimed and then refused. A charset pair
`iconv_open()` cannot open, such as `convert.iconv.nope/alsonope`, is refused the
same way, at attach time as PHP refuses it. An EMPTY half is accepted:
`convert.iconv.UTF-8/` and `convert.iconv./UTF-8` both attach, iconv reading the
empty string as the current locale's charset.

`consumed` counts the bytes it passes and forwards every one of them, as PHP's
filter does. PHP additionally rewinds the stream when the filter chain closes,
which discards the bytes PHP had read ahead into its own buffer — so PHP's
`fread($s, 100)` on an 11-byte file returns `""` where elephc returns the 11
bytes.

User filters can implement either `filter(string $data): string` or PHP's
four-argument `filter($in, $out, &$consumed, $closing): int` bucket form.
Classes may extend PHP's `php_user_filter` base class; the fourth
`stream_filter_append`/`prepend` `$params` argument is available as
`$this->params` before `onCreate()` runs, and `$this->filtername` carries the name the
filter was ATTACHED under — the same class registered under two names reports each in
turn, as PHP does. The base class also declares `$stream`, so the manual's
`stream_bucket_new($this->stream, …)` idiom compiles, but it is **not** seeded with a
value: PHP sets it for the duration of each `filter()` call (and leaves it null in
`onCreate()`/`onClose()`), while elephc leaves it null throughout. Optional `onCreate(): bool` and
`onClose(): void` hooks are honored; `onClose()` fires exactly once, whether the
filter is removed with `stream_filter_remove()` or carried off by `fclose()`.
`$closing` is `false` on read and write dispatches and `true` on the single
closing flush a removal performs. `PSFS_ERR_FATAL` cancels a removal but does not
otherwise propagate as a stream error.

`PSFS_FEED_ME` means the filter has taken the input and has no output yet, so the read
returns nothing and fetches more input to dispatch again. A filter that buffers across
dispatches therefore behaves as in PHP: reading `abcdefghi` in three-byte chunks through
a filter that accumulates six bytes before emitting yields `ABC`, `DEF`, `GHI`.

Filtered reads are buffered on the stream, which is what makes that work. A read filter
does not emit one byte per byte consumed, so `fread($h, $n)` caps the result at `$n` and
keeps the remainder for the next read — a filter that triples `"ab"` answers three
`fread($f, 2)` calls with `ab`, `ab`, `ab`. Reaching the end of the input gives the chain
one final `filter(..., $closing = true)` dispatch, and whatever it emits reaches the
reader, so a filter still holding bytes when the stream ends flushes them. `feof()` stays
false while any of that output is still owed, and seeking discards it: the new pass earns
its own closing dispatch.

## User stream wrappers

| Function | Signature | Description |
|---|---|---|
| `stream_get_wrappers()` | `stream_get_wrappers(): array` | Return built-in wrappers in PHP's registration order: `https`, `ftps`, `compress.zlib`, `compress.bzip2`, `php`, `file`, `glob`, `data`, `http`, and `ftp`, then `phar` and `zip`, followed by every scheme `stream_wrapper_register()` added. This is PHP's full list of twelve. |
| `stream_wrapper_register()` | `stream_wrapper_register(string $protocol, string $class, int $flags = 0): bool` | Register a userspace wrapper class for `$protocol://` URLs. Up to 16 registrations are stored. |
| `stream_wrapper_unregister()` | `stream_wrapper_unregister(string $protocol): bool` | Remove a user-registered wrapper; built-in wrappers cannot be unregistered in v1. |
| `stream_wrapper_restore()` | `stream_wrapper_restore(string $protocol): bool` | Clears the disabled bit `stream_wrapper_unregister()` set on a built-in wrapper, and answers PHP's three cases. A wrapper that really was unregistered is restored silently and reports `true`. One that was never unregistered reports `true` with `Notice: stream_wrapper_restore(): <proto>:// was never changed, nothing to restore`. A scheme that never existed reports `false` with `Warning: stream_wrapper_restore(): <proto>:// never existed, nothing to restore`, which `@` suppresses. Both go to stdout through the output-buffer funnel, which is where PHP CLI puts them. |

When `fopen("$protocol://...")` matches a registered wrapper, elephc creates an
instance through the runtime class registry. Declared property defaults are
applied, but `__construct` is not invoked on this path.

Supported wrapper methods include `stream_open`, `stream_read`, `stream_write`,
`stream_close`, `stream_eof`, `stream_seek`, `stream_tell`, `stream_flush`,
`stream_stat`, `stream_lock`, `stream_truncate`, `stream_metadata`,
`stream_set_option`, `stream_cast`, `url_stat`, and the directory methods
`dir_opendir`, `dir_readdir`, `dir_rewinddir`, and `dir_closedir`.

Wrapper methods should declare return types that match their PHP contracts.
`stream_stat()` and `url_stat()` are exceptions: declare them without a return
type, or as `mixed`, when returning associative stat arrays with string keys.

## Sockets and process streams

### By-reference output parameters

A builtin parameter the runtime only writes — `&$error_code`, `&$error_message`,
`&$peer_name`, `&$address`, `flock()`'s `&$would_block` — may be passed a variable
that was never declared, exactly as PHP's own examples do:

```php
$fp = stream_socket_client("tcp://127.0.0.1:80", $errno, $errstr, 30);
if ($fp === false) {
    echo "$errstr ($errno)\n";
}
```

The call is the variable's definition, and it holds the type that parameter
writes: `int` for `&$error_code`, `string` for `&$error_message`. A variable that
already holds an incompatible type reports elephc's ordinary reassignment error
rather than being silently overwritten, and an argument with no storage to write
back into — a literal, an expression — is rejected as it is in PHP.

This applies only to parameters the builtin purely writes. One it also reads,
such as `stream_select()`'s three arrays, stays an ordinary use and must be
declared first.

| Function | Signature | Description |
|---|---|---|
| `stream_get_transports()` | `stream_get_transports(): array` | Return recognized socket transports: `tcp`, `udp`, `unix`, `udg`, `tls`, `ssl`, `sslv2`, `sslv3`, `tlsv1.0`, `tlsv1.1`, `tlsv1.2`, and `tlsv1.3`. TLS-version names all use rustls default negotiation. |
| `stream_socket_server()` | `stream_socket_server($address, int &$error_code = null, string &$error_message = null, int $flags = STREAM_SERVER_BIND\|STREAM_SERVER_LISTEN, $context = null): resource\|false` | Bind a server socket for `[tcp://]host:port`, `udp://host:port`, `unix:///path`, or `udg:///path`. TCP and Unix-stream sockets listen; the datagram transports (`udp://`, `udg://`) cannot, so they must be opened with `STREAM_SERVER_BIND` alone — the default flags ask for `listen()` and PHP fails the call, warning `Unable to connect to <address> (Unknown error)`. `&$error_message` carries the reason a bind or listen failed; `&$error_code` stays `0`, as it does in php-src for this function. A failure PHP describes with no reason at all leaves `&$error_message` empty even though the warning says `Unknown error`. |
| `stream_socket_client()` | `stream_socket_client($address, int &$error_code = null, string &$error_message = null): resource\|false` | Open a client stream for `[tcp://]host:port`, `udp://host:port`, `unix:///path`, or `udg:///path`. The two error outputs carry the real failure: the `errno` of the syscall that failed and its `strerror` text. Both may be passed undeclared, as in PHP — the call defines them as `int` and `string`. A failed open also raises PHP's `Unable to connect to <address> (<reason>)` Warning, whether or not the error outputs were passed; `@` suppresses it. A host that does not resolve reports php-src's own `php_network_getaddresses: getaddrinfo for <host> failed: <reason>` text — that failure has no `errno`, so `&$error_code` stays `0` — and warns twice, as PHP does. |
| `stream_socket_accept()` | `stream_socket_accept($socket): resource\|false` | Accept the next pending connection from a listening stream. |
| `stream_socket_enable_crypto()` | `stream_socket_enable_crypto(resource $stream, bool $enable, int $crypto_method = null, resource $session_stream = null): bool` | Attach TLS to an already-connected TCP fd. `$enable=false` unwinds the session (sends `close_notify` and detaches it from the stream), leaving the fd a plain TCP socket, and reports `false` as PHP does — php-src performs the shutdown and still returns -1. On a handle that never had crypto it is a no-op and reports `true`. |
| `fsockopen()` | `fsockopen(string $hostname, int $port, int &$error_code = null, string &$error_message = null, float $timeout = null): resource\|false` | Open a TCP connection to `$hostname:$port`. The by-reference error outputs carry the real failure, as for `stream_socket_client()`. The timeout arg is evaluated but the OS default connect timeout is used in v1. |
| `pfsockopen()` | `pfsockopen(string $hostname, int $port, int &$error_code = null, string &$error_message = null, float $timeout = null): resource\|false` | Alias of `fsockopen()`; persistent connections are not meaningful for standalone native binaries. |
| `stream_set_blocking()` | `stream_set_blocking($stream, bool $enable): bool` | Toggle `O_NONBLOCK`. Non-blocking read misses return an empty `fread()` result or `false` from `fgetc()`/`fgets()` without setting EOF. User wrappers route through `stream_set_option(STREAM_OPTION_BLOCKING, ...)`. |
| `stream_set_timeout()` | `stream_set_timeout($stream, int $seconds, int $microseconds = 0): bool` | Set `SO_RCVTIMEO` on socket streams. User wrappers route through `stream_set_option(STREAM_OPTION_READ_TIMEOUT, ...)`. |
| `stream_select()` | `stream_select(array &$read, array &$write, array &$except, ?int $seconds, ?int $microseconds = 0): int` | Wait until stream arrays are ready, answer how many are, and rewrite each array to its ready subset. Backed by `poll(2)`, so there is no 64-descriptor ceiling. A `null` array is an EMPTY set, as in PHP. User wrappers are selectable when `stream_cast(STREAM_CAST_FOR_SELECT)` returns a real stream resource; one that refuses is named — `Cannot represent a stream of type user-space as a select()able descriptor` — and dropped. A `php://memory` stream is refused the same way, with `MEMORY` in the text, because it has no descriptor to poll; `php://temp`, `data:`, plain files and the standard streams all select normally. When nothing in the three arrays can be represented, PHP's `ValueError: No stream arrays were passed` is raised. |
| `stream_socket_shutdown()` | `stream_socket_shutdown($stream, int $mode): bool` | Shut down socket reads (`0`), writes (`1`), or both (`2`). |
| `stream_socket_sendto()` | `stream_socket_sendto($socket, string $data, int $flags = 0, string $address = ""): int\|false` | Send bytes to the connected peer or to an explicit datagram address. |
| `stream_socket_recvfrom()` | `stream_socket_recvfrom($socket, int $length, int $flags = 0, string &$address = ""): string\|false` | Receive bytes and optionally write back the sender address as `host:port`. |
| `stream_socket_get_name()` | `stream_socket_get_name($socket, bool $remote): string\|false` | Return local or remote socket name as `host:port`. |
| `stream_socket_pair()` | `stream_socket_pair(int $domain, int $type, int $protocol): array` | Create a pair of connected socket streams, for example `STREAM_PF_UNIX`, `STREAM_SOCK_STREAM`, `0`. |
| `popen()` | `popen(string $command, string $mode): resource\|false` | Open a pipe to a process in read (`"r"`) or write (`"w"`) mode. |
| `pclose()` | `pclose($handle): int` | Close a process pipe and return its termination status. |

Socket addresses use `[tcp://]host:port`, `udp://host:port`, `unix:///path`, or
`udg:///path`. Host names are resolved through the system resolver to IPv4.

## Directory streams

Directory handles are stream resources too. `opendir()`, `readdir()`,
`rewinddir()`, and `closedir()` are documented with filesystem functions in
[System & I/O](system-and-io.md). Registered userspace wrappers can implement
`dir_opendir`, `dir_readdir`, `dir_rewinddir`, and `dir_closedir`; the `glob://`
wrapper exposes glob matches through the same directory-stream API.

## Stream metadata and introspection

| Function | Signature | Description |
|---|---|---|
| `get_resource_type()` | `get_resource_type(resource $handle): string` | Return the resource's PHP type name — `"stream"`, `"stream-context"` or `"stream filter"` — and `"Unknown"` once the handle has been closed. |
| `get_resource_id()` | `get_resource_id(resource $handle): int` | Return the numeric id shown in `Resource id #N`. |
| `stream_isatty()` | `stream_isatty(resource $stream): bool` | Report whether the stream is connected to an interactive terminal. |
| `stream_is_local()` | `stream_is_local(resource\|string $stream): bool` | Return `true` for local streams. |
| `stream_supports_lock()` | `stream_supports_lock(resource $stream): bool` | Return `true` when a stream supports `flock()`. |
| `stream_get_meta_data()` | `stream_get_meta_data(resource $stream): array` | Return metadata keys `timed_out`, `blocked`, `eof`, `unread_bytes`, `stream_type`, `wrapper_type`, `mode`, `seekable`, and `uri`, in PHP's insertion order. A stream opened over `http://` or `https://` also carries `wrapper_data`, the response header lines — the same array `$http_response_header` holds. |
| `http_get_last_response_headers()` | `http_get_last_response_headers(): ?array` | PHP 8.4's replacement for `$http_response_header`. Returns the last HTTP response's header lines, status line first, or `null` when no request has been made yet. |
| `http_clear_last_response_headers()` | `http_clear_last_response_headers(): void` | Drop the buffered response so the getter answers `null` again. It clears engine state only: an already-populated `$http_response_header` keeps its value, matching PHP. |

From `--php-version 8.5` on, naming `$http_response_header` emits
`Deprecated: The predefined locally scoped $http_response_header variable is
deprecated, call http_get_last_response_headers() instead`. PHP raises the same
notice while *compiling* a file that mentions the variable, so it fires once per
program and before any output, even when the mentioning statement never runs;
elephc emits it from the program prologue for the same reason. Programs that use
`http_get_last_response_headers()` instead stay quiet.

`$http_response_header` itself is published by `fopen()` over `http://`/`https://`.
`file_get_contents()` does not publish it (`http_get_last_response_headers()` does
answer after a `file_get_contents()` request), which is a known divergence from PHP.

Closing a handle changes its reported type but not its id. After `fclose()`,
`pclose()` or `closedir()`, `get_resource_type($handle)` returns `"Unknown"` and
`var_dump($handle)` prints `resource(N) of type (Unknown)` — matching PHP 8.5.6,
which renames every closed resource that way regardless of what it was. The id
`N` is unchanged, `get_resource_id()` still answers it, and `"$handle"` still
renders `Resource id #N`, because php-src leaves `zend_resource.handle` alone on
close. A resource reports its own type name — `"stream"`, `"stream-context"` or
`"stream filter"` — whether it is held directly or has travelled through an
untyped parameter, and `"Unknown"` once closed in either case.

`stream_get_meta_data()` derives `eof`, `seekable` and `blocked` from the live
descriptor. `wrapper_type`, `uri` and `mode` are recorded per handle when the
stream is opened, so a file reports `plainfile` with its path, a `php://` stream
reports `PHP`, and a `data:` URL reports `RFC2397` — PHP's name for that wrapper,
which is the RFC rather than the scheme.

`mode` is the mode string the caller passed, unnormalised: `rb` stays `rb` and
`a` stays `a`. The memory wrappers are the exception PHP itself makes — they
report the mode of the stream php-src built, so `php://memory` and `php://temp`
answer `a+b` for an append mode, `w+b` when the mode asks for any write access,
and `rb` otherwise, while `php://output` always answers `wb`. A stream that
records no mode, such as a socket or an accepted connection, still reports the
descriptor's access bits.

`stream_type` names the wrapper and backend, as php-src does, rather than the
descriptor: `STDIO` for a file, a process pipe and the standard streams, `MEMORY`
for `php://memory`, `TEMP` for `php://temp`, `Output` and `Input` for the two
matching wrappers, `RFC2397` for a `data:` URL, `dir` and `glob` for directory
handles, and one of `tcp_socket/ssl`, `udp_socket`, `unix_socket` or
`generic_socket` for a socket — the transport being read from the address the
caller wrote, with an accepted connection taking its listener's.

`uri` is the path the caller opened, and for a stream that made its own — `tmpfile()`
— the file it created, as PHP reports.
