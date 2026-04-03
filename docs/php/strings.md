---
title: "Strings"
description: "String types, escape sequences, interpolation, heredoc/nowdoc, and built-in string functions."
sidebar:
  order: 5
---

## Double-quoted strings

Support escape sequences:

```php
<?php
echo "Hello\n";      // newline
echo "Tab\there";    // tab
echo "Quote: \"";    // escaped quote
echo "Backslash: \\"; // backslash
```

## Single-quoted strings

No escape sequences except `\\` and `\'`:

```php
<?php
echo 'Hello\n';      // prints: Hello\n (literal)
echo 'It\'s here';   // prints: It's here
```

## String interpolation

```php
<?php
$name = "World";
echo "Hello, $name\n";
```

## Heredoc strings

Multi-line with escape processing (like double-quoted):

```php
<?php
echo <<<EOT
Hello World
This is line 2
EOT;
```

## Nowdoc strings

Multi-line without escape processing (like single-quoted):

```php
<?php
echo <<<'EOT'
Hello World
No escapes: \n \t stay literal
EOT;
```

## String indexing

```php
<?php
$s = "hello";
echo $s[1];    // e
echo $s[-1];   // o
echo "[" . $s[99] . "]";  // []
```

Read-only. Negative indices count from end. Out-of-bounds returns empty string.

## Built-in string functions

| Function | Signature | Description |
|---|---|---|
| `strlen()` | `strlen($str): int` | Returns string length |
| `substr()` | `substr($str, $start [, $len]): string` | Extract substring |
| `strpos()` | `strpos($hay, $needle): int` | Find first occurrence. Returns `-1` if not found (PHP returns `false`) |
| `strrpos()` | `strrpos($hay, $needle): int` | Find last occurrence (-1 if not found) |
| `strstr()` | `strstr($hay, $needle): string` | Find first occurrence and return rest |
| `str_replace()` | `str_replace($search, $replace, $subject): string` | Replace all occurrences |
| `str_ireplace()` | `str_ireplace($search, $replace, $subject): string` | Case-insensitive replace |
| `substr_replace()` | `substr_replace($str, $repl, $start [, $len]): string` | Replace substring |
| `strtolower()` | `strtolower($str): string` | Convert to lowercase |
| `strtoupper()` | `strtoupper($str): string` | Convert to uppercase |
| `ucfirst()` | `ucfirst($str): string` | Uppercase first character |
| `lcfirst()` | `lcfirst($str): string` | Lowercase first character |
| `ucwords()` | `ucwords($str): string` | Uppercase first letter of each word |
| `trim()` | `trim($str [, $chars]): string` | Strip whitespace from both ends |
| `ltrim()` | `ltrim($str [, $chars]): string` | Strip whitespace from left |
| `rtrim()` | `rtrim($str [, $chars]): string` | Strip whitespace from right |
| `str_repeat()` | `str_repeat($str, $times): string` | Repeat a string |
| `str_pad()` | `str_pad($str, $len [, $pad, $type]): string` | Pad string to length |
| `str_split()` | `str_split($str [, $len]): array` | Split into chunks |
| `strrev()` | `strrev($str): string` | Reverse a string |
| `strcmp()` | `strcmp($a, $b): int` | Binary-safe string comparison |
| `strcasecmp()` | `strcasecmp($a, $b): int` | Case-insensitive comparison |
| `str_contains()` | `str_contains($hay, $needle): bool` | Check if string contains substring |
| `str_starts_with()` | `str_starts_with($hay, $prefix): bool` | Check prefix |
| `str_ends_with()` | `str_ends_with($hay, $suffix): bool` | Check suffix |
| `ord()` | `ord($char): int` | ASCII value of first character |
| `chr()` | `chr($code): string` | Character from ASCII code |
| `explode()` | `explode($delim, $str): array` | Split string into array |
| `implode()` | `implode($glue, $arr): string` | Join array into string |
| `number_format()` | `number_format($n [, $dec [, $dec_point, $thou_sep]]): string` | Format number |
| `sprintf()` | `sprintf($fmt, ...): string` | Format string (%s, %d, %f, %x, %e, %g, %o, %c, %%) |
| `printf()` | `printf($fmt, ...): int` | Format and print |
| `sscanf()` | `sscanf($str, $fmt): array` | Parse string with format (%d, %s) |
| `addslashes()` | `addslashes($str): string` | Escape quotes and backslashes |
| `stripslashes()` | `stripslashes($str): string` | Remove escape backslashes |
| `nl2br()` | `nl2br($str): string` | Insert `<br />` before newlines |
| `wordwrap()` | `wordwrap($str [, $width [, $break [, $cut]]]): string` | Wrap text at width |
| `bin2hex()` | `bin2hex($str): string` | Convert binary to hex |
| `hex2bin()` | `hex2bin($str): string` | Convert hex to binary |
| `md5()` | `md5($str): string` | MD5 hash (32-char hex) |
| `sha1()` | `sha1($str): string` | SHA1 hash (40-char hex) |
| `hash()` | `hash($algo, $data): string` | Hash with algorithm (md5, sha1, sha256) |
| `htmlspecialchars()` | `htmlspecialchars($str): string` | Escape HTML special chars |
| `htmlentities()` | `htmlentities($str): string` | Alias for htmlspecialchars |
| `html_entity_decode()` | `html_entity_decode($str): string` | Decode HTML entities |
| `urlencode()` | `urlencode($str): string` | URL-encode (spaces as +) |
| `urldecode()` | `urldecode($str): string` | URL-decode |
| `rawurlencode()` | `rawurlencode($str): string` | URL-encode (spaces as %20) |
| `rawurldecode()` | `rawurldecode($str): string` | URL-decode (RFC 3986) |
| `base64_encode()` | `base64_encode($str): string` | Base64 encode |
| `base64_decode()` | `base64_decode($str): string` | Base64 decode |
| `ctype_alpha()` | `ctype_alpha($str): bool` | All chars are A-Z/a-z |
| `ctype_digit()` | `ctype_digit($str): bool` | All chars are 0-9 |
| `ctype_alnum()` | `ctype_alnum($str): bool` | All chars are alphanumeric |
| `ctype_space()` | `ctype_space($str): bool` | All chars are whitespace |
| `preg_match()` | `preg_match($pattern, $subject): int` | Test if regex matches (1 or 0). Uses POSIX extended regex. |
| `preg_match_all()` | `preg_match_all($pattern, $subject): int` | Count all non-overlapping matches |
| `preg_replace()` | `preg_replace($pattern, $replacement, $subject): string` | Replace all regex matches |
| `preg_split()` | `preg_split($pattern, $subject): array` | Split string by regex pattern |

### Regex limitations

- Uses POSIX extended regex via libc, with translation of common PCRE shorthands (`\s`, `\d`, `\w`)
- Lookahead, lookbehind, non-greedy quantifiers are not supported
- `preg_match()` does not support `$matches` capture parameter
- `preg_replace()` does not support backreferences like `$1`
