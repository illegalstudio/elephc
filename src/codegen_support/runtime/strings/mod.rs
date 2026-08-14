//! Purpose:
//! Collects string, formatting, encoding, hashing, and resource string runtime emitters.
//! The module owns re-export wiring for helpers that operate on PHP pointer/length string pairs.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` during the string runtime section.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.

mod itoa;
mod concat;
mod concat_scratch;
mod ftoa;
mod php_num_scan;
mod str_eq;
mod str_loose_eq;
mod str_to_number;
mod str_to_int;
mod str_to_int_base;
mod number_format;
mod atoi;
mod grapheme_strrev;
mod strcopy;
mod str_inc_dec;
mod str_persist;
mod strtolower;
mod strtoupper;
mod trim;
mod ltrim;
mod rtrim;
mod strpos;
mod strrpos;
mod stripos;
mod strripos;
mod str_repeat;
mod strrev;
mod chr;
mod strcmp;
mod strcasecmp;
mod strncmp;
mod strncasecmp;
mod str_starts_with;
mod str_ends_with;
mod str_replace;
mod explode;
mod implode;
mod implode_bool;
mod implode_int;
mod ucwords;
mod str_ireplace;
mod substr_count;
mod substr_replace;
mod str_pad;
mod str_split;
mod str_word_count;
mod addslashes;
mod stripslashes;
mod nl2br;
mod base_convert;
mod chunk_split;
mod count_chars;
mod strtr;
mod quotemeta;
mod quoted_printable_encode;
mod wordwrap;
mod bin2hex;
mod base_to_number;
mod dec_to_base;
mod hex2bin;
mod inet_ntop;
mod inet_pton;
mod ip2long;
mod long2ip;
mod htmlspecialchars;
mod html_entity_decode;
mod urlencode;
mod urldecode;
mod rawurlencode;
mod base64_encode;
mod base64_decode;
mod sprintf;
mod sprintf_pack_mixed;
mod sprintf_x86_64;
mod vsprintf;
mod md5;
mod sha1;
mod crc32;
mod mb_strlen;
mod hash;
pub(crate) mod hash_algos;
mod hash_context;
mod hash_equals;
mod hash_hmac;
mod openssl_methods;
mod openssl;
mod digest_to_string;
mod sscanf;
mod rtrim_mask;
mod ltrim_mask;
mod trim_mask;
mod resource_to_string;
mod resource_type_name;
mod resource_write_stdout;
mod parse_url;

pub use itoa::emit_itoa;
/// Emit integer-to-string conversion helper.
pub use concat::emit_concat;
/// Emit string concatenation helper.
pub use ftoa::{emit_ftoa, emit_ftoa_repr};
pub use php_num_scan::emit_php_num_scan;
/// Emit float-to-string conversion helper.
pub use str_eq::emit_str_eq;
/// Emit case-sensitive string equality check.
pub use str_loose_eq::emit_str_loose_eq;
/// Emit case-insensitive string equality check.
pub use str_to_number::emit_str_to_number;
/// Emit string-to-int parameter coercion validation helper.
pub use str_to_number::emit_str_looks_like_int_for_coercion;
/// Emit string-to-number conversion helper.
pub use str_to_int::emit_str_to_int;
pub use str_to_int_base::emit_str_to_int_base;
/// Emit PHP string-to-integer cast helper.
pub use number_format::emit_number_format;
/// Emit number formatting helper.
pub use atoi::emit_atoi;
/// Emit ASCII-to-integer conversion.
pub use strcopy::emit_strcopy;
/// Emit string copy helper.
pub use concat_scratch::emit_concat_scratch;
pub use str_persist::emit_str_persist;
/// Emit string persistence helper.
pub use str_inc_dec::{emit_mixed_inc_dec, emit_str_inc_dec};
/// Emit PHP's `++`/`--` on a string value and its boxed dispatch entry point.
pub use strtolower::emit_strtolower;
/// Emit lowercase string conversion.
pub use strtoupper::emit_strtoupper;
/// Emit uppercase string conversion.
pub use trim::emit_trim;
/// Emit trim helper (strips whitespace from both ends).
pub use ltrim::emit_ltrim;
/// Emit left trim helper.
pub use rtrim::emit_rtrim;
/// Emit right trim helper.
pub use strpos::emit_strpos;
/// Emit string position lookup (first occurrence).
pub use strrpos::emit_strrpos;
/// Emit string position lookup (last occurrence).
pub use stripos::emit_stripos;
/// Emit case-insensitive string position lookup (first occurrence).
pub use strripos::emit_strripos;
/// Emit case-insensitive string position lookup (last occurrence).
pub use str_repeat::emit_str_repeat;
/// Emit string repeat helper.
pub use strrev::emit_strrev;
/// Emit string reverse helper.
pub use grapheme_strrev::emit_grapheme_strrev;
/// Emit grapheme-aware string reverse helper.
pub use chr::emit_chr;
/// Emit character code to string helper.
pub use strcmp::emit_strcmp;
/// Emit case-sensitive string comparison.
pub use strcasecmp::emit_strcasecmp;
/// Emit length-limited case-sensitive string comparison.
pub use strncmp::emit_strncmp;
/// Emit length-limited case-insensitive string comparison.
pub use strncasecmp::emit_strncasecmp;
/// Emit case-insensitive string comparison.
pub use str_starts_with::emit_str_starts_with;
/// Emit check for string prefix match.
pub use str_ends_with::emit_str_ends_with;
/// Emit check for string suffix match.
pub use str_replace::emit_str_replace;
/// Emit string replace helper.
pub use explode::emit_explode;
/// Emit explode (split by delimiter) helper.
pub use implode::emit_implode;
/// Emit implode (join array to string) helper.
pub use implode_int::emit_implode_int;
/// Emit integer-optimized implode helper.
pub use implode_bool::emit_implode_bool;
/// Emit bool-element implode helper (`true` → `"1"`, `false` → `""`).
pub use ucwords::emit_ucwords;
/// Emit uppercase-words helper.
pub use str_ireplace::emit_str_ireplace;
/// Emit case-insensitive string replace.
pub use substr_count::emit_substr_count;
/// Emit the non-overlapping substring occurrence counter.
pub use substr_replace::emit_substr_replace;
/// Emit substring replace helper.
pub use str_pad::emit_str_pad;
/// Emit string padding helper.
pub use str_split::emit_str_split;
/// Emit string to array split helper.
pub use addslashes::emit_addslashes;
/// Emit addslashes escaping helper.
pub use stripslashes::emit_stripslashes;
/// Emit stripslashes unescaping helper.
pub use nl2br::emit_nl2br;
/// Emit the base_convert numeral re-renderer.
pub use base_convert::emit_base_convert;
/// Emit the chunk_split fixed-length splitter.
pub use chunk_split::emit_chunk_split;
/// Emit the count_chars byte-frequency tally.
pub use count_chars::emit_count_chars;
/// Emit the strtr pairwise and replacement-pair translators.
pub use strtr::emit_strtr;
/// Emit the quotemeta regular-expression metacharacter escaper.
pub use quotemeta::emit_quotemeta;
/// Emit the quoted_printable_encode MIME transfer encoder.
pub use quoted_printable_encode::emit_quoted_printable_encode;
/// Emit the str_word_count word scanner.
pub use str_word_count::emit_str_word_count;
/// Emit newline to `<br>` conversion.
pub use wordwrap::emit_wordwrap;
/// Emit wordwrap helper.
pub use bin2hex::emit_bin2hex;
/// Emit the shared unsigned integer-to-base renderer used by dechex/decbin/decoct.
pub use dec_to_base::emit_dec_to_base;
/// Emit the shared base-digit parser used by hexdec/bindec/octdec.
pub use base_to_number::emit_base_to_number;
/// Emit binary-to-hexadecimal encoding.
pub use hex2bin::emit_hex2bin;
/// Emit hexadecimal-to-binary decoding.
pub use inet_ntop::emit_inet_ntop;
/// Emit inet_ntop (network address to string) conversion.
pub use inet_pton::emit_inet_pton;
/// Emit inet_pton (string to network address) conversion.
pub use ip2long::emit_ip2long;
/// Emit IP-to-long address conversion.
pub use long2ip::emit_long2ip;
/// Emit long-to-IP address conversion.
pub use htmlspecialchars::emit_htmlspecialchars;
/// Emit HTML special characters escaping.
pub use html_entity_decode::emit_html_entity_decode;
/// Emit HTML entity decode helper.
pub use urlencode::emit_urlencode;
/// Emit URL encoding helper.
pub use urldecode::emit_urldecode;
/// Emit URL decoding helper.
pub use rawurlencode::emit_rawurlencode;
/// Emit raw URL encoding helper.
pub use base64_encode::emit_base64_encode;
/// Emit Base64 encoding helper.
pub use base64_decode::emit_base64_decode;
/// Emit Base64 decoding helper.
/// Re-export the php-src reverse-table sentinels so `runtime::data::fixed` builds
/// `_b64_decode_tbl` from the exact classification `__rt_base64_decode` reads back.
pub use base64_decode::{B64_DECODE_INVALID, B64_DECODE_SKIP, B64_DECODE_WHITESPACE};
pub use sprintf::emit_sprintf;
pub use sprintf_pack_mixed::emit_sprintf_pack_mixed;
pub use vsprintf::emit_vsprintf;
/// Emit sprintf formatting helper.
pub use md5::emit_md5;
/// Emit MD5 hash helper.
pub use sha1::emit_sha1;
/// Emit CRC-32 checksum helper.
pub use crc32::emit_crc32;
/// Emit mb_strlen UTF-8 code-point-count helper.
pub use mb_strlen::emit_mb_strlen;
/// Emit SHA1 hash helper.
pub use hash::emit_hash;
/// Emit generic hash helper.
pub use hash_hmac::emit_hash_hmac;
pub use openssl_methods::emit_openssl_methods;
pub use openssl::emit_openssl_cipher;
/// Emit keyed-hash HMAC helper.
pub use hash_algos::emit_hash_algos_list;
pub use hash_context::emit_hash_context;
/// Emit the incremental HashContext helpers (hash_init/update/final/copy).
/// Emit the supported-hash-algorithm-name list helper.
pub use hash_equals::emit_hash_equals;
/// Emit timing-safe string-equality helper.
pub use digest_to_string::emit_digest_to_string;
/// Emit the shared raw-digest-to-PHP-string formatter used by hash/md5/sha1.
pub use sscanf::emit_sscanf;
/// Emit string scanf parsing helper.
pub use rtrim_mask::emit_rtrim_mask;
/// Emit right trim with custom mask helper.
pub use ltrim_mask::emit_ltrim_mask;
/// Emit left trim with custom mask helper.
pub use trim_mask::emit_trim_mask;
/// Emit trim with custom mask helper.
pub use resource_to_string::emit_resource_to_string;
/// Emit resource-to-string conversion.
pub use resource_type_name::emit_resource_type_name;
/// Emit the resource type-name resolver (`stream` when open, `Unknown` once closed).
pub use resource_write_stdout::emit_resource_write_stdout;
/// Emit the PHP-compatible URL component scanner.
pub use parse_url::emit_parse_url;
