---
title: "iconv"
description: "Character-set conversion and character-oriented string functions: the 10 PHP iconv functions."
sidebar:
  order: 23
---

elephc implements all 10 procedural `iconv` functions and the extension's four
constants, on the native compilation path and inside dynamic `eval()`. The
conversions themselves are performed by the platform's `iconv` implementation —
glibc on Linux, GNU libiconv on macOS — so the charsets your system supports are
exactly the charsets your program supports, the same relationship PHP has.

The implementation lives in the `elephc_iconv` bridge crate. It is linked
automatically the first time a program calls one of these functions, and never
linked otherwise; `--with-iconv` force-links it when detection cannot see the
call. See
[Linking & Conditional Compilation](../compiling/linking-and-conditional-compilation.md).

## Converting between charsets

| Function | Signature | Notes |
|---|---|---|
| `iconv()` | `iconv(string $from_encoding, string $to_encoding, string $string): string\|false` | Converts a byte string from one charset to another |

`$to_encoding` accepts the two suffixes the platform `iconv` defines:

- `//TRANSLIT` approximates characters the target charset cannot represent.
- `//IGNORE` drops them instead.

The approximation `//TRANSLIT` chooses belongs to the platform's `iconv`, not to PHP,
so it differs between targets: glibc renders `é` as `e`, while GNU libiconv renders it
as `'e`. PHP behaves the same way, reporting whatever its own provider produces. Only
`//IGNORE` reads identically everywhere, because dropping a character has one spelling.

```php
$utf8   = "Prüfung café";
$latin1 = iconv("UTF-8", "ISO-8859-1", $utf8);

echo bin2hex($latin1);                        // 5072fc66756e6720636166e9
echo iconv("ISO-8859-1", "UTF-8", $latin1);   // Prüfung café
echo iconv("UTF-8", "ASCII//TRANSLIT", $utf8); // Prufung cafe   (glibc)
echo iconv("UTF-8", "ASCII//IGNORE", $utf8);   // Prfung caf
```

A charset pair the platform cannot open raises a warning and returns `false`; a
byte sequence the source charset rejects raises a notice and returns `false`.

## Character-oriented strings

`strlen()` and friends count bytes. These four functions count characters in the
selected charset, so multibyte text measures and slices the way a reader expects.

| Function | Signature | Notes |
|---|---|---|
| `iconv_strlen()` | `iconv_strlen(string $string, ?string $encoding = null): int\|false` | Character count |
| `iconv_substr()` | `iconv_substr(string $string, int $offset, ?int $length = null, ?string $encoding = null): string\|false` | Character-indexed slice, with `substr()`'s negative-value conventions |
| `iconv_strpos()` | `iconv_strpos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int\|false` | First character position |
| `iconv_strrpos()` | `iconv_strrpos(string $haystack, string $needle, ?string $encoding = null): int\|false` | Last character position |

```php
$text = "Prüfung café";

echo strlen($text);                  // 14 bytes
echo iconv_strlen($text);            // 12 characters
echo iconv_substr($text, 0, 7);      // Prüfung
echo iconv_strpos($text, "café");    // 8
echo iconv_strrpos($text, "f");      // 10
```

An omitted or `null` `$encoding` uses the internal encoding (see below); an
explicitly empty string uses PHP's `default_charset`, which is `UTF-8`. An empty
`$needle` never matches. An `$offset` outside `$haystack` throws a catchable
`\ValueError`, matching PHP 8.

## MIME header fields

RFC 2047 encoded-words carry non-ASCII text through mail headers that only allow
ASCII.

| Function | Signature | Notes |
|---|---|---|
| `iconv_mime_encode()` | `iconv_mime_encode(string $field_name, string $field_value, array $options = []): string\|false` | Encodes one field, folding it across lines |
| `iconv_mime_decode()` | `iconv_mime_decode(string $string, int $mode = 0, ?string $encoding = null): string\|false` | Decodes one header field |
| `iconv_mime_decode_headers()` | `iconv_mime_decode_headers(string $headers, int $mode = 0, ?string $encoding = null): array\|false` | Decodes a whole header block |

`$options` accepts these keys:

| Key | Default | Meaning |
|---|---|---|
| `scheme` | `B` | `B` for base64 encoded-words, `Q` for quoted-printable |
| `input-charset` | internal encoding | Charset `$field_value` is currently in |
| `output-charset` | `input-charset` | Charset the header is encoded into |
| `line-length` | `76` | Maximum length of one output line |
| `line-break-chars` | `"\r\n"` | Bytes written between folded lines |

```php
$subject = iconv_mime_encode("Subject", "Prüfung");
echo $subject;                        // Subject: =?UTF-8?B?UHLDvGZ1bmc=?=
echo iconv_mime_decode($subject);     // Subject: Prüfung

echo iconv_mime_encode("Subject", "Prüfung", ["scheme" => "Q"]);
// Subject: =?UTF-8?Q?Pr=C3=BCfung?=
```

`iconv_mime_decode()` decodes exactly one field: a line break that no linear
whitespace follows ends it. `iconv_mime_decode_headers()` walks the whole block
and returns an associative array; a field name that appears more than once
collects its values into a list.

```php
$headers = iconv_mime_decode_headers(
    "Subject: =?ISO-8859-1?Q?Pr=FCfung?=\r\n" .
    "To: alice@example.com\r\n" .
    "To: bob@example.com\r\n\r\nbody"
);

echo $headers["Subject"];              // Prüfung
echo implode(", ", $headers["To"]);    // alice@example.com, bob@example.com
```

`$mode` accepts the extension's two flags, which may be combined:

| Constant | Value | Effect |
|---|---|---|
| `ICONV_MIME_DECODE_STRICT` | `1` | Only accept encoded-words RFC 2047 allows at that position; anything else stays literal text |
| `ICONV_MIME_DECODE_CONTINUE_ON_ERROR` | `2` | Keep undecodable text verbatim instead of failing the whole call |

## Default encodings

The extension keeps three process-wide charset settings. All three start at
`UTF-8`, and the character-oriented functions fall back to `internal_encoding`
when their `$encoding` argument is omitted.

| Function | Signature | Notes |
|---|---|---|
| `iconv_get_encoding()` | `iconv_get_encoding(string $type = "all"): array\|string\|false` | `all` reports the trio as an array; a single name reports one charset |
| `iconv_set_encoding()` | `iconv_set_encoding(string $type, string $encoding): bool` | Sets one of `input_encoding`, `output_encoding`, `internal_encoding` |

```php
$encodings = iconv_get_encoding();
echo $encodings["internal_encoding"];        // UTF-8

iconv_set_encoding("internal_encoding", "ISO-8859-1");
echo iconv_strlen("Prüfung café");           // 14 — the bytes are now read as Latin-1
```

Like PHP, `iconv_set_encoding()` stores the charset without validating it, so
only an unrecognized `$type` reports `false`. `$type` is matched
case-insensitively.

## Constants

| Constant | Value |
|---|---|
| `ICONV_MIME_DECODE_STRICT` | `1` |
| `ICONV_MIME_DECODE_CONTINUE_ON_ERROR` | `2` |
| `ICONV_IMPL` | `"glibc"` on Linux targets, `"libiconv"` on macOS targets |
| `ICONV_VERSION` | `"unknown"` |

PHP bakes `ICONV_IMPL` and `ICONV_VERSION` when the interpreter is built. elephc
compiles ahead of time for a target whose libc build is not knowable, so
`ICONV_IMPL` is derived from the target platform and `ICONV_VERSION` reports the
`unknown` spelling php-src itself uses when it cannot identify its provider.

## Stream filters

The `convert.iconv.<from>/<to>` stream filter transcodes data as it flows through
a stream, in either direction. It is part of the stream layer rather than this
extension; see [Streams](streams.md).

```php
$stream = fopen("php://memory", "r+");
stream_filter_append($stream, "convert.iconv.UTF-8/ISO-8859-1", STREAM_FILTER_WRITE);
fwrite($stream, "café");
```
