//! Purpose:
//! Groups all `string`-area builtin homes into this module so the registry can
//! collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its checker contract and typed runtime target.
//!
//! Called from:
//! - `crate::builtins` (`mod string;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Add `pub mod <name>;` here for every new string builtin home.
//! - Pure-data builtins (no check hook) only need a `lower` fn; the `builtin!`
//!   `returns:` field provides the declared return type.

// The four incremental hash-context builtins are `internal: true` and carry
// `__elephc_` names: PHP's `hash_init`/`hash_update`/`hash_final`/`hash_copy` are
// elephc-PHP wrappers declared by `crate::hash_prelude`, which returns a real
// `HashContext` object. A prelude function cannot shadow a builtin of the same
// name, so the raw builtins had to be renamed out of the way.
pub mod __elephc_hash_ctx_copy;
pub mod __elephc_hash_ctx_final;
pub mod __elephc_hash_ctx_init;
pub mod __elephc_hash_ctx_update;
pub mod addslashes;
pub mod base64_decode;
pub mod base64_encode;
pub mod bin2hex;
pub mod chop;
pub mod chr;
pub mod chunk_split;
pub mod count_chars;
pub mod crc32;
pub mod ctype_alnum;
pub mod ctype_alpha;
pub mod ctype_digit;
pub mod ctype_space;
pub mod explode;
pub mod grapheme_strrev;
pub mod gzcompress;
pub mod gzdeflate;
pub mod gzinflate;
pub mod gzuncompress;
pub mod hash;
pub mod hash_algos;
pub mod hash_equals;
pub mod hash_hmac;
pub mod hex2bin;
pub mod html_entity_decode;
pub mod htmlentities;
pub mod htmlspecialchars;
pub mod iconv;
pub mod iconv_get_encoding;
pub mod iconv_mime_decode;
pub mod iconv_mime_decode_headers;
pub mod iconv_mime_encode;
pub mod iconv_set_encoding;
pub mod iconv_strlen;
pub mod iconv_strpos;
pub mod iconv_strrpos;
pub mod iconv_substr;
pub mod implode;
pub mod inet_ntop;
pub mod inet_pton;
pub mod ip2long;
pub mod join;
pub mod lcfirst;
pub mod long2ip;
pub mod ltrim;
pub mod mb_ereg_match;
pub mod mb_strlen;
pub mod md5;
pub mod nl2br;
pub mod number_format;
pub mod ord;
pub mod openssl_cipher_iv_length;
pub mod openssl_decrypt;
pub mod openssl_encrypt;
pub mod openssl_get_cipher_methods;
pub mod parse_url;
pub mod str_getcsv;
pub mod printf;
pub mod quoted_printable_encode;
pub mod quotemeta;
pub mod rawurldecode;
pub mod rawurlencode;
pub mod rtrim;
pub mod sha1;
pub mod sprintf;
pub mod similar_text;
pub mod sscanf;
pub mod str_contains;
pub mod str_ends_with;
pub mod str_ireplace;
pub mod str_pad;
pub mod str_repeat;
pub mod str_replace;
pub mod str_split;
pub mod str_starts_with;
pub mod str_word_count;
pub mod strcasecmp;
pub mod strcmp;
pub mod stripslashes;
pub mod strlen;
pub mod strncasecmp;
pub mod strncmp;
pub mod stripos;
pub mod strpos;
pub mod strrev;
pub mod strripos;
pub mod strrpos;
pub mod strstr;
pub mod strtolower;
pub mod strtoupper;
pub mod strtr;
pub mod substr;
pub mod substr_count;
pub mod substr_replace;
pub mod trim;
pub mod ucfirst;
pub mod ucwords;
pub mod urldecode;
pub mod urlencode;
pub mod vprintf;
pub mod vsprintf;
pub mod wordwrap;
