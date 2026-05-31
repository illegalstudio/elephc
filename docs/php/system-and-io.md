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
- The Linux x86_64 target uses its own minimal runtime emitter, but it now registers the same JSON helper families as the ARM64 runtime: structural decode into `Mixed`, stdClass dynamic-property helpers, JsonSerializable-aware object encoding, validation, pretty-printing, depth tracking, and JSON error-message lookup.

## Regex

Regex functions and SPL regex iterators are documented in [Regex](regex.md),
including the PCRE2 native library requirements for compiling programs that use
them.

## File I/O

| Function | Signature | Description |
|---|---|---|
| `fopen()` | `fopen($filename, $mode): resource\|false` | Open file (modes: r, w, a, r+, w+, a+), or `false` on failure |
| `fclose()` | `fclose(resource $handle): bool` | Close file handle |
| `fread()` | `fread(resource $handle, $length): string` | Read up to $length bytes |
| `fwrite()` | `fwrite(resource $handle, $data): int` | Write to file |
| `fgets()` | `fgets(resource $handle): string` | Read a line |
| `feof()` | `feof(resource $handle): bool` | End-of-file check |
| `readline()` | `readline([$prompt]): string` | Read line from STDIN |
| `fseek()` | `fseek(resource $handle, $offset [, $whence]): int` | Seek in file |
| `ftell()` | `ftell(resource $handle): int` | Current position |
| `rewind()` | `rewind(resource $handle): bool` | Seek to beginning |
| `fgetcsv()` | `fgetcsv(resource $handle [, $sep]): array` | Read CSV line |
| `fputcsv()` | `fputcsv(resource $handle, $fields [, $sep]): int` | Write CSV line |
| `fgetc()` | `fgetc(resource $handle): string\|false` | Read one byte, or `false` at EOF/failure |
| `readfile()` | `readfile($filename): int\|false` | Stream a file to stdout, return bytes copied, `-1` on read failure, or `false` on open failure |
| `fpassthru()` | `fpassthru(resource $handle): int` | Stream remaining bytes of an open handle to stdout, returning `-1` on read failure |
| `flock()` | `flock(resource $handle, int $op, &$would_block = null): bool` | Advisory lock. Combine `LOCK_SH` (1), `LOCK_EX` (2), or `LOCK_UN` (3) with the optional `LOCK_NB` (4) flag. `$would_block` is set to `0` or `1` when provided. |
| `tmpfile()` | `tmpfile(): resource\|false` | Create a `/tmp/elephc-XXXXXX` temp file via `mkstemp`, immediately `unlink` the path so the file disappears when the descriptor closes. Returns a stream `resource` on success, or `false` on failure. |

File handles are PHP `resource` values, not integers. `gettype(fopen(...))` returns `"resource"` on success and `"boolean"` on failure, `gettype(STDIN)` returns `"resource"`, and passing a plain `int` to stream functions is rejected. Failed `fopen()` calls, including invalid or empty modes, emit a suppressible runtime warning and return `false`; passing that `false` to stream functions is a fatal runtime TypeError with PHP-style "false given" wording, matching PHP's guard-before-use pattern.

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
| `copy()` | `copy($source, $dest): bool` | Copy file |
| `rename()` | `rename($old, $new): bool` | Rename/move |
| `unlink()` | `unlink($filename): bool` | Delete file |
| `mkdir()` | `mkdir($pathname): bool` | Create directory |
| `rmdir()` | `rmdir($pathname): bool` | Remove directory |
| `scandir()` | `scandir($directory): array` | List files |
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
| `touch()` | `touch($filename [, $mtime [, $atime]]): bool` | Set access/modification times. Creates the file with permissions `0666 & umask` if missing. With no `$mtime`, or `$mtime = null`, uses the current time; with no `$atime`, or `$atime = null`, defaults to `$mtime`. |
| `chmod()` | `chmod($filename, $mode): bool` | Change file mode. |
| `chown()` | `chown($filename, $user): bool` | Change owner by UID or user name. The group is left unchanged. |
| `chgrp()` | `chgrp($filename, $group): bool` | Change group by GID or group name. The owner is left unchanged. |
| `umask()` | `umask([$mask]): int` | Set the process umask and return the previous value. With no argument, returns the current umask without changing it (implemented by setting `umask(0)` and immediately restoring the original). |
| `ftruncate()` | `ftruncate(resource $handle, $size): bool` | Truncate or extend an open file to `$size` bytes. |
| `fflush()` | `fflush(resource $handle): bool` | Flush buffered output. Implemented as `fsync()` since elephc has no userspace stdio buffer. |
| `fsync()` | `fsync(resource $handle): bool` | Flush data and metadata to durable storage. |
| `fdatasync()` | `fdatasync(resource $handle): bool` | Flush data only. On macOS (which lacks a `fdatasync` libc entry point) this falls back to `fsync()`. |

> All file-modification functions return `true` on success and `false` on failure.

> `touch()` accepts integer Unix timestamps or `null` for `$mtime` / `$atime`. Numeric values, including `-1`, are treated as explicit timestamps; `null` and omitted arguments select PHP's default/current-time behaviour.

## Debugging

| Function | Signature | Description |
|---|---|---|
| `var_dump()` | `var_dump($value): void` | Output type and value |
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
