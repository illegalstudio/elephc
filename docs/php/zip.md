---
title: "Zip"
description: "PHP's ZipArchive extension: in-memory archives, extraction, globbing, and PHP 8.6 openString()/closeString()."
sidebar:
  order: 23
---

elephc implements `ZipArchive` through an injected PHP prelude backed by the
pure-Rust `elephc-zip` bridge. The bridge reuses the ZIP reader/writer from
`elephc-phar` and keeps every archive in memory, so it needs no `libzip` or
system ZIP library and works on every supported target.

## Supported surface

| Area | Methods |
|---|---|
| Open and close | `open()`, `close()`, `openString()`, `closeString()` (PHP 8.6) |
| Entries | `count()`, `numFiles`, `getNameIndex()`, `locateName()`, `getFromIndex()`, `getFromName()` |
| Metadata | `statIndex()`, `statName()`, `getStatusString()`, `clearError()` |
| Streams | `getStream()`, `getStreamName()`, `getStreamIndex()` |
| Writing | `addFromString()`, `addEmptyDir()`, `addFile()`, `replaceFile()`, `addGlob()`, `addPattern()` |
| Editing | `deleteIndex()`, `deleteName()`, `renameIndex()`, `renameName()` |
| Comments | `setArchiveComment()`, `getArchiveComment()`, `setCommentIndex()`, `setCommentName()`, `getCommentIndex()`, `getCommentName()` |
| Compression and time | `setCompressionIndex()`, `setCompressionName()`, `setMtimeIndex()`, `setMtimeName()` |
| Extraction | `extractTo()` |

`openString()` with no arguments starts an empty archive. `closeString()`
serializes the current in-memory state and closes the handle. An archive that
was only read is returned byte-for-byte, so an untouched input round-trips
exactly.

`open()` reads a real file, honors `CREATE`, `EXCL` and `OVERWRITE`, and
commits pending mutations back to that file on `close()` (or destruction).
`RDONLY` rejects every mutating method with `ER_RDONLY`. A missing file without
`CREATE` reports `ER_NOENT`.

```php
$zip = new ZipArchive();
$zip->openString();
$zip->addFromString("hello.txt", "Hello\n");
$bytes = $zip->closeString();

$copy = new ZipArchive();
$copy->openString($bytes);
echo $copy->getFromName("hello.txt");
```

## PHP 8.6 behavior

- `openString()` returns `true` or a `ZipArchive::ER_*` error code; `EXCL`
  returns `ER_EXISTS` because there is no file to create.
- `extractTo()` throws a `TypeError` when the `$files` array contains a
  non-string element.
- `addGlob()` and `addPattern()` throw a `TypeError` when `remove_all_path` is
  not a bool, or when `comp_method`, `comp_flags`, or `enc_method` is not an
  int (numeric strings are coerced).
- `serialize()` and `unserialize()` throw an `Exception`; a subclass can
  override `__serialize()`/`__unserialize()` to round-trip through
  `closeString()`/`openString()`.

## Limitations

- Only the store and deflate compression methods are emitted; other
  `ZipArchive::CM_*` values report `ER_COMPNOTSUPP`.
- Encrypted entries require a password set through libzip; elephc's bridge does
  not implement ZIP encryption. `setPassword()`/`setEncryption*()` return
  `false` with `ER_ENCRNOTSUPP`, and `isEncryptionMethodSupported()` returns
  `false`. Unchanged encrypted entries still survive a rewrite because their
  original compressed payload is copied verbatim.
- Progress and cancellation callbacks, external attributes, archive flags, and
  the `unchange*` family return `false` with `ER_OPNOTSUPP`.
- `addGlob()` accepts the PHP `$flags` argument but the in-tree `glob()`
  builtin does not implement `GLOB_*` flags, so they are ignored.
- Writing a ZIP64 archive is rejected instead of truncating; archives that
  need 64-bit offsets or sizes must be handled by the caller.

## Linking

The `ZipArchive` prelude carries an `extern "elephc_zip"` block, so a program
that names the class links `libelephc_zip.a` automatically. Use `--with-zip` to
force the bridge and prelude into a program that reaches `ZipArchive` through
dynamic class naming.
