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
| [`chop()`](./string/chop.md) | `(string $string, string $characters = " \n\r\t\x0B\x0C\x00"): string` | `string` | ✓ | ✓ |
| [`chr()`](./string/chr.md) | `(int $codepoint): string` | `string` | ✓ | ✓ |
| [`chunk_split()`](./string/chunk_split.md) | `(string $string, int $length = 76, string $separator = "\r\n"): string` | `string` | ✓ | ✓ |
| [`count_chars()`](./string/count_chars.md) | `(string $string, int $mode = 0): array|string` | `array|string` | ✓ | ✓ |
| [`crc32()`](./string/crc32.md) | `(string $string): int` | `int` | ✓ | ✓ |
| [`explode()`](./string/explode.md) | `(string $separator, string $string, int $limit = PHP_INT_MAX): array` | `array` | ✓ | ✓ |
| [`grapheme_strrev()`](./string/grapheme_strrev.md) | `(string $string): mixed` | `mixed` | ✓ | ✓ |
| [`gzcompress()`](./string/gzcompress.md) | `(string $data, int $level = -1): string` | `string` | ✓ | ✓ |
| [`gzdeflate()`](./string/gzdeflate.md) | `(string $data, int $level = -1): string` | `string` | ✓ | ✓ |
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
| [`iconv_mime_decode()`](./string/iconv_mime_decode.md) | `(string $string, int $mode = 0, ?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_mime_decode_headers()`](./string/iconv_mime_decode_headers.md) | `(string $headers, int $mode = 0, ?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_mime_encode()`](./string/iconv_mime_encode.md) | `(string $field_name, string $field_value, mixed $options = []): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_set_encoding()`](./string/iconv_set_encoding.md) | `(string $type, string $encoding): bool` | `bool` | ✓ | ✓ |
| [`iconv_strlen()`](./string/iconv_strlen.md) | `(string $string, ?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_strpos()`](./string/iconv_strpos.md) | `(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_strrpos()`](./string/iconv_strrpos.md) | `(string $haystack, string $needle, ?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`iconv_substr()`](./string/iconv_substr.md) | `(string $string, int $offset, ?int $length = null, ?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`implode()`](./string/implode.md) | `(string $separator, array $array = null): string` | `string` | ✓ | ✓ |
| [`inet_ntop()`](./string/inet_ntop.md) | `(string $ip): mixed` | `mixed` | ✓ | ✓ |
| [`inet_pton()`](./string/inet_pton.md) | `(string $ip): mixed` | `mixed` | ✓ | ✓ |
| [`ip2long()`](./string/ip2long.md) | `(string $ip): mixed` | `mixed` | ✓ | ✓ |
| [`join()`](./string/join.md) | `(mixed $separator, mixed $array = null): string` | `string` | ✓ | - |
| [`lcfirst()`](./string/lcfirst.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`long2ip()`](./string/long2ip.md) | `(int $ip): string` | `string` | ✓ | ✓ |
| [`ltrim()`](./string/ltrim.md) | `(string $string, string $characters = " \n\r\t\x0B\x0C\x00"): string` | `string` | ✓ | ✓ |
| [`mb_check_encoding()`](./string/mb_check_encoding.md) | `(array|string|null $value = null, ?string $encoding = null): bool` | `bool` | ✓ | ✓ |
| [`mb_chr()`](./string/mb_chr.md) | `(int $codepoint, ?string $encoding = null): string|false` | `string|false` | ✓ | ✓ |
| [`mb_convert_case()`](./string/mb_convert_case.md) | `(string $string, int $mode, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_convert_encoding()`](./string/mb_convert_encoding.md) | `(array|string $string, string $to_encoding, array|string|null $from_encoding = null): array|string|false` | `array|string|false` | ✓ | ✓ |
| [`mb_convert_kana()`](./string/mb_convert_kana.md) | `(string $string, string $mode = 'KV', ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_decode_mimeheader()`](./string/mb_decode_mimeheader.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`mb_decode_numericentity()`](./string/mb_decode_numericentity.md) | `(string $string, array $map, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_detect_encoding()`](./string/mb_detect_encoding.md) | `(string $string, array|string|null $encodings = null, bool $strict = false): string|false` | `string|false` | ✓ | ✓ |
| [`mb_detect_order()`](./string/mb_detect_order.md) | `(array|string|null $encoding = null): array|bool` | `array|bool` | ✓ | ✓ |
| [`mb_encode_mimeheader()`](./string/mb_encode_mimeheader.md) | `(string $string, ?string $charset = null, ?string $transfer_encoding = null, string $newline = "\r\n", int $indent = 0): string` | `string` | ✓ | ✓ |
| [`mb_encode_numericentity()`](./string/mb_encode_numericentity.md) | `(string $string, array $map, ?string $encoding = null, bool $hex = false): string` | `string` | ✓ | ✓ |
| [`mb_encoding_aliases()`](./string/mb_encoding_aliases.md) | `(string $encoding): array` | `array` | ✓ | ✓ |
| [`mb_ereg()`](./string/mb_ereg.md) | `(string $pattern, string $string, mixed $matches = null): bool` | `bool` | ✓ | ✓ |
| [`mb_ereg_replace()`](./string/mb_ereg_replace.md) | `(string $pattern, string $replacement, string $string, ?string $options = null): string|false|null` | `string|false|null` | ✓ | ✓ |
| [`mb_ereg_replace_callback()`](./string/mb_ereg_replace_callback.md) | `(string $pattern, callable $callback, string $string, ?string $options = null): string|false|null` | `string|false|null` | ✓ | ✓ |
| [`mb_ereg_search()`](./string/mb_ereg_search.md) | `(?string $pattern = null, ?string $options = null): bool` | `bool` | ✓ | ✓ |
| [`mb_ereg_search_getpos()`](./string/mb_ereg_search_getpos.md) | `(): int` | `int` | ✓ | ✓ |
| [`mb_ereg_search_getregs()`](./string/mb_ereg_search_getregs.md) | `(): array|false` | `array|false` | ✓ | ✓ |
| [`mb_ereg_search_init()`](./string/mb_ereg_search_init.md) | `(string $string, ?string $pattern = null, ?string $options = null): bool` | `bool` | ✓ | ✓ |
| [`mb_ereg_search_pos()`](./string/mb_ereg_search_pos.md) | `(?string $pattern = null, ?string $options = null): array|false` | `array|false` | ✓ | ✓ |
| [`mb_ereg_search_regs()`](./string/mb_ereg_search_regs.md) | `(?string $pattern = null, ?string $options = null): array|false` | `array|false` | ✓ | ✓ |
| [`mb_ereg_search_setpos()`](./string/mb_ereg_search_setpos.md) | `(int $offset): bool` | `bool` | ✓ | ✓ |
| [`mb_eregi()`](./string/mb_eregi.md) | `(string $pattern, string $string, mixed $matches = null): bool` | `bool` | ✓ | ✓ |
| [`mb_eregi_replace()`](./string/mb_eregi_replace.md) | `(string $pattern, string $replacement, string $string, ?string $options = null): string|false|null` | `string|false|null` | ✓ | ✓ |
| [`mb_get_info()`](./string/mb_get_info.md) | `(string $type = 'all'): array|string|int|false|null` | `array|string|int|false|null` | ✓ | ✓ |
| [`mb_http_input()`](./string/mb_http_input.md) | `(?string $type = null): array|string|false` | `array|string|false` | ✓ | ✓ |
| [`mb_http_output()`](./string/mb_http_output.md) | `(?string $encoding = null): string|bool` | `string|bool` | ✓ | ✓ |
| [`mb_internal_encoding()`](./string/mb_internal_encoding.md) | `(?string $encoding = null): string|bool` | `string|bool` | ✓ | ✓ |
| [`mb_language()`](./string/mb_language.md) | `(?string $language = null): string|bool` | `string|bool` | ✓ | ✓ |
| [`mb_lcfirst()`](./string/mb_lcfirst.md) | `(string $string, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_list_encodings()`](./string/mb_list_encodings.md) | `(): array` | `array` | ✓ | ✓ |
| [`mb_ltrim()`](./string/mb_ltrim.md) | `(string $string, ?string $characters = null, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_ord()`](./string/mb_ord.md) | `(string $string, ?string $encoding = null): int|false` | `int|false` | ✓ | ✓ |
| [`mb_output_handler()`](./string/mb_output_handler.md) | `(string $string, int $status): string` | `string` | ✓ | ✓ |
| [`mb_parse_str()`](./string/mb_parse_str.md) | `(string $string, mixed $result): bool` | `bool` | ✓ | ✓ |
| [`mb_preferred_mime_name()`](./string/mb_preferred_mime_name.md) | `(string $encoding): string|false` | `string|false` | ✓ | ✓ |
| [`mb_regex_encoding()`](./string/mb_regex_encoding.md) | `(?string $encoding = null): string|bool` | `string|bool` | ✓ | ✓ |
| [`mb_regex_set_options()`](./string/mb_regex_set_options.md) | `(?string $options = null): string` | `string` | ✓ | ✓ |
| [`mb_rtrim()`](./string/mb_rtrim.md) | `(string $string, ?string $characters = null, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_scrub()`](./string/mb_scrub.md) | `(string $string, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_split()`](./string/mb_split.md) | `(string $pattern, string $string, int $limit = -1): array|false` | `array|false` | ✓ | ✓ |
| [`mb_str_pad()`](./string/mb_str_pad.md) | `(string $string, int $length, string $pad_string = ' ', int $pad_type = 1, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_str_split()`](./string/mb_str_split.md) | `(string $string, int $length = 1, ?string $encoding = null): array` | `array` | ✓ | ✓ |
| [`mb_strcut()`](./string/mb_strcut.md) | `(string $string, int $start, ?int $length = null, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_strimwidth()`](./string/mb_strimwidth.md) | `(string $string, int $start, int $width, string $trim_marker = '', ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_stripos()`](./string/mb_stripos.md) | `(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false` | `int|false` | ✓ | ✓ |
| [`mb_stristr()`](./string/mb_stristr.md) | `(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false` | `string|false` | ✓ | ✓ |
| [`mb_strlen()`](./string/mb_strlen.md) | `(string $string, ?string $encoding = null): int` | `int` | ✓ | ✓ |
| [`mb_strpos()`](./string/mb_strpos.md) | `(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false` | `int|false` | ✓ | ✓ |
| [`mb_strrchr()`](./string/mb_strrchr.md) | `(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false` | `string|false` | ✓ | ✓ |
| [`mb_strrichr()`](./string/mb_strrichr.md) | `(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false` | `string|false` | ✓ | ✓ |
| [`mb_strripos()`](./string/mb_strripos.md) | `(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false` | `int|false` | ✓ | ✓ |
| [`mb_strrpos()`](./string/mb_strrpos.md) | `(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false` | `int|false` | ✓ | ✓ |
| [`mb_strstr()`](./string/mb_strstr.md) | `(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false` | `string|false` | ✓ | ✓ |
| [`mb_strtolower()`](./string/mb_strtolower.md) | `(string $string, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_strtoupper()`](./string/mb_strtoupper.md) | `(string $string, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_strwidth()`](./string/mb_strwidth.md) | `(string $string, ?string $encoding = null): int` | `int` | ✓ | ✓ |
| [`mb_substitute_character()`](./string/mb_substitute_character.md) | `(string|int|null $substitute_character = null): string|int|bool` | `string|int|bool` | ✓ | ✓ |
| [`mb_substr()`](./string/mb_substr.md) | `(string $string, int $start, ?int $length = null, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_substr_count()`](./string/mb_substr_count.md) | `(string $haystack, string $needle, ?string $encoding = null): int` | `int` | ✓ | ✓ |
| [`mb_trim()`](./string/mb_trim.md) | `(string $string, ?string $characters = null, ?string $encoding = null): string` | `string` | ✓ | ✓ |
| [`mb_ucfirst()`](./string/mb_ucfirst.md) | `(string $string, ?string $encoding = null): string` | `string` | ✓ | ✓ |
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
| [`rtrim()`](./string/rtrim.md) | `(string $string, string $characters = " \n\r\t\x0B\x0C\x00"): string` | `string` | ✓ | ✓ |
| [`sha1()`](./string/sha1.md) | `(string $string, bool $binary = false): string` | `string` | ✓ | ✓ |
| [`sprintf()`](./string/sprintf.md) | `(string $format, ...$values): string` | `string` | ✓ | ✓ |
| [`sscanf()`](./string/sscanf.md) | `(string $string, string $format, ...$vars): array` | `array` | ✓ | ✓ |
| [`str_contains()`](./string/str_contains.md) | `(string $haystack, string $needle): bool` | `bool` | ✓ | ✓ |
| [`str_ends_with()`](./string/str_ends_with.md) | `(string $haystack, string $needle): bool` | `bool` | ✓ | ✓ |
| [`str_ireplace()`](./string/str_ireplace.md) | `(string $search, string $replace, string $subject, int $count = null): string` | `string` | ✓ | ✓ |
| [`str_pad()`](./string/str_pad.md) | `(string $string, int $length, string $pad_string = ' ', int $pad_type = 1): string` | `string` | ✓ | ✓ |
| [`str_repeat()`](./string/str_repeat.md) | `(string $string, int $times): string` | `string` | ✓ | ✓ |
| [`str_replace()`](./string/str_replace.md) | `(string $search, string $replace, string $subject, int $count = null): string` | `string` | ✓ | ✓ |
| [`str_split()`](./string/str_split.md) | `(string $string, int $length = 1): array` | `array` | ✓ | ✓ |
| [`str_starts_with()`](./string/str_starts_with.md) | `(string $haystack, string $needle): bool` | `bool` | ✓ | ✓ |
| [`str_word_count()`](./string/str_word_count.md) | `(string $string, int $format = 0, ?string $characters = null): array|int` | `array|int` | ✓ | ✓ |
| [`strcasecmp()`](./string/strcasecmp.md) | `(string $string1, string $string2): int` | `int` | ✓ | ✓ |
| [`strcmp()`](./string/strcmp.md) | `(string $string1, string $string2): int` | `int` | ✓ | ✓ |
| [`stripos()`](./string/stripos.md) | `(string $haystack, string $needle, int $offset = 0): mixed` | `mixed` | ✓ | ✓ |
| [`stripslashes()`](./string/stripslashes.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`strlen()`](./string/strlen.md) | `(string $string): int` | `int` | ✓ | ✓ |
| [`strncasecmp()`](./string/strncasecmp.md) | `(string $string1, string $string2, int $length): int` | `int` | ✓ | - |
| [`strncmp()`](./string/strncmp.md) | `(string $string1, string $string2, int $length): int` | `int` | ✓ | - |
| [`strpos()`](./string/strpos.md) | `(string $haystack, string $needle, int $offset = 0): mixed` | `mixed` | ✓ | ✓ |
| [`strrev()`](./string/strrev.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`strripos()`](./string/strripos.md) | `(string $haystack, string $needle, int $offset = 0): mixed` | `mixed` | ✓ | ✓ |
| [`strrpos()`](./string/strrpos.md) | `(string $haystack, string $needle, int $offset = 0): mixed` | `mixed` | ✓ | ✓ |
| [`strstr()`](./string/strstr.md) | `(string $haystack, string $needle, bool $before_needle = false): mixed` | `mixed` | ✓ | ✓ |
| [`strtolower()`](./string/strtolower.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`strtoupper()`](./string/strtoupper.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`strtr()`](./string/strtr.md) | `(string $string, array|string $from, ?string $to = null): string` | `string` | ✓ | ✓ |
| [`substr()`](./string/substr.md) | `(string $string, int $offset, ?int $length = null): string` | `string` | ✓ | ✓ |
| [`substr_count()`](./string/substr_count.md) | `(string $haystack, string $needle, int $offset = 0, mixed $length = null): int` | `int` | ✓ | - |
| [`substr_replace()`](./string/substr_replace.md) | `(string $string, string $replace, int $offset, int $length = null): string` | `string` | ✓ | ✓ |
| [`trim()`](./string/trim.md) | `(string $string, string $characters = " \n\r\t\x0B\x0C\x00"): string` | `string` | ✓ | ✓ |
| [`ucfirst()`](./string/ucfirst.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`ucwords()`](./string/ucwords.md) | `(string $string, string $separators = " \t\r\n\x0C\x0B"): string` | `string` | ✓ | ✓ |
| [`urldecode()`](./string/urldecode.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`urlencode()`](./string/urlencode.md) | `(string $string): string` | `string` | ✓ | ✓ |
| [`vprintf()`](./string/vprintf.md) | `(string $format, array $values): int` | `int` | ✓ | ✓ |
| [`vsprintf()`](./string/vsprintf.md) | `(string $format, array $values): string` | `string` | ✓ | ✓ |
| [`wordwrap()`](./string/wordwrap.md) | `(string $string, int $width = 75, string $break = "\n", bool $cut_long_words = false): string` | `string` | ✓ | ✓ |
