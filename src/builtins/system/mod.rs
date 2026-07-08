//! Purpose:
//! Groups all `system`-area time/date/sleep/env/process/output/define/json/serialize builtin homes
//! into this module so the registry can collect them in one place. Each submodule
//! declares exactly one builtin via `builtin!` and provides its lowering hook (and
//! optional check hook).
//!
//! Called from:
//! - `crate::builtins` (`mod system;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Pure-data builtins (no check hook): time, sleep, usleep, checkdate, date, gmdate,
//!   mktime, gmmktime, hrtime, getdate, localtime, date_default_timezone_get/set,
//!   __elephc_mktime_raw, __elephc_gmmktime_raw, __elephc_strtotime_raw,
//!   putenv, http_response_code, header, phpversion, exec, shell_exec, system, passthru,
//!   json_last_error, json_last_error_msg, serialize, preg_match_all, preg_replace.
//! - Check-hook builtins: microtime (literal-dependent return type), strtotime
//!   (returns Union(Int, Bool)), getenv (returns Union(Str, Bool)), php_uname (validates
//!   arg type), define (side-effect: registers constant type), defined (validates literal),
//!   class_attribute_names/class_attribute_args/class_get_attributes (compile-time reflection),
//!   json_encode, json_decode, json_validate, unserialize, preg_match (by-ref `$matches`
//!   variable check), preg_split (element type refined by arg count).
//! - `attr_support` holds shared helpers for the class-attribute builtins.
//! - `json_support` holds shared helpers for the JSON/serialize check hooks.
//! - Add `pub mod <name>;` here for every new system builtin home.

pub mod __elephc_gmmktime_raw;
pub mod __elephc_mktime_raw;
pub mod __elephc_strtotime_raw;
pub mod attr_support;
pub mod checkdate;
pub mod class_attribute_args;
pub mod class_attribute_names;
pub mod class_get_attributes;
pub mod date;
pub mod date_default_timezone_get;
pub mod date_default_timezone_set;
pub mod define;
pub mod defined;
pub mod exec;
pub mod getdate;
pub mod getenv;
pub mod gmdate;
pub mod gmmktime;
pub mod header;
pub mod hrtime;
pub mod http_response_code;
pub mod json_decode;
pub mod json_encode;
pub mod json_last_error;
pub mod json_last_error_msg;
pub mod json_support;
pub mod json_validate;
pub mod localtime;
pub mod microtime;
pub mod mktime;
pub mod passthru;
pub mod php_uname;
pub mod phpversion;
pub mod preg_match;
pub mod preg_match_all;
pub mod preg_replace;
pub mod preg_split;
pub mod putenv;
pub mod serialize;
pub mod shell_exec;
pub mod sleep;
pub mod strtotime;
pub mod system;
pub mod time;
pub mod unserialize;
pub mod usleep;
