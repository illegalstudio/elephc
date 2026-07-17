//! Purpose:
//! Groups all `string`-area builtin homes into this module so the registry can
//! collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its type-check and lowering hooks.
//!
//! Called from:
//! - `crate::builtins` (`mod string;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Add `pub mod <name>;` here for every new string builtin home.
//! - Pure-data builtins (no check hook) only need a `lower` fn; the `builtin!`
//!   `returns:` field provides the declared return type.

pub mod addslashes;
pub mod base64_decode;
pub mod base64_encode;
pub mod bin2hex;
pub mod chop;
pub mod chr;
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
pub mod hash_copy;
pub mod hash_equals;
pub mod hash_final;
pub mod hash_hmac;
pub mod hash_init;
pub mod hash_update;
pub mod hex2bin;
pub mod html_entity_decode;
pub mod htmlentities;
pub mod htmlspecialchars;
pub mod implode;
pub mod inet_ntop;
pub mod inet_pton;
pub mod ip2long;
pub mod lcfirst;
pub mod long2ip;
pub mod ltrim;
pub mod mb_ereg_match;
pub mod mb_strlen;
pub mod md5;
pub mod nl2br;
pub mod number_format;
pub mod ord;
pub mod printf;
pub mod rawurldecode;
pub mod rawurlencode;
pub mod rtrim;
pub mod sha1;
pub mod sprintf;
pub mod sscanf;
pub mod str_contains;
pub mod str_ends_with;
pub mod str_ireplace;
pub mod str_pad;
pub mod str_repeat;
pub mod str_replace;
pub mod str_split;
pub mod str_starts_with;
pub mod strcasecmp;
pub mod strcmp;
pub mod stripslashes;
pub mod strlen;
pub mod strpos;
pub mod strrev;
pub mod strrpos;
pub mod strstr;
pub mod strtolower;
pub mod strtoupper;
pub mod substr;
pub mod substr_replace;
pub mod trim;
pub mod ucfirst;
pub mod ucwords;
pub mod urldecode;
pub mod urlencode;
pub mod vprintf;
pub mod vsprintf;
pub mod wordwrap;
