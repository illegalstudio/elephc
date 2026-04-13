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
| `php_uname()` | `php_uname([$mode]): string` | Get OS name (returns "Darwin") |
| `phpversion()` | `phpversion(): string` | Get elephc version string |
| `exec()` | `exec($command): string` | Execute command, return output |
| `shell_exec()` | `shell_exec($command): string` | Execute via shell, return output |
| `system()` | `system($command): string` | Execute, output to stdout |
| `passthru()` | `passthru($command): void` | Execute, pass raw output |

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
| `file_get_contents()` | `file_get_contents($filename): string` | Read entire file |
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
