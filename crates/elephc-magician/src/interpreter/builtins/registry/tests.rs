//! Purpose:
//! Test module wiring for eval builtin registry discovery and metadata checks.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Focused child modules keep large registry assertions near their area while
//!   still sharing access to private registry helpers.

mod direct_hooks;
mod exposure;
mod metadata_core;
mod metadata_filesystem;
mod metadata_misc;
mod metadata_regex;
mod metadata_streams;
mod metadata_time_and_env;
mod strict_mode;

use super::*;

/// Verifies regex specs disappear from runtime lookup without a registered provider.
#[test]
fn regex_builtins_follow_provider_availability() {
    let preg_match = eval_raw_declared_builtin_spec("preg_match")
        .expect("preg_match must remain present in raw registry metadata");
    assert!(!builtin_is_available(preg_match, false, false, true));
    assert!(builtin_is_available(preg_match, false, true, false));

    let strlen = eval_raw_declared_builtin_spec("strlen")
        .expect("strlen must remain present in raw registry metadata");
    assert!(builtin_is_available(strlen, false, false, false));
    for name in ["mb_ereg_match", "mb_ereg_search_init", "mb_ereg_search",
        "mb_ereg_search_pos", "mb_ereg_search_regs", "mb_ereg_search_getpos",
        "mb_ereg_search_getregs", "mb_ereg_search_setpos", "mb_split", "mb_ereg_replace", "mb_eregi_replace"] {
        let spec = eval_raw_declared_builtin_spec(name).unwrap();
        assert!(!builtin_is_available(spec, false, true, false), "{name}");
        assert!(builtin_is_available(spec, false, false, true), "{name}");
    }
}
