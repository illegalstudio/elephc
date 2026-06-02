---
title: "System & I/O"
description: "System functions, file I/O, date/time, JSON, and debugging utilities."
sidebar:
  order: 12
---

## System functions

| Function | Signature | Description |
|---|---|---|
| `exit()` | `exit($code = 0): void` | Terminate program |
| `die()` | `die($code = 0): void` | Alias for `exit()` |
| `time()` | `time(): int` | Unix timestamp |
| `microtime()` | `microtime($as_float = false): float` | Time with microsecond precision |
| `sleep()` | `sleep($seconds): int` | Sleep for seconds |
| `usleep()` | `usleep($microseconds): void` | Sleep for microseconds |
| `getenv()` | `getenv($name): string` | Get environment variable |
| `putenv()` | `putenv($assignment): bool` | Set environment variable ("KEY=VALUE") |
| `define()` | `define($name, $value): bool` | Define a compile-time global constant with a string-literal name |
| `defined()` | `defined($name): bool` | Check whether a string-literal constant name is defined |
| `php_uname()` | `php_uname($mode = "a"): string` | Get system information from the target runtime |
| `phpversion()` | `phpversion(): string` | Get the elephc package version from `Cargo.toml` |
| `exec()` | `exec($command): string` | Execute command, return output |
| `shell_exec()` | `shell_exec($command): string` | Execute via shell, return output |
| `system()` | `system($command): string` | Execute, output to stdout |
| `passthru()` | `passthru($command): void` | Execute, pass raw output |

`define()` returns `true` the first time a constant is defined at runtime. Duplicate attempts keep the first value, return `false`, and emit a suppressible runtime warning. `defined()` currently requires a string literal in AOT mode.

`php_uname()` supports PHP's standard one-character modes:

| Mode | Result |
|---|---|
| `"a"` | Full system line: system name, node name, release, version, machine |
| `"s"` | System name, matching `PHP_OS` (`"Darwin"` on macOS targets, `"Linux"` on Linux targets) |
| `"n"` | Network node name |
| `"r"` | Release |
| `"v"` | Version |
| `"m"` | Machine hardware name |

## Date and time

| Function | Signature | Description |
|---|---|---|
| `date()` | `date($format [, $timestamp]): string` | Format timestamp. Chars: Y, m, d, H, i, s, l, F, D, M, N, j, n, G, g, A, a, U. |
| `mktime()` | `mktime($h, $m, $s, $mon, $day, $yr): int` | Create timestamp from components |
| `strtotime()` | `strtotime($datetime): int` | Parse a date/time string into a Unix timestamp. Supports ISO dates, time-only, relative offsets, named weekdays, and bare keywords. Returns `-1` on failure. |

`strtotime()` accepts the following shapes (input is case-insensitive for keywords/unit names/weekday names, and leading/trailing ASCII whitespace is trimmed):

- **ISO date / datetime** — `YYYY-MM-DD`, `YYYY-MM-DD HH:MM`, `YYYY-MM-DD HH:MM:SS`, `YYYY-MM-DDTHH:MM`, or `YYYY-MM-DDTHH:MM:SS`. Lowercase `t` is also accepted as the date/time separator.
- **Bare keywords** — `now`, `today`, `tomorrow`, `yesterday`, `midnight`, `noon`. (`midnight` is an alias for `today`.)
- **Time-only** — `H:MM`, `HH:MM`, `H:MM:SS`, `HH:MM:SS` — combined with today's date.
- **Relative offsets** — `[+-]?N unit [N unit ...]`, `a/an unit`, and `N unit ago` / `a/an unit ago` (negates the whole expression). Units: `sec(s)`, `second(s)`, `min(s)`, `minute(s)`, `hour(s)`, `day(s)`, `week(s)`, `month(s)`, `year(s)`. Composite forms like `"+1 day 2 hours"`, `"an hour"`, and `"a day ago"` are supported. Day/week offsets honor DST through libc `mktime` normalization.
- **Named weekdays** — `Monday`..`Sunday` and 3-letter abbreviations `Mon`..`Sun`. Modifiers: `next <weekday>` (next future occurrence; today + 7 if today matches), `last <weekday>` (most recent past; today - 7 if today matches), `this <weekday>` (delta may be zero when today matches). Result is midnight of the target day.

Currently out of scope (not accepted): timezone offsets (`+0200`, `UTC`, ...), `@unix_timestamp` form, `first/last day of` patterns, `MM/DD/YYYY` and `DD-Mon-YYYY` alternative date shapes, `nth <weekday> of <month>` patterns. Malformed input returns `-1`.

## JSON

| Function | Signature | Description |
|---|---|---|
| `json_encode()` | `json_encode($value, $flags = 0, $depth = 512): string\|false` | Encode as JSON. Supports int, float, string, bool, null, arrays, mixed payloads, and objects (public properties + `JsonSerializable::jsonSerialize()` dispatch). Multibyte UTF-8 characters are escaped to `\uXXXX` by default (and to surrogate pairs for codepoints ≥ U+10000). `$flags` observes `JSON_UNESCAPED_SLASHES`, `JSON_UNESCAPED_UNICODE`, `JSON_PRETTY_PRINT`, `JSON_FORCE_OBJECT`, `JSON_NUMERIC_CHECK`, `JSON_PRESERVE_ZERO_FRACTION`, `JSON_HEX_TAG`, `JSON_HEX_AMP`, `JSON_HEX_APOS`, `JSON_HEX_QUOT`, `JSON_PARTIAL_OUTPUT_ON_ERROR`, and `JSON_THROW_ON_ERROR` (Inf/NaN trigger `JSON_ERROR_INF_OR_NAN`, and `$depth` overrun triggers `JSON_ERROR_DEPTH`; the throw flag promotes both to `JsonException`, while partial-output keeps the substituted JSON string). `$depth` defaults to 512 and is enforced for every container encoder (assoc arrays, indexed arrays, objects). The remaining `JSON_INVALID_UTF8_*` flags are accepted and observed for malformed UTF-8 strings. |
| `json_decode()` | `json_decode($json, $associative = null, $depth = 512, $flags = 0): mixed` | Full structural decoder. Returns a boxed `Mixed` cell whose runtime tag matches the decoded JSON value (null/bool/int/float/string/array/object). For JSON objects, `$associative` selects the shape: the PHP default (`null`/`false`) returns a `stdClass` instance whose properties are accessible with `$obj->name`; `true` returns an associative array indexable with `$obj["name"]`. Property access on the decoded `Mixed` is supported directly — codegen unboxes the cell, checks the stdClass class_id, and routes through the dynamic-property hash. `$depth` is enforced (`JSON_ERROR_DEPTH` on overflow). Failed decodes record PHP 8.6-style one-based line/column data for `json_last_error_msg()` and `JSON_THROW_ON_ERROR`. `$flags` observes `JSON_THROW_ON_ERROR` (raises `JsonException` on syntax/depth failure) and `JSON_BIGINT_AS_STRING` (integer tokens overflowing PHP_INT return as preserved-digit strings instead of wrapping through `__rt_atoi`). |
| `json_last_error()` | `json_last_error(): int` | Returns the runtime's last JSON error code (`JSON_ERROR_*`). |
| `json_last_error_msg()` | `json_last_error_msg(): string` | Returns the PHP-compatible message for `json_last_error()` (e.g. `"No error"`, `"Syntax error"`). After `json_decode()` failures, syntax/control-character/depth/UTF-8/UTF-16 messages include a `" near location line:column"` suffix while numeric error codes stay unchanged. |
| `json_validate()` | `json_validate($json, $depth = 512, $flags = 0): bool` | RFC 8259 validator. Returns whether `$json` is syntactically valid, sets `json_last_error()` on failure, and accepts only `0` or `JSON_INVALID_UTF8_IGNORE` for `$flags` (matching PHP 8.3). |

### Constants

The full PHP `JSON_*` family is exposed and can be combined with the bitwise OR operator to build flag arguments.

| Encoding flags | Value | Decoding flags | Value |
|---|---|---|---|
| `JSON_HEX_TAG` | 1 | `JSON_OBJECT_AS_ARRAY` | 1 |
| `JSON_HEX_AMP` | 2 | `JSON_BIGINT_AS_STRING` | 2 |
| `JSON_HEX_APOS` | 4 | | |
| `JSON_HEX_QUOT` | 8 | | |
| `JSON_FORCE_OBJECT` | 16 | | |
| `JSON_NUMERIC_CHECK` | 32 | | |
| `JSON_UNESCAPED_SLASHES` | 64 | | |
| `JSON_PRETTY_PRINT` | 128 | | |
| `JSON_UNESCAPED_UNICODE` | 256 | | |
| `JSON_PARTIAL_OUTPUT_ON_ERROR` | 512 | | |
| `JSON_PRESERVE_ZERO_FRACTION` | 1024 | | |
| `JSON_INVALID_UTF8_IGNORE` | 1048576 | | |
| `JSON_INVALID_UTF8_SUBSTITUTE` | 2097152 | | |
| `JSON_THROW_ON_ERROR` | 4194304 | | |

| Error code | Value | Error code | Value |
|---|---|---|---|
| `JSON_ERROR_NONE` | 0 | `JSON_ERROR_RECURSION` | 6 |
| `JSON_ERROR_DEPTH` | 1 | `JSON_ERROR_INF_OR_NAN` | 7 |
| `JSON_ERROR_STATE_MISMATCH` | 2 | `JSON_ERROR_UNSUPPORTED_TYPE` | 8 |
| `JSON_ERROR_CTRL_CHAR` | 3 | `JSON_ERROR_INVALID_PROPERTY_NAME` | 9 |
| `JSON_ERROR_SYNTAX` | 4 | `JSON_ERROR_UTF16` | 10 |
| `JSON_ERROR_UTF8` | 5 | | |

### Classes and interfaces

| Symbol | Kind | Description |
|---|---|---|
| `JsonSerializable` | Interface | Implementing classes can override `jsonSerialize(): mixed`; `json_encode()` dispatches to it instead of walking public properties. |
| `Error` | Class | Base PHP error throwable with `message: string`, `code: int`, `__construct(string $message = "", int $code = 0)`, and the standard `Throwable` methods. `FiberError` extends this class. |
| `Exception` | Class | Base PHP exception with `message: string`, `code: int`, `__construct(string $message = "", int $code = 0)`, and the standard `Throwable` methods. |
| `RuntimeException` | Class | `extends Exception`. Standard PHP "runtime errors" base class. |
| `JsonException` | Class | `extends RuntimeException`. Carries the originating `JSON_ERROR_*` code; `getCode()` returns it (e.g. 4 = SYNTAX, 1 = DEPTH, 10 = UTF16, 7 = INF_OR_NAN). |
| `stdClass` | Class | Dynamic-property container. `$obj = new stdClass(); $obj->name = "x";` works for any property name; storage is a backing hash on the instance. `json_decode($json)` returns stdClass by default (PHP semantics); pass `assoc: true` to get an associative array. |

Encoding rules for objects:

- Classes that **implement `JsonSerializable`** dispatch to `$this->jsonSerialize()` and the returned value is encoded recursively.
- Classes that **do not** implement `JsonSerializable` are encoded as a JSON object whose keys are the **public** properties (private and protected properties are skipped), in declaration order, including inherited public properties.

### Current limitations

- `json_decode()` is a full checked structural decoder: every JSON value type round-trips through a real recursive-descent parser into a boxed `Mixed` cell, and malformed input records the JSON error inside the decode walk instead of running a separate full-buffer validation pass first. Decode failures also record the source offset and format the PHP 8.6 location suffix (`near location line:column`) for `json_last_error_msg()` and `JsonException` messages without changing `json_last_error()` codes. `null` → `Mixed(null)`, `true`/`false` → `Mixed(bool)`, integers → `Mixed(int)`, floats → `Mixed(float)`, strings → `Mixed(str)` with full escape decoding (`\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uXXXX` including surrogate pairs), arrays → `Mixed(array<Mixed>)` with each element recursively decoded, and objects → either a `stdClass` instance (PHP default, `assoc=false`/`null`) or `Mixed(assoc)` (`assoc=true`). The associativity flag is threaded through `_json_decode_assoc` so nested objects share the caller's choice. Container parsing uses a depth-and-string-aware boundary scanner so commas and brackets inside string values never confuse the element/pair detection. Property access (`$obj->name`), `[]` indexing (`$arr["k"]`, `$arr[0]`), and `count()` all work directly on Mixed-typed `json_decode` results: codegen routes through `__rt_mixed_property_get` / `__rt_mixed_array_get` / `__rt_mixed_count` which unbox the cell, dispatch by runtime tag (indexed array / assoc / stdClass), and re-box typed payloads back into a Mixed cell. Missing keys, out-of-bounds indices, and unknown properties all return `Mixed(null)` instead of erroring, mirroring PHP's quiet "undefined index" / "property on non-object" warnings. Use `intval()` / `floatval()` or explicit `(int)` / `(float)` / `(string)` casts to lift a `Mixed` payload back to a typed value before arithmetic since elephc's type system requires numeric operands for `+` and Mixed alone does not satisfy the contract.
- `json_encode()` observes `JSON_UNESCAPED_SLASHES` (default escapes `/` as `\/`), `JSON_UNESCAPED_UNICODE` (default escapes multibyte UTF-8 to `\uXXXX`, surrogate pairs for codepoints ≥ U+10000), `JSON_PRETTY_PRINT` (4-space indentation, newlines between elements, single space after `:`), `JSON_FORCE_OBJECT` (indexed arrays encode as `{"0":val,"1":val,...}`), `JSON_NUMERIC_CHECK` (numeric-looking strings encode as raw JSON numbers per RFC 8259 grammar), `JSON_PRESERVE_ZERO_FRACTION` (integer-valued floats stay `1.0` instead of collapsing to `1`), the full `JSON_HEX_TAG/AMP/APOS/QUOT` family (replaces `<`/`>`, `&`, `'`, `"` with their `\uXXXX` form), Inf/NaN detection (sets `JSON_ERROR_INF_OR_NAN`; under `JSON_THROW_ON_ERROR` raises `JsonException`, otherwise returns `false` unless `JSON_PARTIAL_OUTPUT_ON_ERROR` is set), and **malformed UTF-8 detection**: every multibyte byte is validated (lead-byte range, continuation bytes, truncated sequences). Without sanitization flags this sets `JSON_ERROR_UTF8` and returns `false`; `JSON_INVALID_UTF8_IGNORE` drops malformed bytes silently without raising the error code; `JSON_INVALID_UTF8_SUBSTITUTE` replaces malformed bytes with `�` (or the U+FFFD UTF-8 bytes when `JSON_UNESCAPED_UNICODE` is also set). `JSON_PARTIAL_OUTPUT_ON_ERROR` keeps the partial output for errors that can be substituted.

- `JSON_THROW_ON_ERROR` is observed by `json_encode()` for non-finite floats (Inf/NaN trigger `JSON_ERROR_INF_OR_NAN`), for malformed UTF-8 input (`JSON_ERROR_UTF8`), and by `json_decode()` (`JSON_ERROR_SYNTAX`, `JSON_ERROR_DEPTH`, `JSON_ERROR_UTF16`). Decode exceptions use the same location-aware message string as `json_last_error_msg()` when the failing byte offset is known. PHP does not allow this flag for `json_validate()`; elephc rejects it at compile time when the flag expression is static. The throw helper records the error code in `_json_last_error` so `json_last_error()` / `json_last_error_msg()` keep working when the flag is clear.
- `JSON_ERROR_UTF16` is set by `json_decode()` and `json_validate()` whenever a `\uXXXX` escape in the high-surrogate range (`0xD800..0xDBFF`) is not immediately followed by a low-surrogate `\uYYYY` (`0xDC00..0xDFFF`), or when a low surrogate appears without a preceding high surrogate. The detector walks the surrogate-pair handshake byte by byte, so any malformed second escape (truncated `\u`, non-hex digit, or out-of-range codepoint) routes to UTF16 instead of SYNTAX, matching PHP's exact behavior.
- The `$depth` argument is observed by all three JSON entry points but with the PHP-faithful split: `json_encode()` allows up to `$depth` levels of nesting (`active <= limit`), while `json_decode()` and `json_validate()` reject when the active nesting depth reaches `$depth` (`active >= limit`). For example, `json_decode("[1]", false, 1)` sets `JSON_ERROR_DEPTH` even though the input only nests one level deep, matching PHP. For `json_encode()` and `json_decode()`, `JSON_THROW_ON_ERROR` promotes the error to `JsonException`.
- `JSON_BIGINT_AS_STRING` is observed by `json_decode()`. When set, integer-grammar JSON tokens (no `.`, no `e`/`E`) whose magnitude exceeds `PHP_INT_MAX` (`9223372036854775807`) are returned as a `Mixed(string)` preserving the original digits; in-range integers and any token containing `.`/`e`/`E` are unaffected. Detection is a length-then-lex compare against the threshold strings `9223372036854775807` (positive) / `-9223372036854775808` (negative), which is safe because the fused number validator rejects RFC 8259 leading zeros — equal-length leading-zero-free decimal strings compare lexicographically the same as numerically. The flag threads through nested arrays and objects via the global `_json_active_flags` slot, so a bigint inside a decoded array is also returned as a string.
- `json_validate()` is a recursive-descent RFC 8259 validator: it matches the literals `null`/`true`/`false`, validates the full number grammar (`-?(0|[1-9][0-9]*)(.[0-9]+)?([eE][+-]?[0-9]+)?`), checks every string escape (`\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uHHHH` with four hex digits), verifies bracket pairing in arrays/objects, requires colons between keys and values, and rejects trailing content after the value. Recursion depth is enforced against the `$depth` argument (default 512); on overflow it records `JSON_ERROR_DEPTH`. Every other malformed token records `JSON_ERROR_SYNTAX`.
- `catch (Throwable $e)` supports dispatch to the standard `Throwable` method surface: `getMessage()`, `getCode()`, `getFile()`, `getLine()`, `getTrace()`, `getTraceAsString()`, `getPrevious()`, and `__toString()`.
- Associative arrays whose keys form a sequential `0..count-1` sequence in insertion order encode as JSON arrays (`[...]`) — matching PHP's runtime detection. `__rt_json_encode_assoc` tracks that shape during the main hash walk, emits a provisional object form, and compacts the finished buffer in-place to array form only when every key matched. `JSON_FORCE_OBJECT` disables compaction so that flag still wins. Empty associative arrays also encode as `[]` (PHP's `json_encode([])` semantics).
- JSON helpers are emitted through the shared runtime surface on every supported target. Structural decode into `Mixed`, stdClass dynamic-property helpers, JsonSerializable-aware object encoding, validation, pretty-printing, depth tracking, and JSON error-message lookup are all part of that target-aware runtime path.

## Regex

Regex functions and SPL regex iterators are documented in [Regex](regex.md),
including the PCRE2 native library requirements for compiling programs that use
them.

## File I/O

| Function | Signature | Description |
|---|---|---|
| `fopen()` | `fopen($filename, $mode, $use_include_path = false, $context = null): resource\|false` | Open file (modes: r, w, a, r+, w+, a+), or `false` on failure. The 3rd/4th args are evaluated in source order before opening, but the active context still comes from the global stream-context slot. |
| `fclose()` | `fclose(resource $handle): bool` | Close file handle |
| `fread()` | `fread(resource $handle, $length): string` | Read up to $length bytes |
| `fwrite()` | `fwrite(resource $handle, $data): int` | Write to file |
| `fprintf()` | `fprintf(resource $handle, string $format, ...$values): int` | Format like `sprintf()` and write the result to the stream, returning the number of bytes written. Honors any attached write filter. |
| `vfprintf()` | `vfprintf(resource $handle, string $format, array $values): int` | Like `fprintf()`, with the arguments supplied as an array. Writes the formatted result to the stream (honoring any write filter / userspace wrapper) and returns the byte count. |
| `fscanf()` | `fscanf(resource $handle, string $format): array` | Read one line from the stream and parse it with the `sscanf()` engine, returning the matched fields as an array. Works on registered userspace-wrapper handles (the line is read through the wrapper's `stream_read`). v1 supports the 2-argument array-returning form (the by-ref output-variable form is not yet supported, mirroring `sscanf()`). Supported conversions: `%d`, `%f`, `%s`, `%%`. |
| `fgets()` | `fgets(resource $handle, int $length = null): string\|false` | Read a line (up to `$length` bytes, or until newline / EOF). Returns `false` at EOF so the canonical PHP idiom `while (($line = fgets($f)) !== false)` terminates. |
| `feof()` | `feof(resource $handle): bool` | End-of-file check |
| `readline()` | `readline([$prompt]): string` | Read line from STDIN |
| `fseek()` | `fseek(resource $handle, $offset [, $whence]): int` | Seek in file |
| `ftell()` | `ftell(resource $handle): int` | Current position |
| `rewind()` | `rewind(resource $handle): bool` | Seek to beginning |
| `fgetcsv()` | `fgetcsv(resource $handle [, $sep]): array` | Read CSV line. Works on registered userspace-wrapper handles (the line is read through the wrapper's `stream_read`). |
| `fputcsv()` | `fputcsv(resource $handle, $fields [, $sep]): int` | Write CSV line. Works on registered userspace-wrapper handles (each field/separator/quote/newline segment is written through the wrapper's `stream_write`). |
| `fgetc()` | `fgetc(resource $handle): string\|false` | Read one byte, or `false` at EOF/failure |
| `readfile()` | `readfile($filename): int\|false` | Stream a file to stdout, return bytes copied, `-1` on read failure, or `false` on open failure. A `scheme://...` path matching a registered userspace wrapper is streamed through the wrapper (`fopen` + `stream_read` drain + `stream_close`). |
| `fpassthru()` | `fpassthru(resource $handle): int` | Stream remaining bytes of an open handle to stdout, returning `-1` on read failure |
| `stream_get_contents()` | `stream_get_contents(resource $handle): string` | Read every remaining byte of an open stream into a string, starting at the current position. The optional `$length` and `$offset` parameters are not yet supported. |
| `stream_copy_to_stream()` | `stream_copy_to_stream(resource $from, resource $to): int` | Copy every remaining byte from `$from` to `$to`, returning the number of bytes copied. The optional `$length` and `$offset` parameters are not yet supported. |
| `stream_get_line()` | `stream_get_line(resource $handle, int $length [, string $ending]): string` | Read up to `$length` bytes, stopping early at the `$ending` delimiter (which is consumed and stripped). Returns the bytes read; reaching EOF sets the handle's end-of-file flag. Works on registered userspace-wrapper handles (bytes are read through the wrapper's `stream_read`). |
| `flock()` | `flock(resource $handle, int $op, &$would_block = null): bool` | Advisory lock. Combine `LOCK_SH` (1), `LOCK_EX` (2), or `LOCK_UN` (3) with the optional `LOCK_NB` (4) flag. `$would_block` is set to `0` or `1` when provided. On a registered userspace-wrapper handle the call dispatches to the wrapper's `stream_lock(int $operation)` (the `$op` value is threaded through), returning its `bool` result; a wrapper without `stream_lock` returns `false` and `$would_block` is not populated. |
| `tmpfile()` | `tmpfile(): resource\|false` | Create a `/tmp/elephc-XXXXXX` temp file via `mkstemp`, immediately `unlink` the path so the file disappears when the descriptor closes. Returns a stream `resource` on success, or `false` on failure. |

File handles are PHP `resource` values, not integers. `gettype(fopen(...))` returns `"resource"` on success and `"boolean"` on failure, `gettype(STDIN)` returns `"resource"`, and passing a plain `int` to stream functions is rejected. Failed `fopen()` calls, including invalid or empty modes, emit a suppressible runtime warning and return `false`; passing that `false` to stream functions is a fatal runtime TypeError with PHP-style "false given" wording, matching PHP's guard-before-use pattern.

The `php://` wrapper exposes the process's standard streams through `fopen()`: `php://stdin` and `php://input` open descriptor 0, `php://stdout` and `php://output` open descriptor 1, and `php://stderr` opens descriptor 2. `php://memory` and `php://temp` open an in-memory stream — a real, seekable handle backed by an anonymous temporary buffer that is never linked into the filesystem; every stream function (`fread`, `fwrite`, `fseek`, `ftell`, `rewind`, `feof`, `stream_get_contents`, …) works on it unchanged. `php://temp`'s optional `/maxmemory:N` suffix is accepted and ignored. The path must be a string literal.

The `php://filter` wrapper opens an underlying resource and attaches a built-in filter to it: `fopen("php://filter/read=string.toupper/resource=<path>", "r")` opens `<path>` (any path or stream `fopen()` understands, e.g. a file or `php://temp`) and applies `string.toupper` on read. Use `write=` for the write direction, or a bare filter name (`php://filter/string.rot13/resource=…`) to apply it to both directions. The filter name maps to the same built-in set as `stream_filter_append()` (`string.toupper`, `string.tolower`, `string.rot13`, …); an unrecognized filter still returns the (unfiltered) stream, matching PHP. The URL must be a string literal. Limitations: filters apply through `fread()`/`fwrite()` only — `stream_get_contents()`/`fgets()` read past the filter (a pre-existing single-filter-model limitation) — and a chained `read=F1|F2` list keeps only the first filter.

The `data://` wrapper (RFC 2397) opens a read-only stream over an inline payload. `fopen("data://text/plain;base64,SGVsbG8=", "r")` base64-decodes the body, while `fopen("data://text/plain,Hello%20world", "r")` percent-decodes it (`%HH` escapes, and `+` becomes a space). The decoded bytes back a real, seekable descriptor, so every stream function works on the result. The URI is decoded at compile time and must be a string literal; an unparseable URI returns `false`.

The `phar://` wrapper opens a read-only stream over a single entry inside a PHAR archive: `fopen("phar://path/to/app.phar/dir/file.txt", "r")` reads `dir/file.txt` out of `app.phar`. Like `data://`, the archive is read and parsed **at compile time** — the entry's bytes are embedded in the compiled binary and back a real, seekable descriptor, so every stream function works and the resulting binary needs no archive file at run time. Uncompressed, **gzip-compressed** (PHP stores gzip entries as raw DEFLATE), and **bzip2-compressed** entries are all supported; compressed entries are decompressed at compile time. The archive path resolves against the compiler's working directory (or give an absolute path). A missing archive or a missing entry returns `false`.

`file_get_contents("phar://app.phar/entry.txt")` of a literal URL works the same way — the entry is decoded at compile time and returned as a string (a missing archive or entry returns `false`).

When the archive path is **not** a compile-time string literal (e.g. `fopen("phar://".$path."/entry.txt", "r")` or `file_get_contents("phar://".$path."/entry.txt")`), the entry is read and parsed **at run time** instead: the archive is opened, its manifest walked, and the matched **uncompressed** entry materialized as a readable stream — so a program can read a phar it wrote earlier in the same run, or read an archive whose path is only known at run time. `file_get_contents()` shares this path: a non-literal `phar://` URL opens the runtime stream, slurps it with `stream_get_contents`, and returns the bytes as a string (a missing archive or entry returns `false`). Runtime reads split the archive from the entry at the `.phar/` boundary (the archive's name must contain `.phar/`), and gzip/bzip2 entries on the runtime path are not yet supported (use a literal URL for those, which decompresses at compile time).

Writing a single uncompressed entry is also supported: `fopen("phar://archive.phar/entry", "w")` returns a stream whose `fwrite()` calls are buffered in memory, and `fclose()` assembles a native PHAR (PHP stub + one-file manifest + the written bytes, with the manifest's size and CRC-32 fields filled in) and writes it to `archive.phar`. The archive is **signed**: the manifest sets the `PHAR_HDR_SIGNATURE` flag and `fclose()` appends a SHA1 signature trailer (`raw-sha1(20 bytes) ++ LE32(0x0002) ++ "GBMB"`) computed over the whole archive, so **real PHP accepts it** (PHP requires a hash by default via `phar.require_hash`), not just elephc. `file_put_contents("phar://archive.phar/entry", $data)` is the one-call equivalent and produces the same signed archive. The archive path and entry name are resolved at compile time from the literal URL. Because reads happen at compile time but writes happen at run time, a program cannot write an entry and read it back through `phar://` in the same run — the reader parses the archive when *that* program is compiled, before the writer has produced it; read a written archive back in a separate compilation (or with real PHP). Current limits (Milestone-1): one phar-write stream open at a time, the entry is stored uncompressed, the content must fit the in-memory buffer, and the key/private-key signing variants (OpenSSL) are not supported — only SHA1. The tar/zip PHAR variants and runtime (non-literal) archive paths are not yet supported.

The `ftp://` wrapper retrieves a file from an FTP server: `fopen("ftp://host[:port]/path", "r")` connects to the server (port `21` by default), logs in anonymously, switches to binary passive mode, and issues `RETR`, returning the data connection as a readable stream. The URL must be a string literal. v1 is read-only and logs in anonymously, so any `user:pass@` credentials in the URL are ignored; an unparseable URL returns `false`.

The `http://` wrapper fetches a URL over HTTP: `fopen("http://host[:port]/path", "r")` connects to the server (port `80` by default), issues an `HTTP/1.0` `GET`, and returns the response body — with the headers stripped — as a readable stream. The URL must be a string literal. v1 sends a plain anonymous `GET` (any `user@` userinfo is ignored), buffers up to 1 MiB of response, and does not follow redirects; an unparseable URL or a failed connection returns `false`.

The `https://` wrapper does the same thing over TLS: `fopen("https://host[:port]/path", "r")` performs a TLS handshake (port `443` by default), sends the `HTTP/1.0` `GET`, and exposes the decrypted response body as a readable stream. The TLS layer is provided by the `elephc-tls` staticlib (rustls + the `ring` crypto provider, with the Mozilla webpki-roots trust store); programs that use `https://` automatically link `-lelephc_tls`, leaving programs that do not pay no extra cost. Same v1 limits as `http://`: anonymous `GET`, 1 MiB cap, no redirects, string-literal URL.

Several `ssl` stream-context options influence the TLS handshake (set them with `stream_context_set_option(stream_context_get_default(), "ssl", "<option>", "<value>")` before `fopen("https://…")`):

- `ssl.cafile` — a PEM bundle that replaces the built-in Mozilla trust store with a custom set of CA certificates (for servers signed by a private CA). A `cafile` that cannot be read (missing, or containing no certificates) fails the open and returns `false`.
- `ssl.capath` — a **directory** of PEM CA-certificate files; every certificate found in the directory becomes a trust anchor. A missing or certificate-less directory fails the open.
- `ssl.peer_name` — verify the server certificate (and send SNI) for this name instead of the connection host, for cases where you connect to one address but the certificate is issued for a different hostname.
- `ssl.verify_peer = "0"` (or `false`), `ssl.allow_self_signed`, and `ssl.verify_peer_name = "0"` — relax peer authentication: the channel stays encrypted but the peer identity is not verified. elephc treats all three as the same relaxed mode (it does not distinguish self-signed acceptance from a full identity skip).

Dispatch priority when several are set: `cafile` → `capath` → relaxed (`verify_peer=0` / `allow_self_signed` / `verify_peer_name=0`) → `peer_name` → the default Mozilla trust store. Client certificates (`local_cert`/`local_pk`) and `ciphers`/`security_level` remain unsupported.

## File system

| Function | Signature | Description |
|---|---|---|
| `file_get_contents()` | `file_get_contents($filename): string\|false` | Read entire file, or `false` if the file cannot be opened |
| `file_put_contents()` | `file_put_contents($filename, $data): int` | Write file |
| `file()` | `file($filename): array` | Read into array of lines |
| `file_exists()` | `file_exists($filename): bool` | Check exists |
| `is_file()` | `is_file($filename): bool` | Is regular file |
| `is_dir()` | `is_dir($filename): bool` | Is directory |
| `is_readable()` | `is_readable($filename): bool` | Is readable |
| `is_writable()` | `is_writable($filename): bool` | Is writable |
| `filesize()` | `filesize($filename): int` | File size in bytes |
| `filemtime()` | `filemtime($filename): int` | Modification time |
| `disk_free_space()` | `disk_free_space($directory): float` | Free bytes of the filesystem holding `$directory`; `0.0` on failure |
| `disk_total_space()` | `disk_total_space($directory): float` | Total bytes of the filesystem holding `$directory`; `0.0` on failure |
| `copy()` | `copy($source, $dest): bool` | Copy file |
| `rename()` | `rename($old, $new): bool` | Rename/move |
| `unlink()` | `unlink($filename): bool` | Delete file |
| `mkdir()` | `mkdir($pathname): bool` | Create directory |
| `rmdir()` | `rmdir($pathname): bool` | Remove directory |
| `scandir()` | `scandir($directory): array` | List files |
| `opendir()` | `opendir($directory): resource\|false` | Open a directory stream for iteration with `readdir()`; returns a stream resource, or `false` on failure |
| `readdir()` | `readdir($dir_handle): string\|false` | Read the next entry name from a directory handle (including `.` and `..`); returns `false` once every entry has been read |
| `closedir()` | `closedir($dir_handle): void` | Close a directory handle opened by `opendir()` |
| `rewinddir()` | `rewinddir($dir_handle): void` | Rewind a directory handle back to its first entry |
| `glob()` | `glob($pattern): array` | Find matching files |
| `getcwd()` | `getcwd(): string` | Current working directory |
| `chdir()` | `chdir($directory): bool` | Change directory |
| `tempnam()` | `tempnam($dir, $prefix): string` | Create temp filename |
| `sys_get_temp_dir()` | `sys_get_temp_dir(): string` | System temp directory |

## Symbolic links

| Function | Signature | Description |
|---|---|---|
| `symlink()` | `symlink($target, $link): bool` | Create a symbolic link at `$link` pointing at `$target`. |
| `link()` | `link($target, $link): bool` | Create a hard link `$link` for an existing path `$target`. |
| `readlink()` | `readlink($path): string\|false` | Read the target of a symbolic link. Returns `false` on failure. |
| `linkinfo()` | `linkinfo($path): int` | Returns the device id (`st_dev`) of the link, or `-1` on failure. |

## File metadata

| Function | Signature | Description |
|---|---|---|
| `fileatime()` | `fileatime($filename): int\|false` | Last access time as Unix timestamp, or `false` on failure |
| `filectime()` | `filectime($filename): int\|false` | Inode-change time as Unix timestamp, or `false` on failure |
| `fileperms()` | `fileperms($filename): int\|false` | Full `st_mode` (file-type bits + permissions), or `false` on failure |
| `fileowner()` | `fileowner($filename): int\|false` | Owner UID, or `false` on failure |
| `filegroup()` | `filegroup($filename): int\|false` | Group GID, or `false` on failure |
| `fileinode()` | `fileinode($filename): int\|false` | Inode number, or `false` on failure |
| `filetype()` | `filetype($filename): string\|false` | One of `"file"`, `"dir"`, `"link"`, `"char"`, `"block"`, `"fifo"`, `"socket"`, `"unknown"` for stated paths, or `false` on `lstat()` failure. Uses `lstat()` semantics. |
| `is_executable()` | `is_executable($filename): bool` | `access(path, X_OK)` |
| `is_link()` | `is_link($filename): bool` | True for symlinks (uses `lstat()`) |
| `is_writeable()` | `is_writeable($filename): bool` | Alias of `is_writable()` |
| `stat()` | `stat($filename): array\|false` | Associative array with both numeric (0..=12) and string keys (`dev`, `ino`, `mode`, `nlink`, `uid`, `gid`, `rdev`, `size`, `atime`, `mtime`, `ctime`, `blksize`, `blocks`), or `false` on failure. |
| `lstat()` | `lstat($filename): array\|false` | Same shape as `stat()` but does not follow symlinks, or `false` on failure |
| `fstat()` | `fstat(resource $handle): array\|false` | Same shape as `stat()` but operates on an open stream resource, or `false` on failure |
| `clearstatcache()` | `clearstatcache($clear_realpath_cache = false, $filename = ""): void` | No-op (elephc does not cache `stat()` results). Arguments are still evaluated. |

> The 13 `stat()` / `lstat()` / `fstat()` fields are inserted in PHP's documented order. Check the return value against `false` before reading fields when the path or stream may be invalid.

## Path manipulation

| Function | Signature | Description |
|---|---|---|
| `basename()` | `basename($path [, $suffix]): string` | Trailing name component. `$suffix` is trimmed when it is a strict suffix of the result. |
| `dirname()` | `dirname($path [, $levels = 1]): string` | Parent directory. Repeats the parent lookup when `$levels` is greater than 1. |
| `pathinfo()` | `pathinfo($path [, $flag]): array\|string` | Without a flag, or with `PATHINFO_ALL`: associative array with keys `dirname`, `basename`, `extension` (when the basename contains a dot), `filename`. With component flags (`DIRNAME`, `BASENAME`, `EXTENSION`, `FILENAME`): the corresponding string. Runtime-computed flags are supported. |
| `realpath()` | `realpath($path): string\|false` | Canonicalized absolute path, or `false` when the path does not exist. |
| `fnmatch()` | `fnmatch($pattern, $filename [, $flags = 0]): bool` | Shell-glob match. Supports `*`, `?`, `[abc]`, `[a-z]`, `[!abc]`/`[^abc]`, `\\<char>`, and PHP flags. |

> `pathinfo()` accepts `PATHINFO_DIRNAME` (1), `PATHINFO_BASENAME` (2), `PATHINFO_EXTENSION` (4), `PATHINFO_FILENAME` (8), and `PATHINFO_ALL` (15) constants, integer literals, variables, and bitmasks such as `PATHINFO_DIRNAME | PATHINFO_EXTENSION`. Component bitmasks follow PHP priority: dirname, basename, extension, then filename. The component-flag form returns the requested component as a string (or empty string when it is absent, for example `pathinfo("foo", PATHINFO_EXTENSION)` returns `""`). The no-flag and exact `PATHINFO_ALL` forms return an associative array; the `extension` key is omitted only when the basename has no dot, matching PHP's behaviour.

> `fnmatch()` supports PHP's `FNM_NOESCAPE`, `FNM_PATHNAME`, `FNM_PERIOD`, and `FNM_CASEFOLD` flags, including runtime-computed bitmasks such as `FNM_PATHNAME | FNM_CASEFOLD`. The numeric values are target-specific and follow the selected platform's PHP/libc constants.

## File modification

| Function | Signature | Description |
|---|---|---|
| `touch()` | `touch($filename [, $mtime [, $atime]]): bool` | Set access/modification times. Creates the file with permissions `0666 & umask` if missing. With no `$mtime`, or `$mtime = null`, uses the current time; with no `$atime`, or `$atime = null`, defaults to `$mtime`. On a registered `scheme://` path it dispatches to the wrapper's `stream_metadata($path, STREAM_META_TOUCH, [$mtime, $atime])` with a 2-element int array. |
| `chmod()` | `chmod($filename, $mode): bool` | Change file mode. On a registered `scheme://` path it dispatches to the wrapper's `stream_metadata($path, STREAM_META_ACCESS, $mode)` and returns its `bool` result (false when the wrapper does not implement `stream_metadata`). |
| `chown()` | `chown($filename, $user): bool` | Change owner by UID or user name. The group is left unchanged. On a registered `scheme://` path it dispatches to the wrapper's `stream_metadata($path, STREAM_META_OWNER, $uid)` (integer `$user`) or `stream_metadata($path, STREAM_META_OWNER_NAME, $name)` (string `$user`). |
| `chgrp()` | `chgrp($filename, $group): bool` | Change group by GID or group name. The owner is left unchanged. On a registered `scheme://` path it dispatches to the wrapper's `stream_metadata($path, STREAM_META_GROUP, $gid)` (integer `$group`) or `stream_metadata($path, STREAM_META_GROUP_NAME, $name)` (string `$group`). |
| `umask()` | `umask([$mask]): int` | Set the process umask and return the previous value. With no argument, returns the current umask without changing it (implemented by setting `umask(0)` and immediately restoring the original). |
| `ftruncate()` | `ftruncate(resource $handle, $size): bool` | Truncate or extend an open file to `$size` bytes. On a registered userspace-wrapper handle the call dispatches to the wrapper's `stream_truncate(int $new_size)` (the `$size` value is threaded through), returning its `bool` result; a wrapper without `stream_truncate` returns `false`. |
| `fflush()` | `fflush(resource $handle): bool` | Flush buffered output. Implemented as `fsync()` since elephc has no userspace stdio buffer. |
| `fsync()` | `fsync(resource $handle): bool` | Flush data and metadata to durable storage. |
| `fdatasync()` | `fdatasync(resource $handle): bool` | Flush data only. On macOS (which lacks a `fdatasync` libc entry point) this falls back to `fsync()`. |

> All file-modification functions return `true` on success and `false` on failure.

> `touch()` accepts integer Unix timestamps or `null` for `$mtime` / `$atime`. Numeric values, including `-1`, are treated as explicit timestamps; `null` and omitted arguments select PHP's default/current-time behaviour.

## Stream and resource introspection

| Function | Signature | Description |
|---|---|---|
| `get_resource_type()` | `get_resource_type(resource $handle): string` | Returns the resource's type name. Every resource elephc produces is a stream, so the result is `"stream"`. |
| `get_resource_id()` | `get_resource_id(resource $handle): int` | Returns the integer id of a resource, matching the number shown in its `Resource id #N` display string. |
| `stream_isatty()` | `stream_isatty(resource $stream): bool` | Returns `true` when the stream is connected to an interactive terminal. |
| `stream_is_local()` | `stream_is_local(resource\|string $stream): bool` | Returns `true` for local streams. |
| `stream_supports_lock()` | `stream_supports_lock(resource $stream): bool` | Returns `true` when the stream supports `flock()` advisory locking. |
| `stream_get_wrappers()` | `stream_get_wrappers(): array` | Returns the list of built-in stream wrappers (`file`, `php`, `data`, `ftp`, `http`, `https`). User wrappers registered via `stream_wrapper_register()` are not yet surfaced through this list. |
| `stream_wrapper_register()` | `stream_wrapper_register(string $protocol, string $class, int $flags = 0): bool` | Record a user-defined wrapper class for `$protocol://` URLs. Up to 16 registrations are stored; returns `true` on success, `false` when the table is full. A literal `$class` name is validated at compile time and class lookup is case-insensitive. When `fopen("$protocol://...")` matches a registration, elephc instantiates `$class` through the runtime class registry — declared property default values are applied, but `__construct` is not invoked on this registry path — and invokes its stream API — `stream_open`, `stream_read`, `stream_write`, `stream_close`, `stream_eof`, `stream_seek`, `stream_tell`, `stream_flush`, `stream_stat`, `stream_lock` (called by `flock()`, declared `stream_lock(int $operation): bool`), `stream_truncate` (called by `ftruncate()`, declared `stream_truncate(int $new_size): bool`), and the path-based methods `unlink(string $path)`, `rename(string $from, string $to)`, `mkdir(string $path)`, `rmdir(string $path)` (called by the same-named builtins on a registered `scheme://` path, each declared returning `bool`), `stream_metadata(string $path, int $option, mixed $value): bool` (declare `$value` as `mixed` — it is always passed as a boxed value: called by `chmod()` with `STREAM_META_ACCESS` (6) and the mode; by `chown()` with `STREAM_META_OWNER` (3) and an integer uid, or `STREAM_META_OWNER_NAME` (2) and a string user name; by `chgrp()` with `STREAM_META_GROUP` (5) and an integer gid, or `STREAM_META_GROUP_NAME` (4) and a string group name; and by `touch()` with `STREAM_META_TOUCH` (1) and a `[mtime, atime]` int array — an omitted timestamp resolves to the current time), and `stream_set_option(int $option, int $arg1, int $arg2): bool` (called by `stream_set_blocking()` with `STREAM_OPTION_BLOCKING` and by `stream_set_timeout()` with `STREAM_OPTION_READ_TIMEOUT`), `stream_cast(int $cast_as)` (called by `stream_select()` to obtain a real, select()-able fd — return the underlying int fd or a resource that wraps one; a wrapper without `stream_cast`, or one returning a non-resource, is excluded from the select sets), and the directory methods `dir_opendir(string $path, int $options): bool`, `dir_readdir(): string`, `dir_rewinddir(): bool`, `dir_closedir(): bool` (called by `opendir()`/`readdir()`/`rewinddir()`/`closedir()` on a registered `scheme://` path; `dir_readdir()` returns the empty string at end-of-directory) — through the regular method ABI. Wrapper methods should declare their return types (`bool` for `stream_open`/`stream_eof`/`stream_seek`, `string` for `stream_read`, `int` for `stream_write`/`stream_tell`, `void` for `stream_close`) so the runtime call returns through the expected register layout. `stream_stat()` is the exception: declare it **without** a return type (or `: mixed`) and return an associative stat array (`['size' => ..., 'mode' => ..., ...]`); `fstat($handle)` on the wrapper stream dispatches into it and returns that array (or `false` when the method is absent), so `fstat($f)['size']` works. A `: array` return type would be treated as integer-keyed and reject the string keys PHP stat arrays use. A wrapper may also implement `url_stat(string $path, int $flags)` (same return shape, declared without a return type): the path-based `file_exists()`, `filesize()`, and `is_file()` route through it for `"$protocol://..."` URLs — `file_exists()` reports present when `url_stat()` returns a stat array rather than `false`, `filesize()` returns its `'size'` entry, and `is_file()` checks that the `'mode'` entry's `S_IFMT` bits are `S_IFREG`. Non-wrapper paths fall through to a real filesystem stat. |
| `stream_wrapper_unregister()` | `stream_wrapper_unregister(string $protocol): bool` | Remove a user-registered wrapper for `$protocol://`. Returns `true` when a registration was cleared, `false` when no matching protocol is registered. Built-in protocols (`file`, `php`, ...) are not user-registered and cannot be unregistered in v1. |
| `stream_wrapper_restore()` | `stream_wrapper_restore(string $protocol): bool` | Restore a previously-unregistered built-in wrapper. v1 is a no-op that always reports success: elephc cannot unregister built-in wrappers, so they are always present. |
| `stream_socket_enable_crypto()` | `stream_socket_enable_crypto(resource $stream, bool $enable, int $crypto_method = null, resource $session_stream = null): bool` | Attach a TLS session to an already-connected TCP fd. `$enable=true` calls `elephc_tls_attach_fd` via the runtime function-pointer slot, records the handle in `_tls_sessions[fd]`, and returns whether the attach setup succeeded. After this, every `fread`/`fwrite`/`fclose` on the fd routes through the elephc-tls helpers instead of the read/write syscalls. SNI / cert-name comes from `stream_context_create(['ssl' => ['peer_name' => ...]])`; when no peer-name is in the active context it defaults to the transport host of the `stream_socket_client("tcp://host:port")` call (matching PHP), falling back to `"localhost"` only when neither is available. `$enable=false` (mid-stream crypto shutdown) is a stub that reports `true` — callers typically follow with `fclose`, which unwinds the session cleanly. The 3rd `$crypto_method` and 4th `$session_stream` args are evaluated but otherwise ignored — elephc relies on rustls's default protocol negotiation. **Client certificates (mutual TLS):** when the active context carries both `['ssl']['local_cert']` and `['ssl']['local_pk']` (PEM file paths), the attach presents that client certificate chain + private key. The key must be an **unencrypted** PEM (PKCS#8/PKCS#1/SEC1); `['ssl']['passphrase']` is **not** honored (rustls cannot decrypt an encrypted key in this subset), and a passphrase-protected key fails the load. A bad/unreadable `local_cert` or `local_pk` fails the load before any network I/O, so `stream_socket_enable_crypto` returns `false`. **Not honored (rustls limitation):** `['ssl']['ciphers']` (OpenSSL cipher strings have no rustls equivalent — rustls uses a fixed set of modern, safe suites) and `['ssl']['security_level']` (rustls selects TLS 1.2/1.3 automatically) are accepted without error but ignored. |
| `stream_context_create()` | `stream_context_create(array $options = [], array $params = []): resource` | Returns a stream-context resource. The options hash is retained in a global slot (`_stream_context_options`); `stream_context_get_options` and consumer code read it back. v1 limitation: only one active context at a time — each `stream_context_create` call overwrites the slot. The `$params` array is evaluated for its side effects; a literal `['notification' => <closure>]` entry is captured into a global slot and fired by `fopen("http://...")` at the `STREAM_NOTIFY_*` milestones (see the notification note below). Active consumers: `stream_socket_enable_crypto` reads `['ssl']['peer_name']` for SNI / cert-name validation, and `fopen("http://...")` reads `['http']['method']`, `['http']['header']`, and `['http']['content']` to build the request line (with auto-emitted `Content-Length` when a body is supplied). `fopen`'s 4th-arg context resource is accepted for source compatibility. `ftp://` reads `['ftp']['resume_pos']` (a `REST <N>` is sent before `RETR`). `stream_socket_server` reads `['socket']['backlog']` for the `listen()` backlog (default 128). The socket/ftp option values set through the 4-argument `stream_context_set_option` are stored as strings, so integer options (`backlog`, `resume_pos`) are passed as numeric strings (e.g. `"511"`). |
| `stream_context_get_default()` | `stream_context_get_default(array $options = []): resource` | Returns the default stream-context resource (id 1, same slot as `stream_context_create`). Options arg is evaluated for side effects. |
| `stream_context_set_option()` | `stream_context_set_option(resource $context, ...): bool` | Both PHP call shapes work and mutate the persisted options. `stream_context_set_option($ctx, $opts_array)` replaces the global options hash. `stream_context_set_option($ctx, $wrapper, $opt, $value)` navigates the nested `options[wrapper][option]` structure, creating the wrapper sub-hash on demand, and stores `$value` as a string. v1 limitation on the 4-arg form: the value is always stored with the string runtime tag — cast non-string values at the call site. |
| `stream_context_get_options()` | `stream_context_get_options(resource $context): array` | Returns the hash that was passed to the most recent `stream_context_create` call (single global slot in v1). Falls back to an empty hash when no context has been created. |
| `stream_context_get_params()` | `stream_context_get_params(resource $context): array` | v1 stub: returns an empty associative array. Params are not yet persisted on the context resource. |
| `stream_filter_register()` | `stream_filter_register(string $filter_name, string $class): bool` | Register a user-defined filter class. Up to 128 registrations are stored; returns `true` on success, `false` when the table is full. A literal `$class` name is validated at compile time and class lookup is case-insensitive. When `stream_filter_append("$filter_name", ...)` matches, elephc instantiates `$class` through the runtime class registry (declared property defaults are applied; `__construct` is not invoked on this registry path) and routes every fread/fwrite through its `filter(string $data): string` method. Optional `onCreate(): bool` / `onClose(): void` lifecycle hooks fire when present: `onCreate()` runs at `stream_filter_append` time and can refuse the attach by returning `false` (the call site then sees `false` and no filter is recorded). `onClose()` runs at `fclose` time AND at `stream_filter_remove` time, once per direction the filter was attached to.

The `stream_bucket_*` PHP-portable bucket API: `stream_bucket_new($stream, $data)` returns a real stdClass-backed bucket object with `data` (string) and `datalen` (int) public properties, `stream_bucket_make_writeable($brigade)` pops and returns the next bucket from a brigade (or null when empty), and `stream_bucket_append`/`stream_bucket_prepend($brigade, $bucket)` push to the brigade. A registered user filter class is dispatched by its `filter()` arity: a 1-parameter `filter(string $data): string` method takes the simple one-string-in/one-string-out path, while the PHP-canonical 4-parameter `filter($in, $out, &$consumed, $closing): int` method is driven through bucket brigades — the runtime seeds `$in` with one bucket holding the current segment, calls the method, then concatenates the `data` of every bucket the method appended to `$out` (the standard `while ($b = stream_bucket_make_writeable($in)) { $b->data = …; stream_bucket_append($out, $b); }` idiom works). Limits vs PHP: each dispatch seeds a single input bucket (no multi-segment feeding), `&$consumed` can be written by the method but is not fed back into stream read accounting, and the `int` return (`PSFS_PASS_ON` / `PSFS_FEED_ME` / `PSFS_ERR_FATAL`) is observed only loosely — `PSFS_FEED_ME` does not request more input and `PSFS_ERR_FATAL` does not propagate as an error. |

### Stream notification callbacks

A stream context's `notification` callback is fired during `http://` transfers. Register it via the `$params` array of `stream_context_create` (or through `stream_context_set_params($ctx, $params)`):

```php
$ctx = stream_context_create([], [
    'notification' => function (int $code, int $severity, ?string $message,
                                int $message_code, int $bytes_transferred,
                                int $bytes_max): void {
        if ($code === STREAM_NOTIFY_CONNECT)   { echo "connected\n"; }
        if ($code === STREAM_NOTIFY_COMPLETED) { echo "got $bytes_transferred bytes\n"; }
        if ($code === STREAM_NOTIFY_FAILURE)   { echo "failed\n"; }
    },
]);
$body = fopen('http://example.com/', 'r');
```

`fopen("http://...")` fires the callback at three milestones: `STREAM_NOTIFY_CONNECT` (severity `STREAM_NOTIFY_SEVERITY_INFO`, after the TCP connection is established — once per connection, including each redirect hop), `STREAM_NOTIFY_COMPLETED` (after the whole response body is buffered, with `$bytes_transferred` set to the body length), and `STREAM_NOTIFY_FAILURE` (severity `STREAM_NOTIFY_SEVERITY_ERR`, when the connect, status, or temp-file step fails). The `STREAM_NOTIFY_*` and `STREAM_NOTIFY_SEVERITY_*` constants are available.

v1 limitations: only a *literal* `['notification' => <closure or first-class callable>]` entry is captured — a string function-name, `[object, method]` array, or variable callback is not fired (the slot is cleared instead). The callback is stored in a single global slot (matching the single-context model), so it fires for any subsequent `http://` open until another context replaces or clears it. `$message` is always `null` and `$message_code` is always `0`; `$bytes_max` is `0` (the response is read close-framed, without a `Content-Length` size hint). Only `http://` fires notifications — `https://`, `ftp://`, and the `STREAM_NOTIFY_PROGRESS` / `STREAM_NOTIFY_FILE_SIZE_IS` / `STREAM_NOTIFY_MIME_TYPE_IS` / `STREAM_NOTIFY_REDIRECTED` / `STREAM_NOTIFY_AUTH_*` milestones are deferred.
| `stream_set_chunk_size()` | `stream_set_chunk_size(resource $stream, int $size): int` | Sets the per-stream chunk size and returns the **previous** value (PHP's contract, so save/restore works): the first call on a stream reports the `8192` default, and each later call reports the value set by the previous one. Tracked per fd (up to 256; out-of-range / synthetic-wrapper fds report `8192` without storing). The size does not yet change read granularity — reads return identical data — so only the returned previous value is observable. |
| `stream_set_read_buffer()` | `stream_set_read_buffer(resource $stream, int $size): int` | Returns `0` ("success"). elephc streams are unbuffered (direct read syscalls), so the buffer size has no effect — `0` is the correct result for an unbuffered stream. |
| `stream_set_write_buffer()` | `stream_set_write_buffer(resource $stream, int $size): int` | Returns `0` ("success"). elephc streams are unbuffered (each write is flushed immediately), which is exactly the `stream_set_write_buffer($s, 0)` mode; the size is accepted but has no effect. |
| `stream_get_transports()` | `stream_get_transports(): array` | Returns the list of recognised socket transports (`tcp`, `udp`, `unix`, `udg`, `tls`, `ssl`). The `tls` / `ssl` aliases reach `stream_socket_enable_crypto` for in-place TLS promotion of an existing tcp:// socket. |
| `stream_get_filters()` | `stream_get_filters(): array` | Returns the list of registered stream filters (`string.toupper`, `string.tolower`, `string.rot13`, `convert.iconv.*`, `zlib.deflate`, `zlib.inflate`, `bzip2.compress`, `bzip2.decompress`). |
| `stream_filter_append()` | `stream_filter_append(resource $stream, string $filtername, int $read_write = STREAM_FILTER_ALL, mixed $params = null): resource\|false` | Attach a filter to a stream. `$filtername` is the built-in `string.toupper`, `string.tolower`, `string.rot13` (1:1 byte transforms), `zlib.deflate` (raw-deflate compression on writes), `zlib.inflate` (raw-deflate decompression on reads — the whole stream is inflated at attach time), `convert.iconv.<from>/<to>` (charset transcoding via libc `iconv` — on `STREAM_FILTER_READ`/`STREAM_FILTER_ALL` the whole stream is transcoded at attach time like `zlib.inflate`; on `STREAM_FILTER_WRITE` each `fwrite()` payload is transcoded on the way out), `bzip2.compress` (bzip2 compression on writes via libbz2, streamed through `BZ2_bzCompress` and flushed at `fclose`), `bzip2.decompress` (bzip2 decompression on reads — the whole stream is decompressed at attach time), **or** the name of a user filter registered via `stream_filter_register()`. `$read_write` (`STREAM_FILTER_READ`, `STREAM_FILTER_WRITE`, or `STREAM_FILTER_ALL`) selects the directions. Returns a filter resource on success, `false` when the filter name is unknown. The `convert.iconv.*` filter needs `-liconv` on macOS (auto-linked); musl's iconv supports a limited charset set (UTF-8/UTF-16/UTF-32 are fine). The `bzip2.*` filters link `-lbz2` (auto-linked only for programs that attach one) and produce/consume the standard bzip2 stream format (interoperable with PHP's `bzcompress`/`bzdecompress`). The optional 4th `$params` argument tunes the compression filters, in either of PHP's two forms: a bare integer literal (`stream_filter_append($fp, 'zlib.deflate', STREAM_FILTER_WRITE, 6)`) or the canonical associative-array form (`['level' => 6]`). `zlib.deflate` reads `level` (`-1`..`9`, default `-1`); `bzip2.compress` reads `blocks` (the blockSize100k, `1`..`9`, default `9`) and `work` (the workFactor, `0`..`250`, default `0` = libbz2's own default) — e.g. `stream_filter_append($fp, 'bzip2.compress', STREAM_FILTER_WRITE, ['blocks' => 1, 'work' => 30])`. The value must be a compile-time literal (a bare int or a literal array with static int entries); a non-constant `$params` keeps the defaults. zlib's `window` (fixed at `-15` so the raw-deflate output round-trips with `compress.zlib://`) and `memory` sub-options are not exposed, and other filters ignore `$params`. |
| `stream_filter_prepend()` | `stream_filter_prepend(resource $stream, string $filtername, int $read_write = STREAM_FILTER_ALL, mixed $params = null): resource` | Same as `stream_filter_append()`; elephc attaches one filter per stream per direction, so append and prepend are equivalent. |
| `stream_filter_remove()` | `stream_filter_remove(resource $filter): bool` | Remove the filter attached to the stream the resource refers to. Returns `true`. |
| `stream_get_meta_data()` | `stream_get_meta_data(resource $stream): array` | Returns the stream's metadata as an associative array with the keys `timed_out`, `blocked`, `eof`, `unread_bytes`, `stream_type`, `wrapper_type`, `mode`, `seekable`, and `uri`. |

> Use `is_resource()` to check whether a value is an open resource handle before inspecting it.

> `stream_get_meta_data()` derives `eof`, `seekable`, `blocked`, and `mode` from the live descriptor (`lseek`/`fcntl`). `stream_type` is `"STDIO"` for seekable streams and `"tcp_socket"` for non-seekable ones; `wrapper_type` is reported as `"plainfile"` and `uri` as the empty string, since elephc does not track per-resource open paths.

## Network sockets

| Function | Signature | Description |
|---|---|---|
| `stream_socket_server()` | `stream_socket_server($address): resource\|false` | Open a server socket bound to a `[tcp://]A.B.C.D:port`, `udp://A.B.C.D:port`, `unix:///path` (stream) or `udg:///path` (datagram) address; TCP and Unix-stream sockets also listen, while UDP / Unix-datagram sockets only bind. The `listen()` backlog honors the `socket.backlog` context option (default 128). Returns a stream resource, or `false` on failure |
| `stream_socket_client()` | `stream_socket_client($address): resource\|false` | Open a connection to a `[tcp://]A.B.C.D:port`, `udp://A.B.C.D:port`, `unix:///path` (stream) or `udg:///path` (datagram) address; returns a stream resource, or `false` on failure |
| `fsockopen()` | `fsockopen(string $hostname, int $port, int &$error_code = null, string &$error_message = null, float $timeout = null): resource\|false` | Open a TCP connection to `$hostname:$port` and return it as a stream resource, or `false` on failure. On failure the by-reference `$error_code` / `$error_message` are filled (`$error_code` is 0 on success). `$hostname` may be a name or a numeric IPv4 address. The `$timeout` argument is accepted but the connection uses the OS default connect timeout; `$error_code` / `$error_message` must be passed as initialized variables |
| `stream_socket_accept()` | `stream_socket_accept($socket): resource\|false` | Accept the next pending connection on a listening socket; blocks until one arrives. Returns the connection as a stream resource, or `false` on failure |
| `stream_set_blocking()` | `stream_set_blocking($stream, bool $enable): bool` | Toggle a stream's blocking mode (`O_NONBLOCK`). Returns `true` on success |
| `stream_set_timeout()` | `stream_set_timeout($stream, int $seconds, int $microseconds = 0): bool` | Set the receive timeout on a socket stream (`SO_RCVTIMEO`); later reads fail instead of blocking past the timeout. Returns `true` on success |
| `stream_select()` | `stream_select(array &$read, array &$write, array &$except, int $seconds, int $microseconds = 0): int` | Block until one or more streams in the three arrays become ready (or the timeout elapses). Each array is rewritten in place to its ready subset; returns the number of ready streams. Word-0 only — descriptors must be in `0..63`. The three arguments must be arrays (pass `[]`, not `null`, for an unused set). Userspace-wrapper streams are select()-able when their class implements `stream_cast()`: the synthetic wrapper fd is resolved to the real underlying fd via `stream_cast(STREAM_CAST_FOR_SELECT)` and that fd is what is polled. A wrapper without `stream_cast` (or whose `stream_cast` returns a non-resource) is excluded from the sets, matching PHP's "cannot represent as a select()able descriptor" behavior. |
| `stream_socket_shutdown()` | `stream_socket_shutdown($stream, int $mode): bool` | Shut down a socket: `0` disables reads, `1` disables writes, `2` disables both. Returns `true` on success |
| `stream_socket_sendto()` | `stream_socket_sendto($socket, string $data, int $flags = 0, string $address = ""): int\|false` | Send a message on a socket; an empty address sends on the connected peer, a `[scheme://]A.B.C.D:port` address sends an explicit datagram. Returns the byte count, or `false` |
| `stream_socket_recvfrom()` | `stream_socket_recvfrom($socket, int $length, int $flags = 0, string &$address = ""): string\|false` | Receive up to `$length` bytes from a socket. When the optional `$address` string variable is supplied, it is overwritten with the sender address as `A.B.C.D:port`. Returns the received data, or `false` on failure |
| `stream_socket_get_name()` | `stream_socket_get_name($socket, bool $remote): string\|false` | Return a socket's local address (`$remote` false) or peer address (`$remote` true) as `A.B.C.D:port`; `false` on failure |
| `stream_socket_pair()` | `stream_socket_pair(int $domain, int $type, int $protocol): array` | Create a pair of connected sockets (e.g. `STREAM_PF_UNIX`, `STREAM_SOCK_STREAM`, `0`); returns a two-element array of stream resources |
| `popen()` | `popen(string $command, string $mode): resource\|false` | Open a pipe to a process running `$command`; `$mode` is `"r"` to read its output or `"w"` to write to its input. Returns a stream resource, or `false` on failure |
| `pclose()` | `pclose($handle): int` | Close a pipe opened by `popen()` and wait for the process; returns its termination status |
| `gethostname()` | `gethostname(): string` | Return the host name of the machine running the program |
| `gethostbyname()` | `gethostbyname(string $hostname): string` | Resolve a host name to its IPv4 dotted-quad address through the system resolver; returns the host name unchanged when it cannot be resolved |
| `gethostbyaddr()` | `gethostbyaddr(string $ip): string\|false` | Reverse-resolve an IPv4 dotted-quad address to a host name; returns the address unchanged when no record exists, or `false` when it is malformed |
| `getprotobyname()` | `getprotobyname(string $protocol): int\|false` | Look up an IP protocol number by name or alias in the system protocols database; `false` when no entry matches |
| `getprotobynumber()` | `getprotobynumber(int $protocol): string\|false` | Look up an IP protocol name by number in the system protocols database; `false` when no entry matches |
| `getservbyname()` | `getservbyname(string $service, string $protocol): int\|false` | Look up an internet service port by service name or alias and protocol in the system services database; `false` when no entry matches |
| `getservbyport()` | `getservbyport(int $port, string $protocol): string\|false` | Look up an internet service name by port number and protocol in the system services database; `false` when no entry matches |

> A socket address has the form `[tcp://]host:port`, `udp://host:port`, or `unix:///path`. The `host` may be a numeric IPv4 dotted quad (`127.0.0.1`) or a host name (`localhost`, `example.com`); non-numeric host names are resolved to an IPv4 address through the system resolver (libc `gethostbyname`).

## Debugging

| Function | Signature | Description |
|---|---|---|
| `var_dump()` | `var_dump($value): void` | Output type and value. Homogeneous indexed arrays of `int`, `string`, `bool`, or `float` print full per-element bodies (`[N]=>\n  int(V)\n`, etc.). Heterogeneous Mixed-element arrays, hashes, and nested arrays/objects print the `array(N) { ... }` shell with an empty body — the recursive walkers for those layouts are still pending. |
| `print_r()` | `print_r($value): void` | Human-readable output |

```php
<?php
$arr = [1, 2, 3];
var_dump($arr);
// array(3) {
//   [0]=> int(1)
//   [1]=> int(2)
//   [2]=> int(3)
// }
```
