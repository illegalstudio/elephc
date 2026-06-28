---
title: "String builtins"
description: "Builtins in the String category."
sidebar:
  order: 101
---

## String builtins

| Function | Signature | Returns |
|---|---|---|
| [`addslashes()`](./string/addslashes.md) | `(string $string): string` | `string` |
| [`base64_decode()`](./string/base64_decode.md) | `(string $string, bool $strict): string` | `string` |
| [`base64_encode()`](./string/base64_encode.md) | `(string $string): string` | `string` |
| [`bin2hex()`](./string/bin2hex.md) | `(string $string): string` | `string` |
| [`chop()`](./string/chop.md) | `(string $string, string $characters): string` | `string` |
| [`chr()`](./string/chr.md) | `(int $codepoint): string` | `string` |
| [`crc32()`](./string/crc32.md) | `(string $string): int` | `int` |
| [`explode()`](./string/explode.md) | `(string $separator, string $string, int $limit): array` | `array` |
| [`grapheme_strrev()`](./string/grapheme_strrev.md) | `(string $string): mixed` | `mixed` |
| [`gzcompress()`](./string/gzcompress.md) | `(string $data, int $level, int $encoding): string` | `string` |
| [`gzdeflate()`](./string/gzdeflate.md) | `(string $data, int $level, int $encoding): string` | `string` |
| [`gzinflate()`](./string/gzinflate.md) | `(string $data, int $max_length): string` | `string` |
| [`gzuncompress()`](./string/gzuncompress.md) | `(string $data, int $max_length): string` | `string` |
| [`hash()`](./string/hash.md) | `(string $algo, string $data, bool $binary = false, array $options = []): string` | `string` |
| [`hash_algos()`](./string/hash_algos.md) | `(): array` | `array` |
| [`hash_copy()`](./string/hash_copy.md) | `(resource $context): mixed` | `mixed` |
| [`hash_equals()`](./string/hash_equals.md) | `(string $known_string, string $user_string): bool` | `bool` |
| [`hash_final()`](./string/hash_final.md) | `(resource $context, bool $binary): string` | `string` |
| [`hash_hmac()`](./string/hash_hmac.md) | `(string $algo, string $data, string $key, bool $binary): string` | `string` |
| [`hash_init()`](./string/hash_init.md) | `(string $algo, int $flags = 0, string $key = '', array $options = []): mixed` | `mixed` |
| [`hash_update()`](./string/hash_update.md) | `(resource $context, string $data): bool` | `bool` |
| [`hex2bin()`](./string/hex2bin.md) | `(string $string): string` | `string` |
| [`html_entity_decode()`](./string/html_entity_decode.md) | `(string $string, int $flags, string $encoding): string` | `string` |
| [`htmlentities()`](./string/htmlentities.md) | `(string $string, int $flags, string $encoding, bool $double_encode): string` | `string` |
| [`htmlspecialchars()`](./string/htmlspecialchars.md) | `(string $string, int $flags, string $encoding, bool $double_encode): string` | `string` |
| [`implode()`](./string/implode.md) | `(string $separator, array $array): string` | `string` |
| [`inet_ntop()`](./string/inet_ntop.md) | `(string $ip): mixed` | `mixed` |
| [`inet_pton()`](./string/inet_pton.md) | `(string $ip): mixed` | `mixed` |
| [`ip2long()`](./string/ip2long.md) | `(string $ip): mixed` | `mixed` |
| [`lcfirst()`](./string/lcfirst.md) | `(string $string): string` | `string` |
| [`long2ip()`](./string/long2ip.md) | `(int $ip): string` | `string` |
| [`ltrim()`](./string/ltrim.md) | `(string $string, string $characters): string` | `string` |
| [`md5()`](./string/md5.md) | `(string $string, bool $binary): string` | `string` |
| [`nl2br()`](./string/nl2br.md) | `(string $string, bool $use_xhtml): string` | `string` |
| [`number_format()`](./string/number_format.md) | `(float $num, int $decimals, string $decimal_separator, string $thousands_separator): string` | `string` |
| [`ord()`](./string/ord.md) | `(string $character): int` | `int` |
| [`printf()`](./string/printf.md) | `(string $format, ...$values): int` | `int` |
| [`rawurldecode()`](./string/rawurldecode.md) | `(string $string): string` | `string` |
| [`rawurlencode()`](./string/rawurlencode.md) | `(string $string): string` | `string` |
| [`rtrim()`](./string/rtrim.md) | `(string $string, string $characters): string` | `string` |
| [`sha1()`](./string/sha1.md) | `(string $string, bool $binary): string` | `string` |
| [`sprintf()`](./string/sprintf.md) | `(string $format, ...$values): string` | `string` |
| [`sscanf()`](./string/sscanf.md) | `(string $string, string $format, ...$vars): array` | `array` |
| [`str_contains()`](./string/str_contains.md) | `(string $haystack, string $needle): bool` | `bool` |
| [`str_ends_with()`](./string/str_ends_with.md) | `(string $haystack, string $needle): bool` | `bool` |
| [`str_ireplace()`](./string/str_ireplace.md) | `(mixed $search, mixed $replace, mixed $subject, int $count): mixed` | `mixed` |
| [`str_pad()`](./string/str_pad.md) | `(string $string, int $length, string $pad_string, int $pad_type): string` | `string` |
| [`str_repeat()`](./string/str_repeat.md) | `(string $string, int $times): string` | `string` |
| [`str_replace()`](./string/str_replace.md) | `(string $search, string $replace, string $subject, int $count): mixed` | `mixed` |
| [`str_split()`](./string/str_split.md) | `(string $string, int $length): array` | `array` |
| [`str_starts_with()`](./string/str_starts_with.md) | `(string $haystack, string $needle): bool` | `bool` |
| [`strcasecmp()`](./string/strcasecmp.md) | `(string $string1, string $string2): int` | `int` |
| [`strcmp()`](./string/strcmp.md) | `(string $string1, string $string2): int` | `int` |
| [`stripslashes()`](./string/stripslashes.md) | `(string $string): string` | `string` |
| [`strlen()`](./string/strlen.md) | `(string $string): int` | `int` |
| [`strpos()`](./string/strpos.md) | `(string $haystack, string $needle, int $offset): mixed` | `mixed` |
| [`strrev()`](./string/strrev.md) | `(string $string): string` | `string` |
| [`strrpos()`](./string/strrpos.md) | `(string $haystack, string $needle, int $offset): mixed` | `mixed` |
| [`strstr()`](./string/strstr.md) | `(string $haystack, string $needle, bool $before_needle): string` | `string` |
| [`strtolower()`](./string/strtolower.md) | `(string $string): string` | `string` |
| [`strtoupper()`](./string/strtoupper.md) | `(string $string): string` | `string` |
| [`substr()`](./string/substr.md) | `(string $string, int $offset, int $length): string` | `string` |
| [`substr_replace()`](./string/substr_replace.md) | `(string $string, string $replace, int $offset, int $length): string` | `string` |
| [`trim()`](./string/trim.md) | `(string $string, string $characters): string` | `string` |
| [`ucfirst()`](./string/ucfirst.md) | `(string $string): string` | `string` |
| [`ucwords()`](./string/ucwords.md) | `(string $string, string $separators): string` | `string` |
| [`urldecode()`](./string/urldecode.md) | `(string $string): string` | `string` |
| [`urlencode()`](./string/urlencode.md) | `(string $string): string` | `string` |
| [`vprintf()`](./string/vprintf.md) | `(string $format, array $values): int` | `int` |
| [`vsprintf()`](./string/vsprintf.md) | `(string $format, array $values): string` | `string` |
| [`wordwrap()`](./string/wordwrap.md) | `(string $string, int $width, string $break, bool $cut_long_words): string` | `string` |
