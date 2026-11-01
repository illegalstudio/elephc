---
title: "Zip"
description: "PHP's ext/zip surface: ZipArchive archives, compression, encryption, extraction, and the deprecated procedural API."
sidebar:
  order: 29
---

elephc implements PHP's `ext/zip` extension on a pure-Rust archive engine. There is
no system `libzip`, no `-lzip`, and no native package to install: `libelephc_zip.a`
is linked only when a program references the zip surface. `fopen('zip://archive#entry')`
is available, including archive passwords supplied through the `zip` stream context,
and `ZipArchive::getStream*()` exposes PHP-shaped stream metadata, CRC errors, and
archive-close invalidation.

## Setup

Static references to `ZipArchive`, a `zip_*` function, or a `zip://` literal inject
the zip prelude automatically. For code reached only through opaque `eval()` source,
force it with:

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
the basenames in one directory with the pure-Rust engine, without requiring the
managed PCRE2 package.

## Compression and encryption

Reading supports stored, deflate, deflate64, bzip2, LZMA, XZ, PPMd, AES-128/192/256,
and traditional PKWARE encryption. Writing supports stored, deflate, bzip2,
AES-128/192/256, and traditional PKWARE (ZipCrypto). `setEncryptionName()` /
`setEncryptionIndex()` default to the password passed at the call site;
`setPassword()` supplies the archive-wide fallback used by reads and later writes.

## Differences from PHP

- `ZipArchive::LIBZIP_VERSION` answers `1.11.4` as a compatibility identifier for
  the libzip surface this engine implements. The engine itself is pure Rust.
- The `zip://` wrapper is a PHP user-space wrapper over the same bridge rather than
  a native libzip stream. A `zip://` literal (and `--with-zip`) injects it, it accepts
  the `zip` stream-context password, and `file_get_contents()` honors that context.
  The `getStream*()` streams become unreadable with PHP's "Containing zip archive was
  closed" warning after `close()`.
- `stat()`/`file_exists()`/`filesize()` on `zip://` fail like PHP; `fstat()` returns
  the PHP-shaped OS stat array. Deflated `getStream*()` and `zip://` descriptors still
  report `seekable=true` in `stream_get_meta_data()`, although `fseek()` correctly
  fails.
- `extractTo()` into a custom (non-`file://`) stream wrapper is refused with PHP's
  `stream_open` warning when the wrapper cannot open a target; actual extraction is
  limited to local filesystem destinations.
- `addPattern()` accepts the PCRE subset the pure-Rust `regex` engine understands.
  Lookarounds/backreferences that only PCRE2 can evaluate are rejected with
  `ZipArchive::addPattern(): Invalid expression`.
- `addGlob()` honors `GLOB_BRACE`; the remaining `GLOB_*` ordering/only-directory
  flags are not represented by the compiler's `glob()`.
- `CHECKCONS` validates local headers, entry names/methods and data ranges. It does
  not turn an entry CRC mismatch into an open failure; the stream read reports it,
  matching PHP.
- When no OPcache/web `ini_set()` owns the name, the zip prelude's fallback answers
  `false` for directives other than `open_basedir`. The shared `open_basedir` arm
  returns the previous value even when PHP would refuse a loosening.
- `stream_get_meta_data()` on a generic user-space wrapper now reports its URI,
  `seekable` and `eof`; PHP's `wrapper_data` object is not exposed. Runtime
  `file_get_contents()` warnings omit PHP's `in <file> on line <line>` suffix.
- `torrentzip` writes a deterministic `TORRENTZIPPED-*` archive comment and reports
  `AFL_IS_TORRENTZIP` on reopen; the exact libzip byte layout is not reproduced.
- `getStream*()` snapshots the entry when the stream is created, then marks the
  stream closed when the archive closes, matching PHP's observable failure mode.
- Rewriting an encrypted entry (for example to change its comment or mtime)
  requires the archive or entry password.

## Tests

The committed corpus under `tests/php_src/zip/` is the PHP 8.5.10 `ext/zip/tests`
suite. `scripts/gen_php_src_zip_cases.py` generates
`tests/codegen/zip_php_src_cases.rs`; 109 fixtures run on macOS (one more,
`oo_addfile_proc`, is Linux-only). Two fixtures are not generated: `bug51353` has
an unconditional php-src `SKIPIF` for its 100,000-entry stress run, and `gh17319`
expects `false` from PHP's own freed archive resource while elephc safely returns
the entry name. The generator applies the compiler-compatibility source edits listed in
`tests/php_src/zip/README.md`; the assertions still compare php-src's values.
`tests/codegen/zip.rs` carries focused end-to-end coverage, and
`cargo test -p elephc-zip` runs the engine unit tests.
