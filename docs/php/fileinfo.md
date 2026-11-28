---
title: "File information"
description: "PHP fileinfo functions, finfo objects, MIME types and character encodings."
sidebar:
  order: 28
---

PHP's fileinfo extension inspects file contents with libmagic. Elephc provides all
six functions, the `finfo` class and the eleven `FILEINFO_*` constants on every
supported target, including inside `eval()`.

## Setup

Declare the pinned native package once in the PHP project:

```bash
elephc native add libmagic
elephc build main.php
```

Static references automatically link `elephc_fileinfo` and the managed libmagic
package. For opaque dynamic source use `--with-fileinfo`, which also retains the
PHP declarations. The compiled binary includes the complete libmagic 5.46 magic
database. Deployment needs no system libmagic or database file.

## Identify content

```php
$info = new finfo(FILEINFO_MIME_TYPE);
echo $info->buffer("Hello"), "\n"; // text/plain
echo $info->buffer("Hello", FILEINFO_MIME_ENCODING), "\n"; // us-ascii
echo $info->buffer("Hello", FILEINFO_MIME), "\n"; // text/plain; charset=us-ascii
echo $info->file(__FILE__), "\n";
```

`finfo_buffer()` and `finfo_file()` are the procedural equivalents. Both return a
string on success or `false` on a recoverable identification/read failure.
Failures raise `E_WARNING` through the active PHP error handler and reporting mask. Buffer
contents are binary-safe. `finfo_file()` accepts stream-wrapper URLs and a nullable
stream context. The context argument of `finfo_buffer()` has no effect, as in PHP.
Directories return `directory`.

Wrapper names may be built at runtime. File queries support `data://`, local
`file://` URLs, Phar URLs, read-only `zip://archive#entry` URLs, standard-input
and temporary `php://` streams, and `php://filter` read chains.
Read filters inspect the transformed bytes, including nested wrapper resources.
Like PHP 8.5, file queries identify a wrapper only after stream stat succeeds.
Direct network URLs still perform their request with the stream context, then
return `false` because their wrapper stat fails. A `php://filter` layer can expose
the underlying socket stat, which PHP 8.5 classifies as `directory`. Compressed
wrappers without stream stat return `false`. User wrappers returning directory metadata
produce `directory`; a failed initial `stream_stat()` returns `false` without reading.
Byte identification performs its own `stream_stat()` and, after reading, invokes
`stream_cast(0)` on unfiltered user streams. A missing cast method warns while
identification continues. Successfully attached read or write filters suppress
that cast callback. Write filters retain their creation and close callbacks
without transforming the bytes read for identification.
Plain data URL payloads use PHP URL decoding: `+` becomes a space and `%2B`
preserves a literal plus byte. Base64 payloads retain their decoded binary bytes.

`mime_content_type($filename)` accepts a filename or an open stream resource and
returns its MIME type. Stream identification seeks to the beginning and restores
the previous stream position.
Stream callback exceptions propagate with PHP warning behavior. A matching PHP
error handler is not reentered for diagnostics emitted while an exception is pending.

## Configuration and ownership

`finfo_open(int $flags = FILEINFO_NONE, ?string $magic_database = null)` returns a
`finfo` object or `false`. `new finfo(...)` throws an exception when database
initialization fails. Null or empty database names select the embedded default;
custom text and compiled magic databases are accepted. When a successful load
emits diagnostics, `finfo_open()` replays them as warnings and returns the
object, while `new finfo()` throws the first diagnostic as an exception. A
user error handler that throws during replay propagates its exception.

When the build sets `open_basedir`, a custom database must resolve inside one of
its roots. The check follows symlinks, folds `..`, honours directory boundaries,
and accepts multiple colon-separated roots. A rejected path produces PHP's
`open_basedir restriction in effect` warning for `finfo_open()` or exception for
`new finfo()` before the database is opened.

`$info->set_flags($flags)` and `finfo_set_flags($info, $flags)` change persistent
options. A nonzero flags argument on an individual query temporarily replaces
those options, including on failure. Passing `FILEINFO_NONE` to a query uses the
instance's persistent options.

Like PHP 8, `finfo_close($info)` returns true and leaves the object usable. The
object releases its native state automatically. In the PHP 8.5 and later profiles,
`finfo_close()` and an explicitly supplied `finfo_buffer()` / `finfo::buffer()`
context emit `E_DEPRECATED`; older profiles retain their original behavior. Cloning and serialization are
refused. Content descriptions and the set of recognized file formats follow the
pinned upstream libmagic database, which can differ from the database bundled by
a particular PHP release.

## Known divergences

- `open_basedir` is enforced for the fileinfo magic database and for every file
  content read this prelude performs. Other file APIs in the same program do not
  yet share that confinement.
- `phpinfo(0)` prints only the page header and `phpinfo(INFO_MODULES)` prints the
  compact module table; every other flag value renders the complete compile-time
  table. The eval interpreter renders its own environment table instead.
- The integer and float bytes emitted by `pack()` use the native little-endian
  layout. Every currently supported target is little-endian.
- `phpinfo()` always reports the fileinfo module, because the compiler ships that
  surface and the PHPT corpus inspects it. `extension_loaded('fileinfo')` and
  `get_loaded_extensions()` report it only when the program references the
  bridge, so the two signals disagree for a program that never uses fileinfo.
- Under `strict_types`, a boxed `mixed` argument bound to a scalar union such as
  `int|string` is checked exactly and reports PHP's runtime type. Coercive mode
  checks the same value member by member but does not convert it to another
  member. The gap follows the argument's **static** type: a value whose static
  type is `mixed` is never converted, even when the runtime value is a scalar
  member, so a `mixed` holding `true` passed to `int|string` stays boolean where
  PHP passes `int(1)`, and one holding `1.5` stays a float where PHP passes
  `int(1)` with its `Deprecated:` notice. A statically typed concrete scalar
  follows the coercive table; this remains a known divergence.
- The `finfo` query methods keep their declared `string` parameters strict, as
  compiler-injected classes do. `$f->file(false)` and `$f->file(null)` are
  compile errors where PHP's weak argument handling coerces the value to `""`;
  pass an explicit string. `$flags` and `$context` slots are widened and follow
  PHP's coercions.
- `finfo` declares `__clone`, `__serialize`, `__unserialize` and `__debugInfo` to
  reproduce PHP's clone/serialization errors and `var_dump()` output, so
  `get_class_methods('finfo')` lists those four names in addition to PHP's
  `__construct`, `file`, `buffer` and `set_flags`.
- `pack()` reports each format's diagnostics while it encodes, whereas php-src
  parses the whole format first. When one call produces both an encoding
  diagnostic and trailing unused values, PHP prints `arguments unused` first and
  elephc prints it last. The returned bytes and every warning text match.

## Compliance

PHP 8.5.10's complete `ext/fileinfo/tests` corpus is committed under
`tests/php_src/fileinfo/`, and each fixture is generated into an ordinary Rust `#[test]` with its PHP source and
assertion inline in `tests/codegen/fileinfo_php_src_cases.rs`. All **62** fixtures
run through the compiler's own codegen harness,
including the zip-stream fixture and the `intl` locale-reset fixture; the latter
skips, like php-src's SKIPIF, when the host does not expose `ru_RU.koi8r`.

```bash
cargo test --test codegen_tests_ext fileinfo_php_src
```

`scripts/php-src-fileinfo.py` remains available to compare a newer php-src
checkout against the same corpus without changing the committed tests.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`finfo_buffer()`](./builtins/io/finfo_buffer.md) | `(finfo $finfo, string $string, int $flags = 0, ?resource $context = null): string|false` | `string|false` | ✓ | ✓ |
| [`finfo_close()`](./builtins/io/finfo_close.md) | `(finfo $finfo): bool` | `bool` | ✓ | ✓ |
| [`finfo_file()`](./builtins/io/finfo_file.md) | `(finfo $finfo, string $filename, int $flags = 0, ?resource $context = null): string|false` | `string|false` | ✓ | ✓ |
| [`finfo_open()`](./builtins/io/finfo_open.md) | `(int $flags = 0, ?string $magic_database = null): finfo|false` | `finfo|false` | ✓ | ✓ |
| [`finfo_set_flags()`](./builtins/io/finfo_set_flags.md) | `(finfo $finfo, int $flags): bool` | `bool` | ✓ | ✓ |
| [`mime_content_type()`](./builtins/io/mime_content_type.md) | `(string|resource $filename): string|false` | `string|false` | ✓ | ✓ |

Classes: `finfo`.

Constants: `FILEINFO_APPLE`, `FILEINFO_CONTINUE`, `FILEINFO_DEVICES`, `FILEINFO_EXTENSION`, `FILEINFO_MIME`, `FILEINFO_MIME_ENCODING`, `FILEINFO_MIME_TYPE`, `FILEINFO_NONE`, `FILEINFO_PRESERVE_ATIME`, `FILEINFO_RAW`, `FILEINFO_SYMLINK`.

<!-- elephc:generated:symbols:end -->
