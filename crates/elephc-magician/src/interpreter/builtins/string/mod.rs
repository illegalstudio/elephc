//! Purpose:
//! Per-builtin declarations for string functions migrated to the eval builtin
//! registry.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!`.

mod addslashes;
mod base64_decode;
mod base64_encode;
mod bin2hex;
mod chr;
mod crc32;
mod ctype_alnum;
mod ctype_alpha;
mod ctype_digit;
mod ctype_space;
mod hex2bin;
mod ord;
mod rawurldecode;
mod rawurlencode;
mod strlen;
mod str_repeat;
mod strrev;
mod stripslashes;
mod urldecode;
mod urlencode;
