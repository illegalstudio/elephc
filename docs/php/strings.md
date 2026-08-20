---
title: "Strings"
description: "String types, escape sequences, interpolation, heredoc/nowdoc, and built-in string functions."
sidebar:
  order: 6
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

// PHP 8.x deprecated forms are accepted for compatibility:
echo "Hello, ${name}\n";        // deprecated ${var}
echo "Sum: ${1 + 2}\n";         // deprecated ${expr}
```

The `${var}` and `${expr}` forms behave like PHP 8.x: they still work, but are
deprecated. Prefer `$var` or `{$expr}` in new code.

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

## Incrementing a string

`++` on a string uses PHP's perl-style alphanumeric carry, which is how the
spreadsheet-column idiom works:

```php
<?php
$col = "A";
for ($i = 0; $i < 30; $i++) { echo $col, " "; $col++; }
// A B C ... Z AA AB AC AD
```

The carry runs over raw bytes from the end: `a`–`y`, `A`–`Y` and `0`–`8` advance in
place; `z`, `Z` and `9` wrap to `a`, `A` and `0` and carry left; a carry out of the
front prepends `a`, `A` or `1` (`"zz"++` is `"aaa"`, `"Zz"++` is `"AAa"`, `"9z"++` is
`"10a"`). Any other byte stops the carry, so `"a-"++` is unchanged while `"-a"++` is
`"-b"`, and `""++` is `"1"`.

A *numeric* string increments as a number and therefore changes type — `"9"++` is
`int(10)`, `"1.5"++` is `float(2.5)`. `--` never carries: it decrements a numeric
string, turns `""` into `int(-1)`, and leaves every other string alone.

See [Operators](./operators.md#increment--decrement) for the full rules and the one
documented divergence (PHP's `E_DEPRECATED` notices are not emitted).

## Built-in string functions

| Function | Signature | Description |
|---|---|---|
| `strlen()` | `strlen($str): int` | Returns string length |
| `mb_strlen()` | `mb_strlen($str, $encoding = null): int` | Character count in the given encoding. An omitted or `null` encoding counts UTF-8, grouping malformed sequences like mbstring; `8bit`/`binary`/`7bit` return the byte length; other encodings are decoded through the system `iconv`. An unknown encoding name throws `\ValueError` |
| `iconv_strlen()` | `iconv_strlen($str, $encoding = null): int\|false` | Character count through the platform `iconv`. See [iconv](./iconv.md) for the whole extension |
| `substr()` | `substr($str, $start [, $len]): string` | Extract a substring. Negative `$start` counts from the end; a negative `$len` omits that many trailing bytes from the selected suffix, matching PHP |
| `strpos()` | `strpos($haystack, $needle, $offset = 0): int\|false` | Find first occurrence at or after `$offset`. A negative `$offset` counts from the end; one outside the haystack raises `ValueError`. Returns `false` if not found |
| `strrpos()` | `strrpos($haystack, $needle, $offset = 0): int\|false` | Find last occurrence. A non-negative `$offset` starts the search there; a negative one stops it that many bytes before the end. Returns `false` if not found |
| `stripos()` | `stripos($haystack, $needle, $offset = 0): int\|false` | Case-insensitive `strpos()`. Folding is ASCII-only (`A`-`Z`), so non-ASCII bytes are matched verbatim. `$offset` behaves exactly as in `strpos()` |
| `strripos()` | `strripos($haystack, $needle, $offset = 0): int\|false` | Case-insensitive `strrpos()`. Folding is ASCII-only (`A`-`Z`). `$offset` behaves exactly as in `strrpos()` |
| `strstr()` | `strstr($hay, $needle, $before_needle = false): string\|false` | Find first occurrence and return the rest, or the part before it when `$before_needle` is truthy. Returns `false` if not found |
| `str_replace()` | `str_replace($search, $replace, $subject): string` | Replace all occurrences |
| `str_ireplace()` | `str_ireplace($search, $replace, $subject): string` | Case-insensitive replace |
| `substr_replace()` | `substr_replace($str, $repl, $start [, $len]): string` | Replace a substring. Negative `$start` counts from the end; a negative `$len` preserves that many trailing bytes after the replacement, matching PHP |
| `strtolower()` | `strtolower($str): string` | Convert to lowercase |
| `strtoupper()` | `strtoupper($str): string` | Convert to uppercase |
| `ucfirst()` | `ucfirst($str): string` | Uppercase first character |
| `lcfirst()` | `lcfirst($str): string` | Lowercase first character |
| `ucwords()` | `ucwords($string, $separators = " \t\r\n\f\v"): string` | Uppercase the first letter of each word. `$separators` is a byte set |
| `trim()` | `trim($str [, $chars]): string` | Strip the default mask (`" \n\r\t\v\f\0"`) or explicit characters from both ends |
| `ltrim()` | `ltrim($str [, $chars]): string` | Strip the default mask (`" \n\r\t\v\f\0"`) or explicit characters from the left |
| `rtrim()` | `rtrim($str [, $chars]): string` | Strip the default mask (`" \n\r\t\v\f\0"`) or explicit characters from the right |
| `chop()` | `chop($str [, $chars]): string` | Alias of `rtrim()` |
| `str_repeat()` | `str_repeat($str, $times): string` | Repeat a string. A negative `$times` throws `\ValueError`. |
| `str_pad()` | `str_pad($str, $len [, $pad, $type]): string` | Pad string to length. When padding is actually needed, an empty `$pad` or a `$type` outside `STR_PAD_LEFT`/`STR_PAD_RIGHT`/`STR_PAD_BOTH` throws `\ValueError`. |
| `str_split()` | `str_split($str [, $len]): array` | Split into chunks. A `$len` of `0` or less throws `\ValueError`. |
| `strrev()` | `strrev($str): string` | Reverse a string |
| `grapheme_strrev()` | `grapheme_strrev($str): string\|false` | Reverse a UTF-8 string by grapheme clusters, preserving embedded NUL bytes and keeping combining marks, emoji modifiers, and ZWJ sequences with their base cluster. Returns `false` on malformed UTF-8. |
| `strcmp()` | `strcmp($a, $b): int` | Binary-safe string comparison |
| `strcasecmp()` | `strcasecmp($a, $b): int` | Case-insensitive comparison |
| `str_contains()` | `str_contains($hay, $needle): bool` | Check if string contains substring |
| `str_starts_with()` | `str_starts_with($hay, $prefix): bool` | Check prefix |
| `str_ends_with()` | `str_ends_with($hay, $suffix): bool` | Check suffix |
| `ord()` | `ord($char): int` | ASCII value of first character |
| `chr()` | `chr($code): string` | Character from ASCII code |
| `explode()` | `explode($separator, $str [, $limit]): array` | Split string into array. An empty `$separator` throws `\ValueError`. |
| `implode()` | `implode($separator, $array): string`<br>`implode($array): string` | Join array into string; the one-argument form joins with an empty separator |
| `number_format()` | `number_format($n [, $dec [, $dec_point, $thou_sep]]): string` | Format number. A negative `$dec` is not an error: it rounds to that power of ten and formats with no decimals. |
| `sprintf()` | `sprintf($fmt, ...): string` | Format string (%s, %d, %f, %x, %e, %g, %o, %c, %%) |
| `printf()` | `printf($fmt, ...): int` | Format and print |
| `vsprintf()` | `vsprintf($fmt, array $values): string` | Like `sprintf()`, with the arguments supplied as an array. Each element becomes one format argument — int/float/bool/string, including the elements of a mixed array. |
| `vprintf()` | `vprintf($fmt, array $values): int` | Like `printf()`, with the arguments supplied as an array; prints the result and returns the byte count. |
| `sscanf()` | `sscanf($str, $fmt): array` | Parse string with format (%d, %f, %s, %%). Matched fields are returned as substrings (e.g. `%f` yields `"3.14"`), mirroring the existing `%d` behavior. |
| `addslashes()` | `addslashes($str): string` | Escape quotes and backslashes |
| `stripslashes()` | `stripslashes($str): string` | Remove escape backslashes |
| `nl2br()` | `nl2br($str): string` | Insert `<br />` before newlines |
| `wordwrap()` | `wordwrap($str [, $width [, $break [, $cut]]]): string` | Wrap text at word boundaries; set `$cut` to break over-long words. An empty `$break`, or a `$width` of `0` together with `$cut`, throws `\ValueError`. |
| `chunk_split()` | `chunk_split($str [, $length [, $separator]]): string` | Split into fixed-length chunks, appending `$separator` after every chunk including the last. Defaults to 76-byte chunks joined by `\r\n`. An empty subject yields a single separator; a `$length` below `1` throws `\ValueError`. |
| `quotemeta()` | `quotemeta($str): string` | Prefix each of `. \ + * ? [ ^ ] $ ( )` with a backslash |
| `strtr()` | `strtr($str, $from, $to): string`<br>`strtr($str, array $pairs): string` | Translate bytes pairwise, truncated to the shorter of `$from`/`$to` (a later pair for the same source byte wins), or apply replacement `$pairs` longest-match-first in a single left-to-right pass with no re-substitution. Empty keys and keys longer than the subject are ignored. `$pairs` must have string values. |
| `str_word_count()` | `str_word_count($str [, $format [, $characters]]): array\|int` | Count words (`$format` 0), return them as a list (1), or map each word to its byte offset (2). A word is letters plus interior `'` and `-`, widened by every byte of `$characters`. `$format` must be an integer literal, and a value outside `0..2` throws `\ValueError`. |
| `count_chars()` | `count_chars($str [, $mode]): array\|string` | Byte-frequency information: `$mode` 0 tallies all 256 byte values, 1 only the used ones, 2 only the unused ones, 3 renders the used byte values as a string, and 4 the unused ones. `$mode` must be an integer literal, and a value outside `0..4` throws `\ValueError`. |
| `bin2hex()` | `bin2hex($str): string` | Convert binary to hex |
| `hex2bin()` | `hex2bin($str): string` | Convert hex to binary |
| `long2ip()` | `long2ip($ip): string` | Format a 32-bit integer as a dotted-quad IPv4 address |
| `ip2long()` | `ip2long($ip): int\|false` | Parse a decimal dotted-quad IPv4 string into an integer, or `false` if invalid |
| `inet_pton()` | `inet_pton($ip): string\|false` | Pack a dotted-quad IPv4 address into a 4-byte binary string, or `false` if invalid |
| `inet_ntop()` | `inet_ntop($binary): string\|false` | Render a 4-byte IPv4 binary string as a dotted-quad address, or `false` if the length is not 4 |
| `md5()` | `md5($str, $binary = false): string` | MD5 hash — 32-char lowercase hex by default, or the raw 16 digest bytes when `$binary` is `true` |
| `sha1()` | `sha1($str, $binary = false): string` | SHA1 hash — 40-char lowercase hex by default, or the raw 20 digest bytes when `$binary` is `true` |
| `crc32()` | `crc32($str): int` | CRC-32 checksum (standard zlib/PHP polynomial), returned as a non-negative 32-bit integer |
| `hash()` | `hash($algo, $data, $binary = false): string` | Hash `$data` with the named algorithm (md5, sha1, sha2 family, sha3 family, ripemd, crc32/crc32b, and more). Returns lowercase hex by default, or the raw digest bytes when `$binary` is `true`. An unknown algorithm throws `\ValueError`. |
| `hash_hmac()` | `hash_hmac($algo, $data, $key, $binary = false): string` | Keyed-hash message authentication code of `$data` under `$key` using the named cryptographic algorithm. Returns lowercase hex by default, or the raw digest bytes when `$binary` is `true`. An unknown algorithm, or a non-cryptographic checksum (crc32/adler/fnv/joaat), throws `\ValueError`. |
| `hash_file()` | `hash_file($algo, $filename, $binary = false): string\|false` | Hash a file's contents with the named algorithm; returns the digest (hex, or raw bytes when `$binary`), or `false` if the file cannot be read. |
| `hash_equals()` | `hash_equals($known, $user): bool` | Timing-safe string comparison — constant-time for equal-length strings, returns `false` immediately on a length mismatch. |
| `hash_algos()` | `hash_algos(): array` | Return the list of supported hash algorithm names. |
| `hash_init()` | `hash_init($algo): HashContext` | Open an incremental hashing context. An unknown algorithm throws `\ValueError`. (The `HASH_HMAC` flag form is not supported — use `hash_hmac()`.) |
| `hash_update()` | `hash_update($context, $data): bool` | Feed data into an incremental hashing context. |
| `hash_final()` | `hash_final($context, $binary = false): string` | Finalize a context and return the digest (hex, or raw bytes when `$binary`). |
| `hash_copy()` | `hash_copy($context): HashContext` | Clone an incremental hashing context so the original and copy can diverge. |
| `openssl_encrypt()` | `openssl_encrypt($data, $cipher_algo, $passphrase, $options = 0, $iv = "", &$tag = null, $aad = "", $tag_length = 16): string\|false` | Encrypt data with a supported AES CBC, CTR, ECB, or GCM cipher. |
| `openssl_decrypt()` | `openssl_decrypt($data, $cipher_algo, $passphrase, $options = 0, $iv = "", $tag = null, $aad = ""): string\|false` | Decrypt and, for GCM, authenticate data with the supplied tag and AAD. |
| `openssl_cipher_iv_length()` | `openssl_cipher_iv_length($cipher_algo): int\|false` | Return the default IV length for a supported cipher. |
| `openssl_get_cipher_methods()` | `openssl_get_cipher_methods($aliases = false): array` | Return the exact cipher matrix implemented by elephc. |
| `htmlspecialchars()` | `htmlspecialchars($str, $flags = ENT_QUOTES \| ENT_SUBSTITUTE \| ENT_HTML401, $encoding = "UTF-8"): string` | Escape HTML special chars: `&` `<` `>` `"` `'` (single quote as `&#039;`). The `ENT_*` flag constants (`ENT_QUOTES`, `ENT_COMPAT`, `ENT_NOQUOTES`, `ENT_HTML401`, `ENT_HTML5`, `ENT_XHTML`, `ENT_XML1`, `ENT_SUBSTITUTE`, `ENT_IGNORE`) are defined with PHP's values; `$flags` and `$encoding` are accepted but the escaper currently always applies `ENT_QUOTES` behaviour |
| `htmlentities()` | `htmlentities($str, $flags = ENT_QUOTES \| ENT_SUBSTITUTE \| ENT_HTML401, $encoding = "UTF-8"): string` | Alias for htmlspecialchars |
| `html_entity_decode()` | `html_entity_decode($str): string` | Decode HTML entities |
| `parse_url()` | `parse_url(string $url, int $component = -1): array\|string\|int\|null\|false` | Parse present URL components without decoding them; `PHP_URL_*` selects one component |
| `urlencode()` | `urlencode($str): string` | URL-encode (spaces as +) |
| `urldecode()` | `urldecode($str): string` | URL-decode |
| `rawurlencode()` | `rawurlencode($str): string` | URL-encode (spaces as %20) |
| `rawurldecode()` | `rawurldecode($str): string` | URL-decode (RFC 3986) |
| `base64_encode()` | `base64_encode($str): string` | Base64 encode |
| `base64_decode()` | `base64_decode($string, $strict = false): string\|false` | Base64 decode. Whitespace inside the payload is skipped and missing padding is tolerated; the default (lax) mode also drops any other character outside the Base64 alphabet, while `$strict = true` returns `false` for such a character, for data after a padding character, for a truncated final group, and for an invalid amount of padding |
| `quoted_printable_encode()` | `quoted_printable_encode($string): string` | MIME quoted-printable encode. Control bytes, `0x7F`, high-bit bytes, `=`, and a space directly before a `CR` become `=XX`; an embedded `CRLF` is kept as a hard line break; lines are folded at 75 columns with a trailing `=` |
| `gzcompress()` | `gzcompress(string $data, int $level = -1): string` | Compress a string with zlib (system `libz`); `$level` is `-1` (default) or `0`–`9` |
| `gzuncompress()` | `gzuncompress(string $data): string\|false` | Decompress a `gzcompress()`-produced string; `false` on a zlib error |
| `gzdeflate()` | `gzdeflate(string $data, int $level = -1): string` | Compress a string into raw DEFLATE — no zlib header or trailer; `$level` is `-1` (default) or `0`–`9` |
| `gzinflate()` | `gzinflate(string $data): string\|false` | Decompress a raw DEFLATE string from `gzdeflate()` or the `zlib.deflate` stream filter; `false` on a zlib error |
| `ctype_alpha()` | `ctype_alpha($str): bool` | All chars are A-Z/a-z |
| `ctype_digit()` | `ctype_digit($str): bool` | All chars are 0-9 |
| `ctype_alnum()` | `ctype_alnum($str): bool` | All chars are alphanumeric |
| `ctype_space()` | `ctype_space($str): bool` | All chars are whitespace |

#### `explode()` and the `$limit` argument

`explode()` takes PHP's optional third argument:

```php
explode(",", "a,b,c");      // ["a", "b", "c"]
explode(",", "a,b,c", 2);   // ["a", "b,c"]  — the last element keeps the rest
explode(",", "a,b,c", 0);   // ["a,b,c"]     — 0 behaves exactly like 1
explode(",", "a,b,c", -1);  // ["a", "b"]    — drops the last element
explode(",", "a,b,c", -9);  // []            — drops every element
```

An empty `$separator` throws `\ValueError: explode(): Argument #1 ($separator) must not be empty`.

#### `str_pad()` padding modes

`STR_PAD_RIGHT` (`1`, the default), `STR_PAD_LEFT` (`0`), and `STR_PAD_BOTH` (`2`)
are predefined constants:

```php
str_pad("x", 4, "-", STR_PAD_LEFT);  // "---x"
str_pad("x", 5, "ab", STR_PAD_BOTH); // "abxab"
```

Both value checks follow PHP's order: a `$len` that cannot grow the input returns
the input untouched *before* either check runs, so `str_pad("xyz", 1, "")` is
`"xyz"` and not an error. Once padding is actually required, an empty `$pad`
throws `\ValueError: str_pad(): Argument #3 ($pad_string) must not be empty` and a
`$type` outside `0..2` throws
`\ValueError: str_pad(): Argument #4 ($pad_type) must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH`.

#### `number_format()` and negative `$decimals`

A negative `$decimals` is not an error in PHP. The number is rounded to that power
of ten (half away from zero, applied to the magnitude) and then formatted with no
decimals:

```php
number_format(1234.5678, -1);  // "1,230"
number_format(1234.5678, -2);  // "1,200"
number_format(-1234.5678, -1); // "-1,230"
number_format(-4.9, -1);       // "0"  — never "-0"
number_format(1234.5678, -9);  // "0"
```

#### The `HashContext` object

`hash_init()` returns a real `HashContext` **object**, matching PHP 8 (which
migrated incremental hashing from a resource to an object, exactly as GD did with
`GdImage`). It behaves like any other object:

```php
$c = hash_init('md5');
var_dump(is_object($c));            // bool(true)
var_dump(gettype($c));              // string(6) "object"
var_dump(get_class($c));            // string(11) "HashContext"
var_dump($c instanceof HashContext) // bool(true)
var_dump($c);                       // object(HashContext)#1 (1) { ["algo"]=> string(3) "md5" }
```

The class is declared by a compiler-injected prelude that is added **only when the
program references `hash_init`/`hash_update`/`hash_final`/`hash_copy` or names
`HashContext`**, so programs that never hash neither declare the class nor link the
`elephc_crypto` bridge.

Supported and matching PHP:

- `is_object()`, `gettype()`, `get_class()`, `instanceof`, and a parameter or return
  typed `HashContext`.
- `var_dump()`, including the object handle: a context is drawn from the same handle
  pool as ordinary objects, and consumes **no** resource id, so it does not shift the
  ids of surrounding `fopen()` streams.
- `hash_copy()` returns an independent object — feeding the original after copying
  does not affect the copy, and finalizing the original leaves the copy usable.
- Direct construction is rejected. PHP raises `Error: Call to private
  HashContext::__construct() from global scope` at runtime; elephc rejects it at
  compile time. `hash_init()` is the only way to obtain one on either side.
- Using a context after `hash_final()` raises PHP's exact catchable
  `TypeError: hash_update(): Argument #1 ($context) must be a valid, non-finalized HashContext`
  (likewise for `hash_final()` and `hash_copy()`).
- The context is freed automatically when the object goes out of scope.

Known divergences:

- **`serialize()` throws.** PHP serializes a `HashContext` together with its full
  internal digest state so it round-trips. elephc holds an opaque bridge handle and
  cannot reproduce that, so `__serialize()` raises an `Exception` rather than emitting
  a reduced string that would look like a serialized context without being one.
- **HMAC streaming is unsupported.** `hash_init($algo, HASH_HMAC, $key)` is rejected at
  compile time (`Function 'hash_init' expects 1 arguments, got 3`). Use
  [`hash_hmac()`](builtins/string/hash_hmac.md) instead, which is fully supported.
- **Object rendering omits undeclared dynamic properties.** `print_r()` and
  `var_export()` render declared object properties, including a `HashContext`'s
  class-shaped output, but properties created dynamically at runtime are not yet
  included by the renderer.
- **Inside `eval()`**, `hash_init()` still returns a resource: the eval interpreter has
  its own hashing implementation that has not been moved to the object model.

### Symmetric encryption (OpenSSL-compatible)

elephc implements `openssl_encrypt()`, `openssl_decrypt()`,
`openssl_cipher_iv_length()`, and `openssl_get_cipher_methods()` on both compiled
and `eval()` paths. They use the pure-Rust `elephc-crypto` bridge and do not link
the system OpenSSL library. Programs that do not use crypto builtins do not link
the bridge.

The supported cipher list is intentionally smaller than stock PHP/OpenSSL and is
exactly what `openssl_get_cipher_methods()` returns:

| Mode | Supported names | Key bytes | IV bytes |
|---|---|---:|---:|
| CBC | `aes-128-cbc`, `aes-192-cbc`, `aes-256-cbc` | 16 / 24 / 32 | 16 |
| CTR | `aes-128-ctr`, `aes-192-ctr`, `aes-256-ctr` | 16 / 24 / 32 | 16 |
| ECB | `aes-128-ecb`, `aes-192-ecb`, `aes-256-ecb` | 16 / 24 / 32 | 0 |
| GCM | `aes-128-gcm`, `aes-192-gcm`, `aes-256-gcm` | 16 / 24 / 32 | 12 by default |

Cipher names are case-insensitive. Without `OPENSSL_RAW_DATA`, encryption returns
Base64 and decryption accepts Base64. With the flag, both functions exchange raw
bytes. The supported option constants are:

| Constant | Value | Behavior |
|---|---:|---|
| `OPENSSL_RAW_DATA` | 1 | Exchange raw ciphertext instead of Base64. |
| `OPENSSL_ZERO_PADDING` | 2 | Disable PKCS#7 padding for CBC/ECB; plaintext must be block-aligned. |
| `OPENSSL_DONT_ZERO_PAD_KEY` | 4 | Reject a short key instead of zero-padding it. |

Short keys are zero-padded by default and long keys are truncated.

CBC and CTR IVs are zero-padded or truncated to 16 bytes. GCM accepts every
non-empty IV length, reports 12 as its default, writes a 1–16 byte authentication
tag through encrypt's by-reference `$tag`, and authenticates the supplied `$tag`
and `$aad` during decryption. Unknown ciphers, invalid tag/IV lengths, padding
errors, and GCM authentication failures return `false`.

PHP's warning text is not yet reproduced for OpenSSL failures or successful
CBC/CTR IV normalization; the cryptographic result and `false` return behavior
match the supported PHP fixtures. See the
[`examples/openssl_crypt`](https://github.com/illegalstudio/elephc/tree/main/examples/openssl_crypt)
program for CBC and GCM round trips.

`parse_url()` follows PHP's component shapes: without a selector (or with any
negative selector) it returns an associative array containing only present keys,
with `port` stored as an integer. `PHP_URL_SCHEME` through `PHP_URL_FRAGMENT`
select one string or integer component, returning `null` when that component is
absent. A malformed URL returns `false` in either mode; selectors greater than
`PHP_URL_FRAGMENT` raise a catchable `ValueError`. Query and fragment bytes are
returned verbatim rather than decoded.

Regex functions are documented separately in [Regex](regex.md), including the
managed `pcre2` declaration required when a `preg_*` program is finally linked.
