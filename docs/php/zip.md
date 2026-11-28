---
title: "Zip"
description: "PHP's ext/zip surface: ZipArchive archives, compression, encryption, extraction, and the deprecated procedural API."
sidebar:
  order: 29
---

elephc implements PHP's `ext/zip` extension as a thin FFI layer over **libzip 1.11.4**,
the same C library PHP itself uses. The bridge is a Rust `staticlib` (`libelephc_zip.a`)
that declares libzip's public API; the final link resolves the pinned archives from the
managed native catalog. There is no system `-lzip` and no Homebrew fallback.
`fopen('zip://archive#entry')` is available, including archive passwords supplied
through the `zip` stream context, and `ZipArchive::getStream*()` exposes PHP-shaped
stream metadata, CRC errors, and archive-close invalidation.

## Setup

Add the managed package to the project once:

```bash
elephc native add libzip
```

That declares `libzip` plus its `bzip2`, `xz` (LZMA), `zlib`, and `openssl`
dependencies in `elephc.toml`/`elephc.lock` and builds them with the target C
toolchain. `cmake` must be available when the package is first built; the pinned
OpenSSL archive provides AES support on every target, and no system compression or
crypto library is consulted.

Static references to `ZipArchive`, a `zip_*` function, or any string literal that
begins with `zip://` inject the zip prelude automatically. Detection is syntactic: a
literal that begins with `zip://` but is only printed still links the bridge, while a
`zip://` mention later in a message (for example `"see zip://docs"`) does not. For code
reached only through opaque `eval()` source, force it with:

```bash
elephc --with-zip main.php
```

`--with-zip` links the bridge and keeps the complete PHP surface reachable. A
program that never mentions zip links nothing.

## Create and read an archive

```php
$zip = new ZipArchive();
$zip->open(__DIR__ . '/demo.zip', ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('readme.txt', "hello\n");
$zip->addFile(__FILE__, 'source/main.php');
$zip->addEmptyDir('assets');
$zip->setCommentName('readme.txt', 'generated');
$zip->close();

$zip = new ZipArchive();
$zip->open(__DIR__ . '/demo.zip');
echo $zip->numFiles, "\n";
echo $zip->getFromName('readme.txt');
$zip->extractTo(__DIR__ . '/out');
$zip->close();
```

`open()` accepts `CREATE`, `EXCL`, `CHECKCONS`, `OVERWRITE`, and `RDONLY`.
`close()` rewrites the archive atomically; deleting every entry removes the file
unless `AFL_CREATE_OR_KEEP_FILE_FOR_EMPTY_ARCHIVE` is set.

## Surface

The complete `ZipArchive` class is available:

- Entries: `addFromString`, `addFile`, `replaceFile`, `addEmptyDir`, `addGlob`,
  `addPattern`, `deleteIndex`, `deleteName`, `renameIndex`, `renameName`.
- Inspection: `count`, `statIndex`, `statName`, `locateName`, `getNameIndex`,
  `getFromIndex`, `getFromName`, `getStream`, `getStreamName`, `getStreamIndex`.
- Metadata: `get/setArchiveComment`, `get/setCommentIndex`, `get/setCommentName`,
  `get/setMtimeIndex`, `get/setMtimeName`, `get/setExternalAttributesIndex`,
  `get/setExternalAttributesName`, `get/setArchiveFlag`.
- Compression and encryption: `setCompressionIndex`, `setCompressionName`,
  `setEncryptionIndex`, `setEncryptionName`, `setPassword`,
  `isCompressionMethodSupported`, `isEncryptionMethodSupported`.
- Revisions: `unchangeIndex`, `unchangeName`, `unchangeAll`, `unchangeArchive`.
- Extraction and callbacks: `extractTo`, `registerProgressCallback`,
  `registerCancelCallback`, `getStatusString`, `clearError`.

All `ZipArchive::*` constants are declared, including `ER_*`, `CM_*`, `EM_*`,
`OPSYS_*`, `FL_*`, `AFL_*`, `LENGTH_TO_END`, `LENGTH_UNCHECKED`, and
`LIBZIP_VERSION`. The deprecated procedural API (`zip_open`, `zip_read`,
`zip_entry_*`, `zip_close`) is also present; each function raises PHP 8's
deprecation notice.

`addGlob()` supports `add_path`, `remove_path`, `remove_all_path`, `flags`,
`comp_method`, `comp_flags`, `enc_method`, and `enc_password`, including brace
patterns such as `*.{txt,baz}`. `addPattern()` matches PCRE-style patterns against
the basenames in one directory with the crate's regex engine, without requiring the
managed PCRE2 package.

## Compression and encryption

libzip provides the format layer. Reading supports stored, deflate, bzip2,
LZMA/XZ, and PPMd entry metadata, with AES-128/192/256 and traditional PKWARE
encryption. Writing supports stored, deflate, bzip2, LZMA/XZ,
AES-128/192/256, and traditional PKWARE (ZipCrypto). `setEncryptionName()` /
`setEncryptionIndex()` default to the password passed at the call site;
`setPassword()` supplies the archive-wide fallback used by reads and later writes.

## Differences from PHP

- `ZipArchive::LIBZIP_VERSION` reports the pinned catalog libzip version (`1.11.4`),
  matching the archive the managed package links.
- The `zip://` wrapper is a PHP user-space wrapper over the same bridge rather than
  a native libzip stream. A `zip://` literal (and `--with-zip`) injects it, it accepts
  the `zip` stream-context password through elephc's single active context slot (the
  last-created/default context, like every user-space wrapper), and
  `file_get_contents()` honors that context. The context is accepted only when the
  path statically starts with `zip://` (including a literal prefix concatenated with
  variables); every other path must pass `null`, because the built-in readers do not
  consume the announced options.
  The `getStream*()` streams become unreadable with PHP's "Containing zip archive was
  closed" warning after `close()`.
- `stat()`/`file_exists()`/`filesize()` on `zip://` fail like PHP; `fstat()` returns
  the PHP-shaped OS stat array. Deflated `getStream*()` and `zip://` descriptors still
  report `seekable=true` in `stream_get_meta_data()`, although `fseek()` correctly
  fails.
- `extractTo()` into a custom (non-`file://`) stream wrapper is refused with PHP's
  `stream_open` warning when the wrapper cannot open a target; actual extraction is
  limited to local filesystem destinations. Extracted files keep the process umask's
  mode, matching php-src's `php_zip_extract_file`; archived permission bits are ignored.
  A final-component symlink in the destination is refused instead of followed.
- `addPattern()` accepts the PCRE subset the crate's `regex` engine understands and
  matches names byte-wise like PCRE without the `u` modifier (the `u` modifier enables
  Unicode matching). `i`, `m`, `s`, `x`, `u` and `A` are honored; `U` (ungreedy) has no
  Rust-regex equivalent and is rejected rather than silently matched greedily. Lookarounds
  and backreferences that only PCRE2 can evaluate are also rejected with
  `ZipArchive::addPattern(): Invalid expression`. Only same-byte delimiters are accepted
  (PCRE's paired `{}`/`[]`/`()` forms are rejected), and `x` maps to Rust's verbose mode,
  where `#` starts a comment (PCRE's `x` leaves `#` literal).
- `addGlob()` honors `GLOB_BRACE`; the remaining `GLOB_*` ordering/only-directory
  flags are not represented by the compiler's `glob()`, and `GLOB_NOCHECK` is not
  emulated. There is no `MAXPATHLEN` pattern-length guard. Option validation is partial:
  an empty `add_path` is treated as unset, and type mismatches are coerced rather than
  rejected with php-src's `ValueError`/type warnings.
- `registerProgressCallback()` observes libzip's intermediate progress while `close()`
  writes; libzip reports the close-time start (`0.0`) and end (`1.0`). An exception
  from an intermediate callback is re-raised from `close()`, matching php-src's
  pending-exception contract, and endpoint throws propagate the same way. A rate
  outside `[0, 1]` returns `false` like libzip.
- `close()` and every mutator (`addFromString()`, `deleteName()`, `setPassword()`,
  callback registration, and so on) throw `Error: Already being closed` when called
  from a progress or cancel callback, matching PHP 8.5.13's `php_zipobj_closing()`.
  `getStream*()` also refuses a closing archive.
- `registerCancelCallback()` is invoked by libzip at the start of the write and through
  the progress updates, so a deadline-based callback can abort a large archive. A
  callback exception or non-`int` return cancels the write and is re-raised from
  `close()` after libzip's `Operation cancelled` warning, matching php-src.
- `CHECKCONS` delegates consistency checking to libzip. It reports libzip's
  `ER_TRUNCATED_ZIP`/`ER_INCONS` code for a damaged central directory rather than
  opening the archive and deferring the failure to a read.
- libzip links the managed OpenSSL 3.5.8 package (`libssl.a` + `libcrypto.a`). A program
  that also selects the OpenSSL cipher bridge links that bridge's separate 3.6.4
  `libcrypto.a`; elephc does not merge the two static OpenSSL builds.
- The exact compression-method availability follows the managed libzip build, just as
  PHP's follows its own libzip build: bzip2, LZMA, and XZ are enabled, while zstd,
  deflate64, and PPMd encode are not. `isCompressionMethodSupported()` therefore
  reports `false` for those methods.
- When no OPcache/web `ini_set()` owns the name, the zip prelude's fallback answers
  `false` for directives other than `open_basedir`. This `open_basedir` support is
  zip-only: other filesystem functions do not observe it, and the fallback returns the
  previous value even when PHP would refuse a loosening. Paths are resolved with
  `realpath()` (symlinks, `.`, `..`, and relative components) before the allowed-root
  prefix check; a `.` root uses the working directory, matching php-src.
- `stream_get_meta_data()` on a generic user-space wrapper reports its URI, requested
  `mode`, `seekable` and `eof`; PHP's `wrapper_data` object is not exposed. Saved
  metadata copies the URI, so reusing the handle slot does not rewrite an earlier
  array. The synthetic zip stream URI is capped at 255 bytes. Runtime
  `file_get_contents()` warnings include PHP's `in <file> on line <line>` suffix.
- Archive and entry paths must be valid UTF-8; php-src passes raw bytes, so a
  Latin-1-named file that PHP can open is rejected here. Interior NUL bytes are
  rejected rather than truncated at the NUL as php-src's C strings are.
- Binary payloads cross the bridge ABI hex-encoded (two bytes per payload byte), so a
  large `getFrom*()` or whole-entry `zip://` read holds a transient hex string plus the
  decoded copy. The encoding keeps arbitrary binary data NUL-safe across the C ABI.
- `stream_get_meta_data()` on a `getStream*()` memory stream whose archive has already
  been closed can emit the wrapper's "Containing zip archive was closed" warning; PHP's
  metadata call is silent.
- A subclass that overrides `getStream*` keeps its own declared return type; only the
  inherited `ZipArchive` implementation receives the `resource|false` checker contract.
- The prelude registers `zip` when it is injected. A later
  `stream_wrapper_register('zip', ...)` returns `false` instead of replacing the wrapper
  as php-src does.
- `zip_open()` and `zip_read()` return stream resources, so `get_resource_type()`
  reports `stream` for an open directory or entry where php-src reports
  `Zip Directory`/`Zip Entry`. After `zip_close()`/`zip_entry_close()` both report
  `Unknown`, like php-src.
- The `zip://` wrapper reads the whole entry when it is opened, whereas PHP's native
  stream reads lazily; opening a very large entry therefore costs its uncompressed size
  up front. A damaged entry surfaces `Zip stream error: CRC error` through the wrapper's
  memory-stream read (the warning carries the `fread()` prefix rather than PHP's
  caller-specific `file_get_contents()` prefix). Because the entry is a snapshot,
  `fstat()` and metadata keep working after the backing archive file is replaced with a
  non-zip file, where php-src's live stream reports the archive as unreadable.
- `extractTo()` and `getFrom*()` reject a CRC-corrupt entry with PHP 8.5.11+'s
  `Cannot extract "<entry>": "CRC error"` / `Cannot read entry: CRC error` warning,
  and install no partial bytes. A failed `zip_fread()` after the entry opened (CRC,
  zlib, truncation) warns the same way, while an entry that cannot be opened at all
  (wrong password, unsupported method) fails silently, matching php-src.
  `extractTo()` iterates array values, so a filtered selection such as
  `array_filter([...])` works despite its preserved keys.
- `extractTo()` carries its selection as a JSON name list, so a non-UTF-8 entry name
  cannot be selected; extracting the whole archive still handles it. It also sanitizes
  `..` and absolute entry names into the destination; php-src's own path helper can
  write outside the destination for a `..` entry, so the bridge is deliberately stricter.
- Zip warnings and deprecations use the engine's `E_WARNING`/`E_DEPRECATED` levels, so
  `set_error_handler()`, `error_reporting()`, and `error_get_last()` observe the same
  values as php-src. The procedural `zip_*` deprecation follows the profile: PHP 8.0–8.3
  print the short `Function zip_open() is deprecated` text, while PHP 8.4+ adds the
  `since 8.0` and replacement suffix. The synthetic wrapper metadata tables and
  wrapper-backed readers are emitted only for programs that can register or reach a
  user-space wrapper.
- `torrentzip` is libzip's own implementation: `AFL_WANT_TORRENTZIP` writes the
  deterministic byte layout and `AFL_IS_TORRENTZIP` reports it on reopen.
- `getStream*()` snapshots the entry when the stream is created, then marks the
  stream closed when the archive closes, matching PHP's observable failure mode.
- Rewriting an encrypted entry (for example to change its comment or mtime)
  requires the archive or entry password.

## Tests

The PHP 8.5.13 `ext/zip/tests` cases are ported to Rust in
`tests/codegen/zip_php_src_cases.rs`; no `.phpt` file is committed or parsed at run
time. `scripts/gen_php_src_zip_cases.py` downloads the tagged upstream tarball and
regenerates that Rust module; `tests/php_src/zip/` holds only the binary/text assets
the cases reference and the `prelude.php` transcription oracle. All 130 fixtures are
ported. `bug51353` carries `#[ignore]` because upstream skips its 100,000-entry stress
run, and `oo_addfile_proc` is Linux-only, so 128 run by default on macOS and 129 on
Linux. The generator applies the compiler-compatibility source edits listed in
`tests/php_src/zip/README.md`; the assertions still compare php-src's values.
`tests/codegen/zip.rs` carries focused end-to-end coverage, and
`cargo test -p elephc-zip` runs the libzip-backed engine unit tests when
`ELEPHC_ZIP_NATIVE_LIB_DIRS` points at the installed `lib/` directories.
