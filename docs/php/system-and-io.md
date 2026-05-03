---
title: "System & I/O"
description: "System functions, file I/O, date/time, JSON, and debugging utilities."
sidebar:
  order: 10
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
| `php_uname()` | `php_uname($mode = "a"): string` | Get system information from the target runtime |
| `phpversion()` | `phpversion(): string` | Get the elephc package version from `Cargo.toml` |
| `exec()` | `exec($command): string` | Execute command, return output |
| `shell_exec()` | `shell_exec($command): string` | Execute via shell, return output |
| `system()` | `system($command): string` | Execute, output to stdout |
| `passthru()` | `passthru($command): void` | Execute, pass raw output |

`define()` returns `true` the first time a constant is defined at runtime. Duplicate attempts keep the first value, return `false`, and emit a suppressible runtime warning.

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
| `strtotime()` | `strtotime($datetime): int` | Parse "YYYY-MM-DD" or "YYYY-MM-DD HH:MM:SS" to timestamp |

> `strtotime()` only supports "YYYY-MM-DD" and "YYYY-MM-DD HH:MM:SS" formats. Relative strings like "next Monday" are not supported.

## JSON

| Function | Signature | Description |
|---|---|---|
| `json_encode()` | `json_encode($value): string` | Encode as JSON. Supports int, float, string, bool, null, arrays, and mixed payloads. |
| `json_decode()` | `json_decode($json): string` | Decode to the current string representation: trims outer JSON whitespace, unescapes quoted JSON strings, and returns other JSON literals/arrays/objects as trimmed strings. |
| `json_last_error()` | `json_last_error(): int` | Always returns 0 |

> `json_decode()` returns a string representation. It does not parse objects to arrays. Standard one-byte escapes (`\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`) are decoded inside quoted JSON strings, and `\uXXXX` escapes are decoded to UTF-8, including surrogate pairs.

## Regex

| Function | Signature | Description |
|---|---|---|
| `preg_match()` | `preg_match($pattern, $subject): int` | Test regex match (1 or 0) |
| `preg_match_all()` | `preg_match_all($pattern, $subject): int` | Count all non-overlapping matches |
| `preg_replace()` | `preg_replace($pattern, $replacement, $subject): string` | Replace all regex matches |
| `preg_split()` | `preg_split($pattern, $subject): array` | Split string by regex |

Uses POSIX extended regex with common PCRE shorthand translation (`\s`, `\d`, `\w`). Lookahead, lookbehind, non-greedy quantifiers not supported.

## File I/O

| Function | Signature | Description |
|---|---|---|
| `fopen()` | `fopen($filename, $mode): int` | Open file (modes: r, w, a, r+, w+, a+) |
| `fclose()` | `fclose($handle): bool` | Close file handle |
| `fread()` | `fread($handle, $length): string` | Read up to $length bytes |
| `fwrite()` | `fwrite($handle, $data): int` | Write to file |
| `fgets()` | `fgets($handle): string` | Read a line |
| `feof()` | `feof($handle): bool` | End-of-file check |
| `readline()` | `readline([$prompt]): string` | Read line from STDIN |
| `fseek()` | `fseek($handle, $offset [, $whence]): int` | Seek in file |
| `ftell()` | `ftell($handle): int` | Current position |
| `rewind()` | `rewind($handle): bool` | Seek to beginning |
| `fgetcsv()` | `fgetcsv($handle [, $sep]): array` | Read CSV line |
| `fputcsv()` | `fputcsv($handle, $fields [, $sep]): int` | Write CSV line |

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
| `fstat()` | `fstat($handle): array\|false` | Same shape as `stat()` but operates on an open file descriptor, or `false` on failure |
| `clearstatcache()` | `clearstatcache($clear_realpath_cache = false, $filename = ""): void` | No-op (elephc does not cache `stat()` results). Arguments are still evaluated. |

> The 13 `stat()` / `lstat()` / `fstat()` fields are inserted in PHP's documented order. Check the return value against `false` before reading fields when the path or descriptor may be invalid.

## Path manipulation

| Function | Signature | Description |
|---|---|---|
| `basename()` | `basename($path [, $suffix]): string` | Trailing name component. `$suffix` is trimmed when it is a strict suffix of the result. |
| `dirname()` | `dirname($path [, $levels = 1]): string` | Parent directory. Repeats the parent lookup when `$levels` is greater than 1. |
| `pathinfo()` | `pathinfo($path [, $flag]): array\|string` | Without a flag, or with compile-time `PATHINFO_ALL`: associative array with keys `dirname`, `basename`, `extension` (when the basename contains a dot), `filename`. With compile-time component flags (`DIRNAME`, `BASENAME`, `EXTENSION`, `FILENAME`): the corresponding string. |
| `realpath()` | `realpath($path): string\|false` | Canonicalized absolute path, or `false` when the path does not exist. |
| `fnmatch()` | `fnmatch($pattern, $filename [, $flags = 0]): bool` | Shell-glob match. Supports `*`, `?`, `[abc]`, `[a-z]`, `[!abc]`/`[^abc]`, `\\<char>`. |

> `pathinfo()` accepts compile-time `PATHINFO_DIRNAME` (1), `PATHINFO_BASENAME` (2), `PATHINFO_EXTENSION` (4), `PATHINFO_FILENAME` (8), and `PATHINFO_ALL` (15) constants, integer literals, and bitmasks such as `PATHINFO_DIRNAME | PATHINFO_EXTENSION`. Component bitmasks follow PHP priority: dirname, basename, extension, then filename. The component-flag form returns the requested component as a string (or empty string when it is absent — e.g. `pathinfo("foo", PATHINFO_EXTENSION)` returns `""`). The no-flag and `PATHINFO_ALL` forms return an associative array; the `extension` key is omitted only when the basename has no dot, matching PHP's behaviour.

> `fnmatch()` accepts the optional flags argument only when it is `0`. Non-zero flags (`FNM_PATHNAME`, `FNM_PERIOD`, `FNM_CASEFOLD`, `FNM_NOESCAPE`) are tracked in `ROADMAP.md`.

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
