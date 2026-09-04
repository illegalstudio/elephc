---
title: "String builtins"
description: "Builtins in the String category."
sidebar:
  order: 101
---

## String builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`addslashes()`](./string/addslashes.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`base64_decode()`](./string/base64_decode.md) | `(string $string, bool $strict = false): mixed` | `mixed` | ✓ | ✓ |
| [`base64_encode()`](./string/base64_encode.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`bin2hex()`](./string/bin2hex.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`chop()`](./string/chop.md) | `(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string` | `string` | ✓ | ✓ |
| [`chr()`](./string/chr.md) | `(int $codepoint): string` | `string` | ✓ | ✓ |
| [`chunk_split()`](./string/chunk_split.md) | `(string $string, int $length = 76, string $separator = '\r\n'): string` | `string` | ✓ | ✓ |
| [`count_chars()`](./string/count_chars.md) | `(string $string, int $mode = 0): array|string` | `array|string` | ✓ | ✓ |
| [`crc32()`](./string/crc32.md) | `(string $string): int` | `int` | ✓ | ✓ |
| [`explode()`](./string/explode.md) | `(string $separator, string $string, int $limit = PHP_INT_MAX): array` | `array` | ✓ | ✓ |
| [`grapheme_strrev()`](./string/grapheme_strrev.md) | `(string $string): mixed` | `mixed` | ✓ | ✓ |
| [`gzcompress()`](./string/gzcompress.md) | `(string $data, int $level = -1): string` | `string` | ✓ | ✓ |
| [`gzdecode()`](./string/gzdecode.md) | `(string $data, int $max_length = 0): mixed` | `mixed` | ✓ | — |
| [`gzdeflate()`](./string/gzdeflate.md) | `(string $data, int $level = -1): string` | `string` | ✓ | ✓ |
| [`gzencode()`](./string/gzencode.md) | `(string $data, int $level = -1, int $encoding = 31): mixed` | `mixed` | ✓ | — |
| [`gzinflate()`](./string/gzinflate.md) | `(string $data, int $max_length = 0): mixed` | `mixed` | ✓ | ✓ |
| [`gzuncompress()`](./string/gzuncompress.md) | `(string $data, int $max_length = 0): mixed` | `mixed` | ✓ | ✓ |
| [`hash()`](./string/hash.md) | `(string $algo, string $data, bool $binary = false): string` | `string` | ✓ | ✓ |
| [`hash_algos()`](./string/hash_algos.md) | `(): array` | `array` | ✓ | ✓ |
| [`hash_copy()`](./string/hash_copy.md) | `(HashContext $context): HashContext` | `HashContext` | ✓ | ✓ |
| [`hash_equals()`](./string/hash_equals.md) | `(string $known_string, string $user_string): bool` | `bool` | ✓ | ✓ |
| [`hash_final()`](./string/hash_final.md) | `(HashContext $context, bool $binary = false): string` | `string` | ✓ | ✓ |
| [`hash_hmac()`](./string/hash_hmac.md) | `(string $algo, string $data, string $key, bool $binary = false): string` | `string` | ✓ | ✓ |
| [`hash_init()`](./string/hash_init.md) | `(string $algo): HashContext` | `HashContext` | ✓ | ✓ |
| [`hash_update()`](./string/hash_update.md) | `(HashContext $context, string $data): bool` | `bool` | ✓ | ✓ |
| [`hex2bin()`](./string/hex2bin.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`html_entity_decode()`](./string/html_entity_decode.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`htmlentities()`](./string/htmlentities.md) | `(string $string, int $flags = 11, string $encoding = 'UTF-8'): string` | `string` | ✓ | ✓ |
| [`htmlspecialchars()`](./string/htmlspecialchars.md) | `(string $string, int $flags = 11, string $encoding = 'UTF-8'): string` | `string` | ✓ | ✓ |
| [`iconv()`](./string/iconv.md) | `(string $from_encoding, string $to_encoding, string $string): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_get_encoding()`](./string/iconv_get_encoding.md) | `(string $type = 'all'): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_mime_decode()`](./string/iconv_mime_decode.md) | `(string $string, int $mode = 0, string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_mime_decode_headers()`](./string/iconv_mime_decode_headers.md) | `(string $headers, int $mode = 0, string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_mime_encode()`](./string/iconv_mime_encode.md) | `(string $field_name, string $field_value, mixed $options = []): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_set_encoding()`](./string/iconv_set_encoding.md) | `(string $type, string $encoding): bool` | `bool` | ✓ | ✓ |
| [`iconv_strlen()`](./string/iconv_strlen.md) | `(string $string, string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_strpos()`](./string/iconv_strpos.md) | `(string $haystack, string $needle, int $offset = 0, string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_strrpos()`](./string/iconv_strrpos.md) | `(string $haystack, string $needle, string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_substr()`](./string/iconv_substr.md) | `(string $string, int $offset, int $length = null, string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`implode()`](./string/implode.md) | `(string $separator, array $array = null): string` | `string` | ✓ | ✓ |
| [`inet_ntop()`](./string/inet_ntop.md) | `(string $ip): mixed` | `mixed` | ✓ | ✓ |
| [`inet_pton()`](./string/inet_pton.md) | `(string $ip): mixed` | `mixed` | ✓ | ✓ |
| [`ip2long()`](./string/ip2long.md) | `(string $ip): mixed` | `mixed` | ✓ | ✓ |
| [`join()`](./string/join.md) | `(mixed $separator, mixed $array = null): string` | `string` | ✓ | — |
| [`lcfirst()`](./string/lcfirst.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`long2ip()`](./string/long2ip.md) | `(int $ip): string` | `string` | ✓ | ✓ |
| [`ltrim()`](./string/ltrim.md) | `(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string` | `string` | ✓ | ✓ |
| [`mb_strlen()`](./string/mb_strlen.md) | `(string $string, string $encoding = null): int` | `int` | ✓ | ✓ |
| [`md5()`](./string/md5.md) | `(string $string, bool $binary = false): string` | `string` | ✓ | ✓ |
| [`nl2br()`](./string/nl2br.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`number_format()`](./string/number_format.md) | `(float $num, int $decimals = 0, string $decimal_separator = '.', string $thousands_separator = ','): string` | `string` | ✓ | ✓ |
| [`openssl_cipher_iv_length()`](./string/openssl_cipher_iv_length.md) | `(string $cipher_algo): mixed` | `mixed` | ✓ | ✓ |
| [`openssl_decrypt()`](./string/openssl_decrypt.md) | `(string $data, string $cipher_algo, string $passphrase, int $options = 0, string $iv = '', mixed $tag = null, string $aad = ''): mixed` | `mixed` | ✓ | ✓ |
| [`openssl_encrypt()`](./string/openssl_encrypt.md) | `(string $data, string $cipher_algo, string $passphrase, int $options = 0, string $iv = '', mixed $tag = null, string $aad = '', int $tag_length = 16): mixed` | `mixed` | ✓ | ✓ |
| [`openssl_get_cipher_methods()`](./string/openssl_get_cipher_methods.md) | `(bool $aliases = false): array` | `array` | ✓ | ✓ |
| [`ord()`](./string/ord.md) | `(string $character): int` | `int` | ✓ | ✓ |
| [`parse_url()`](./string/parse_url.md) | `(string $url, int $component = -1): mixed` | `mixed` | ✓ | ✓ |
| [`printf()`](./string/printf.md) | `(string $format, ...$values): int` | `int` | ✓ | ✓ |
| [`quoted_printable_encode()`](./string/quoted_printable_encode.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`quotemeta()`](./string/quotemeta.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`rawurldecode()`](./string/rawurldecode.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`rawurlencode()`](./string/rawurlencode.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`rtrim()`](./string/rtrim.md) | `(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string` | `string` | ✓ | ✓ |
| [`sha1()`](./string/sha1.md) | `(string $string, bool $binary = false): string` | `string` | ✓ | ✓ |
| [`similar_text()`](./string/similar_text.md) | `(string $string1, string $string2, mixed $percent = null): int` | `int` | ✓ | — |
| [`sprintf()`](./string/sprintf.md) | `(string $format, ...$values): string` | `string` | ✓ | ✓ |
| [`sscanf()`](./string/sscanf.md) | `(string $string, string $format, ...$vars): mixed` | `mixed` | ✓ | ✓ |
| [`str_contains()`](./string/str_contains.md) | `(string $haystack, string $needle): bool` | `bool` | ✓ | ✓ |
| [`str_ends_with()`](./string/str_ends_with.md) | `(string $haystack, string $needle): bool` | `bool` | ✓ | ✓ |
| [`str_getcsv()`](./string/str_getcsv.md) | `(string $string, string $separator = ',', string $enclosure = '"', string $escape = '\\'): array` | `array` | ✓ | ✓ |
| [`str_ireplace()`](./string/str_ireplace.md) | `(string $search, string $replace, string $subject, int $count = null): string` | `string` | ✓ | ✓ |
| [`str_pad()`](./string/str_pad.md) | `(string $string, int $length, string $pad_string = ' ', int $pad_type = 1): string` | `string` | ✓ | ✓ |
| [`str_repeat()`](./string/str_repeat.md) | `(string $string, int $times): string` | `string` | ✓ | ✓ |
| [`str_replace()`](./string/str_replace.md) | `(string $search, string $replace, string $subject, int $count = null): string` | `string` | ✓ | ✓ |
| [`str_split()`](./string/str_split.md) | `(string $string, int $length = 1): array` | `array` | ✓ | ✓ |
| [`str_starts_with()`](./string/str_starts_with.md) | `(string $haystack, string $needle): bool` | `bool` | ✓ | ✓ |
| [`str_word_count()`](./string/str_word_count.md) | `(string $string, int $format = 0, string $characters = null): array|int` | `array|int` | ✓ | ✓ |
| [`strcasecmp()`](./string/strcasecmp.md) | `(string $string1, string $string2): int` | `int` | ✓ | ✓ |
| [`strcmp()`](./string/strcmp.md) | `(string $string1, string $string2): int` | `int` | ✓ | ✓ |
| [`stripos()`](./string/stripos.md) | `(string $haystack, string $needle, int $offset = 0): mixed` | `mixed` | ✓ | ✓ |
| [`stripslashes()`](./string/stripslashes.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`strlen()`](./string/strlen.md) | `(string $string): int` | `int` | ✓ | ✓ |
| [`strncasecmp()`](./string/strncasecmp.md) | `(string $string1, string $string2, int $length): int` | `int` | ✓ | — |
| [`strncmp()`](./string/strncmp.md) | `(string $string1, string $string2, int $length): int` | `int` | ✓ | — |
| [`strpos()`](./string/strpos.md) | `(string $haystack, string $needle, int $offset = 0): mixed` | `mixed` | ✓ | ✓ |
| [`strrev()`](./string/strrev.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`strripos()`](./string/strripos.md) | `(string $haystack, string $needle, int $offset = 0): mixed` | `mixed` | ✓ | ✓ |
| [`strrpos()`](./string/strrpos.md) | `(string $haystack, string $needle, int $offset = 0): mixed` | `mixed` | ✓ | ✓ |
| [`strstr()`](./string/strstr.md) | `(string $haystack, string $needle, bool $before_needle = false): mixed` | `mixed` | ✓ | ✓ |
| [`strtolower()`](./string/strtolower.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`strtoupper()`](./string/strtoupper.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`strtr()`](./string/strtr.md) | `(string $string, array|string $from, string $to = null): string` | `string` | ✓ | ✓ |
| [`substr()`](./string/substr.md) | `(string $string, int $offset, int $length = null): string` | `string` | ✓ | ✓ |
| [`substr_count()`](./string/substr_count.md) | `(string $haystack, string $needle, int $offset = 0, mixed $length = null): int` | `int` | ✓ | — |
| [`substr_replace()`](./string/substr_replace.md) | `(string $string, string $replace, int $offset, int $length = null): string` | `string` | ✓ | ✓ |
| [`trim()`](./string/trim.md) | `(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string` | `string` | ✓ | ✓ |
| [`ucfirst()`](./string/ucfirst.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`ucwords()`](./string/ucwords.md) | `(string $string, string $separators = ' \t\r\n\x0c\x0b'): string` | `string` | ✓ | ✓ |
| [`urldecode()`](./string/urldecode.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`urlencode()`](./string/urlencode.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`vprintf()`](./string/vprintf.md) | `(string $format, array $values): int` | `int` | ✓ | ✓ |
| [`vsprintf()`](./string/vsprintf.md) | `(string $format, array $values): string` | `string` | ✓ | ✓ |
| [`wordwrap()`](./string/wordwrap.md) | `(string $string, int $width = 75, string $break = '\n', bool $cut_long_words = false): string` | `string` | ✓ | ✓ |
| [`zlib_decode()`](./string/zlib_decode.md) | `(string $data, int $max_length = 0): mixed` | `mixed` | ✓ | — |
| [`zlib_encode()`](./string/zlib_encode.md) | `(string $data, int $encoding, int $level = -1): mixed` | `mixed` | ✓ | — |
| [`zlib_get_coding_type()`](./string/zlib_get_coding_type.md) | `(): mixed` | `mixed` | ✓ | — |
