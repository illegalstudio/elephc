---
title: "Tidy"
description: "The ext/tidy HTML repair and inspection API: tidy_* functions, the tidy and tidyNode classes, and PHP 8.5.10's 161 TIDY_* constants."
sidebar:
  order: 29
---

elephc implements PHP's `ext/tidy` through the `elephc-tidy` bridge. The HTML engine
is libtidy itself — the same library PHP uses — pinned at 5.8.0 and supplied by the
managed native catalog as the `libtidy` package. The package is linked statically,
so compiled programs are still standalone binaries: the target machine needs neither
PHP nor a system libtidy. Every supported target (`macos-aarch64`, `ios-arm64`,
`ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`) uses the same managed package path.

## Enabling tidy

tidy needs a native package, so it is opt-in at the project level even though usage
detection is automatic. Declare it once:

```bash
elephc native add libtidy
```

`libtidy` has no catalog dependencies of its own: the one `native add` builds the
pinned 5.8.0 sources and archives `libtidy.a` in the cache. See
[Native dependencies](../compiling/native-dependencies.md) for the full `elephc
native` workflow (lock file, cache, `elephc native install`).

After that, an ordinary compile is enough — using any `tidy_*` function, the `tidy`
or `tidyNode` class, or `ob_start('ob_tidyhandler')` links the bridge and, with it,
the managed package:

```bash
elephc clean.php
```

Use `--with-tidy` when the surface is only reached at runtime, for example through
opaque dynamic `eval()` or a runtime-assembled `new $class` naming `tidy` or `tidyNode`; it
force-links the whole `elephc_tidy` archive and force-injects the prelude even when the
compiler sees no tidy usage:

```bash
elephc --with-tidy clean.php
```

Either way the project must declare `libtidy`: a compile that plans the bridge
without it fails closed with the `elephc native add libtidy` recovery, and there is
no fallback to a system `-ltidy`. `extension_loaded('tidy')` reports `true` whenever
the bridge is linked, natively and inside `eval()`.

Both surfaces behave the same inside `eval()`, which runs the very same compiled
`tidy` / `tidyNode` declarations through Magician's native-function bridge. A
literal `eval()` string is scanned for tidy function, class, and `ob_tidyhandler`
identifiers without parsing it, so `ob_start('ob_tidyhandler')` works in an inline
fragment too. Use `--with-tidy` when the only tidy call is assembled in opaque
dynamic `eval()` source, because the compiler cannot inspect that string.

## Parsing and repairing

`tidy_parse_string()` and `tidy_parse_file()` return a `tidy` document object;
`tidy_repair_string()` and `tidy_repair_file()` return the repaired markup directly.
The configuration argument accepts an associative array of option names or a
libtidy configuration-file path:

```php
$tidy = tidy_parse_string(
    '<HTML><BODY BGCOLOR=#FFFFFF><B>Hi</B></BODY></HTML>',
    ['newline' => 'LF'],
);
tidy_clean_repair($tidy);
echo tidy_get_output($tidy);

$repaired = tidy_repair_string('<B>testing</I>', ['show-body-only' => true]);
// <b>testing</b>
```

`tidy_diagnose()` runs the configured accessibility and compatibility diagnostics.
The counts (`tidy_error_count()`, `tidy_warning_count()`, `tidy_access_count()`,
`tidy_config_count()`), `tidy_get_status()` and `tidy_get_release()` read libtidy's
own state, so their values match the reference extension for the same pinned
library.

## The tidy document object

`tidy` exposes PHP's public `$value` and `$errorBuffer` properties, updated after a
successful parse, `cleanRepair()` or `diagnose()` exactly like php-src. `echo $tidy`
returns the repaired markup. The object methods mirror the procedural functions:
`getOpt()`, `cleanRepair()`, `parseFile()`, `parseString()`, `diagnose()`,
`getRelease()`, `getConfig()`, `getStatus()`, `getHtmlVer()`, `getOptDoc()`,
`isXhtml()`, `isXml()`, `root()`, `head()`, `html()` and `body()`. The static
`repairString()` / `repairFile()` methods are available as well.

`tidy_get_config()` returns every libtidy option in libtidy's iterator order, with
string, integer and boolean values typed as PHP scalars. `tidy_getopt()` returns one
option's current value; an unknown name is a `ValueError`, and a read-only option
cannot be set from the configuration array (`ValueError`).

## The tidyNode object

`tidyNode` is final. Its nine readonly properties — `value`, `name`, `type`, `line`,
`column`, `proprietary`, `id`, `attribute`, `child` — are computed eagerly, including
the recursive `child` array, so `var_dump()` output matches php-src. Tree navigation
uses `getParent()`, `getPreviousSibling()` and `getNextSibling()`; the predicates are
`hasChildren()`, `hasSiblings()`, `isComment()`, `isHtml()`, `isText()`, `isJste()`,
`isAsp()` and `isPhp()`.

A node casts to its serialized text with `(string)`, to `0` with `(int)` / `(float)`,
and to `true` with `(bool)`, matching php-src's object cast handlers.

## Configuration files and output handling

`tidy.default_config` is loaded into every new document. `tidy.clean_output=1`
installs the automatic output buffer at startup, and `ob_start('ob_tidyhandler')`
installs it explicitly; the handler repairs the whole buffer on flush.

`open_basedir` is enforced for configuration-file paths and parsed file paths with
PHP's normal `open_basedir restriction in effect` warning. In the constructor's
`EH_THROW` window both configuration warnings and file-load failures become the base
`Exception`, while a malformed configuration file emits a notice and construction
continues.

## Runtime limits

- `tidy.default_config`, `tidy.clean_output` and `open_basedir` are compile-time INI
  values: they are baked into the binary by `--ini`. Runtime `ini_set()` calls do not
  change them.
- The libtidy release is pinned at 5.8.0. `tidy_get_release()` reports
  `2021/07/10`, the same value the reference extension reports for that library.
- The bridge serializes each document on demand; very large documents therefore cost
  a full libtidy save per `tidy_get_output()` call, as in php-src.

## Differences from PHP

- **Display-layer blank lines.** PHP prints a leading blank line before displayed
  diagnostics; elephc's runtime does not. The migrated PHPT harness restores that
  separator before comparison, so the diagnostic text itself is still compared
  byte-for-byte.
- **`tidy_parse_string()`/`tidy_parse_file()` return type.** The PHP-visible return is
  `tidy|false`; elephc's injected prelude declares `mixed` so the static checker does
  not reject a `tidy` result passed to a `tidy`-typed parameter after a failure check.
  Runtime behavior is unchanged: failure returns `false`, success returns a `tidy`
  object.
- **Bridge string boundaries.** Strings returned by the C bridge are NUL-terminated. An
  embedded NUL byte truncates the value; libtidy normally strips NUL bytes from HTML
  text, so this only affects deliberately embedded control bytes.
- **Prelude adapters are visible to reflection.** The `tidy` and `tidyNode` prelude
  classes carry compiler adapters such as `__toString`, `__debugInfo`, `__clone`, and
  `__destruct`; `method_exists()` and reflection can see those names while php-src's
  internal classes expose only their documented surface. The `__toString` adapter also
  makes both classes satisfy `Stringable`, unlike php-src's cast handlers. Runtime
  casts, cloning failures, and debug dumps still match PHP.
- **`ob_tidyhandler` is callable.** PHP registers it only as an output-handler alias;
  elephc also declares it as a prelude function so `ob_start('ob_tidyhandler')` can
  resolve it. Calling it directly, and `function_exists('ob_tidyhandler')` returning
  `true` in compiled code, are elephc extensions.
- **Configuration notices use `E_USER_NOTICE`.** A malformed `tidy.default_config`
  file prints PHP's `Notice:` text, but a custom error handler sees `E_USER_NOTICE`
  instead of the engine's `E_NOTICE` level.
- **Old nodes after reparsing.** libtidy releases the previous tree when
  `parseString()`/`parseFile()` parses again. elephc invalidates the old `tidyNode`
  handles before that release, including when the new parse fails, so their methods
  return `null`, empty values, or `false` instead of reading freed memory. PHP 8.5.10
  leaves those wrappers dangling; later 8.5 builds add a parse-generation check and
  throw an `Error` instead. The node's already cached readonly properties remain
  readable.
- **Opaque dynamic `eval()` source.** Literal `eval()` strings are scanned for the
  tidy surface and link the prelude, so `tidy_*`, `tidy`/`tidyNode`, and
  `ob_start('ob_tidyhandler')` work in an inline fragment. A string assembled at
  runtime is invisible to that scan: pass `--with-tidy`, or register the handler
  from compiled code, when the buffer is produced by opaque eval source.
- **Chunked `ob_tidyhandler`.** libtidy cannot repair cumulative chunks, so the
  handler leaves intermediate flushes untouched and repairs only the combined
  `START|FINAL` buffer. php-src additionally rejects a non-zero chunk size with a
  `Cannot use a chunk size for ob_tidyhandler` warning at `ob_start()` time.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`tidy_access_count()`](./builtins/tidy/tidy_access_count.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_clean_repair()`](./builtins/tidy/tidy_clean_repair.md) | `(mixed $tidy): bool` | `bool` | ✓ | ✓ |
| [`tidy_config_count()`](./builtins/tidy/tidy_config_count.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_diagnose()`](./builtins/tidy/tidy_diagnose.md) | `(mixed $tidy): bool` | `bool` | ✓ | ✓ |
| [`tidy_error_count()`](./builtins/tidy/tidy_error_count.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_get_body()`](./builtins/tidy/tidy_get_body.md) | `(mixed $tidy): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_get_config()`](./builtins/tidy/tidy_get_config.md) | `(mixed $tidy): array` | `array` | ✓ | ✓ |
| [`tidy_get_error_buffer()`](./builtins/tidy/tidy_get_error_buffer.md) | `(mixed $tidy): string|false` | `string|false` | ✓ | ✓ |
| [`tidy_get_head()`](./builtins/tidy/tidy_get_head.md) | `(mixed $tidy): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_get_html()`](./builtins/tidy/tidy_get_html.md) | `(mixed $tidy): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_get_html_ver()`](./builtins/tidy/tidy_get_html_ver.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_get_opt_doc()`](./builtins/tidy/tidy_get_opt_doc.md) | `(mixed $tidy, string $option): string|false` | `string|false` | ✓ | ✓ |
| [`tidy_get_output()`](./builtins/tidy/tidy_get_output.md) | `(mixed $tidy): string` | `string` | ✓ | ✓ |
| [`tidy_get_release()`](./builtins/tidy/tidy_get_release.md) | `(): string` | `string` | ✓ | ✓ |
| [`tidy_get_root()`](./builtins/tidy/tidy_get_root.md) | `(mixed $tidy): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_get_status()`](./builtins/tidy/tidy_get_status.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_getopt()`](./builtins/tidy/tidy_getopt.md) | `(mixed $tidy, string $option): string|int|bool` | `string|int|bool` | ✓ | ✓ |
| [`tidy_is_xhtml()`](./builtins/tidy/tidy_is_xhtml.md) | `(mixed $tidy): bool` | `bool` | ✓ | ✓ |
| [`tidy_is_xml()`](./builtins/tidy/tidy_is_xml.md) | `(mixed $tidy): bool` | `bool` | ✓ | ✓ |
| [`tidy_parse_file()`](./builtins/tidy/tidy_parse_file.md) | `(string $filename, array|string|null $config = null, ?string $encoding = null, bool $useIncludePath = false): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_parse_string()`](./builtins/tidy/tidy_parse_string.md) | `(string $string, array|string|null $config = null, ?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_repair_file()`](./builtins/tidy/tidy_repair_file.md) | `(string $filename, array|string|null $config = null, ?string $encoding = null, bool $useIncludePath = false): string|false` | `string|false` | ✓ | ✓ |
| [`tidy_repair_string()`](./builtins/tidy/tidy_repair_string.md) | `(string $string, array|string|null $config = null, ?string $encoding = null): string|false` | `string|false` | ✓ | ✓ |
| [`tidy_warning_count()`](./builtins/tidy/tidy_warning_count.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |

Classes: `tidy`, `tidyNode`.

Constants: 161 constants: `TIDY_*` (161).

<!-- elephc:generated:symbols:end -->
