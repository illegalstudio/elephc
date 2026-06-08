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
echo "Return\r";     // carriage return
echo "Vert\v";       // vertical tab
echo "Esc\e";        // escape byte
echo "Form\f";       // form feed
echo "Quote: \"";    // escaped quote
echo "Backslash: \\"; // backslash
echo "\x41";         // hex byte: A
echo "\101";         // octal byte: A
echo "\u{1F600}";    // Unicode codepoint: 😀
```

## Single-quoted strings

No escape sequences except `\\` and `\'`:

```php
<?php
echo 'Hello\n';      // prints: Hello\n (literal)
echo 'It\'s here';   // prints: It's here
```

## String interpolation

Double-quoted strings and heredocs interpolate variables. Both the simple and complex
syntaxes are supported:

```php
<?php
$name = "World";
echo "Hello, $name\n";          // simple: $variable

$user = ["name" => "Ada", "age" => 36];
echo "Name: $user[name]\n";     // simple: one $var[offset] (bareword key, no quotes)

class Point { public int $x = 1; }
$p = new Point();
echo "x = $p->x\n";             // simple: one $var->prop

echo "Sum: {$user['age']}\n";   // complex: {$expr} allows full expressions
```

Variable and identifier names may contain non-ASCII letters, matching PHP:

```php
<?php
$café = "espresso";
echo "Order: $café\n";
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

The closing label closes the heredoc when it is at the start of a line and followed by any
non-identifier character, so a heredoc can be used as an expression — for example as a
function argument or in a concatenation:

```php
<?php
echo strtoupper(<<<EOT
hello
EOT) . "!";
```

PHP 7.3+ flexible (indented) heredocs are supported: the closing marker may be indented,
and that indentation is stripped from every body line.

```php
<?php
function describe(): string {
    return <<<EOT
        line one
        line two
        EOT;   // -> "line one\nline two"
}
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
| `strpos()` | `strpos($hay, $needle): int\|false` | Find first occurrence. Returns `false` if not found |
| `strrpos()` | `strrpos($hay, $needle): int\|false` | Find last occurrence. Returns `false` if not found |
| `strstr()` | `strstr($hay, $needle): string` | Find first occurrence and return rest |
| `str_replace()` | `str_replace($search, $replace, $subject): string` | Replace all occurrences |
| `str_ireplace()` | `str_ireplace($search, $replace, $subject): string` | Case-insensitive replace |
| `substr_replace()` | `substr_replace($str, $repl, $start [, $len]): string` | Replace substring |
| `strtolower()` | `strtolower($str): string` | Convert to lowercase |
| `strtoupper()` | `strtoupper($str): string` | Convert to uppercase |
| `ucfirst()` | `ucfirst($str): string` | Uppercase first character |
| `lcfirst()` | `lcfirst($str): string` | Lowercase first character |
| `ucwords()` | `ucwords($str): string` | Uppercase first letter of each word |
| `trim()` | `trim($str [, $chars]): string` | Strip the default mask (`" \n\r\t\v\f\0"`) or explicit characters from both ends |
| `ltrim()` | `ltrim($str [, $chars]): string` | Strip the default mask (`" \n\r\t\v\f\0"`) or explicit characters from the left |
| `rtrim()` | `rtrim($str [, $chars]): string` | Strip the default mask (`" \n\r\t\v\f\0"`) or explicit characters from the right |
| `chop()` | `chop($str [, $chars]): string` | Alias of `rtrim()` |
| `str_repeat()` | `str_repeat($str, $times): string` | Repeat a string |
| `str_pad()` | `str_pad($str, $len [, $pad, $type]): string` | Pad string to length |
| `str_split()` | `str_split($str [, $len]): array` | Split into chunks |
| `strrev()` | `strrev($str): string` | Reverse a string |
| `grapheme_strrev()` | `grapheme_strrev($str): string\|false` | Reverse a UTF-8 string by grapheme clusters, preserving embedded NUL bytes and keeping combining marks, emoji modifiers, and ZWJ sequences with their base cluster. Returns `false` on malformed UTF-8. |
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
| `vsprintf()` | `vsprintf($fmt, array $values): string` | Like `sprintf()`, with the arguments supplied as an array. Each element becomes one format argument — int/float/bool/string, including the elements of a mixed array. |
| `vprintf()` | `vprintf($fmt, array $values): int` | Like `printf()`, with the arguments supplied as an array; prints the result and returns the byte count. |
| `sscanf()` | `sscanf($str, $fmt): array` | Parse string with format (%d, %f, %s, %%). Matched fields are returned as substrings (e.g. `%f` yields `"3.14"`), mirroring the existing `%d` behavior. |
| `addslashes()` | `addslashes($str): string` | Escape quotes and backslashes |
| `stripslashes()` | `stripslashes($str): string` | Remove escape backslashes |
| `nl2br()` | `nl2br($str): string` | Insert `<br />` before newlines |
| `wordwrap()` | `wordwrap($str [, $width [, $break [, $cut]]]): string` | Wrap text at word boundaries; set `$cut` to break over-long words |
| `bin2hex()` | `bin2hex($str): string` | Convert binary to hex |
| `hex2bin()` | `hex2bin($str): string` | Convert hex to binary |
| `long2ip()` | `long2ip($ip): string` | Format a 32-bit integer as a dotted-quad IPv4 address |
| `ip2long()` | `ip2long($ip): int\|false` | Parse a decimal dotted-quad IPv4 string into an integer, or `false` if invalid |
| `inet_pton()` | `inet_pton($ip): string\|false` | Pack a dotted-quad IPv4 address into a 4-byte binary string, or `false` if invalid |
| `inet_ntop()` | `inet_ntop($binary): string\|false` | Render a 4-byte IPv4 binary string as a dotted-quad address, or `false` if the length is not 4 |
| `md5()` | `md5($str): string` | MD5 hash (32-char hex) |
| `sha1()` | `sha1($str): string` | SHA1 hash (40-char hex) |
| `crc32()` | `crc32($str): int` | CRC-32 checksum (standard zlib/PHP polynomial), returned as a non-negative 32-bit integer |
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
| `gzcompress()` | `gzcompress(string $data, int $level = -1): string` | Compress a string with zlib (system `libz`); `$level` is `-1` (default) or `0`–`9` |
| `gzuncompress()` | `gzuncompress(string $data): string\|false` | Decompress a `gzcompress()`-produced string; `false` on a zlib error |
| `gzdeflate()` | `gzdeflate(string $data, int $level = -1): string` | Compress a string into raw DEFLATE — no zlib header or trailer; `$level` is `-1` (default) or `0`–`9` |
| `gzinflate()` | `gzinflate(string $data): string\|false` | Decompress a raw DEFLATE string from `gzdeflate()` or the `zlib.deflate` stream filter; `false` on a zlib error |
| `ctype_alpha()` | `ctype_alpha($str): bool` | All chars are A-Z/a-z |
| `ctype_digit()` | `ctype_digit($str): bool` | All chars are 0-9 |
| `ctype_alnum()` | `ctype_alnum($str): bool` | All chars are alphanumeric |
| `ctype_space()` | `ctype_space($str): bool` | All chars are whitespace |

Regex functions are documented separately in [Regex](regex.md), including the
PCRE2 build requirements for programs that use `preg_*`.
