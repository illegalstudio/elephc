//! Purpose:
//! Test module wiring for eval builtin registry discovery and metadata checks.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Focused child modules keep large registry assertions near their area while
//!   still sharing access to private registry helpers.

mod arity_contract;
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

/// PHP's `HAVE_LCHOWN` guard removes both names from every Windows eval
/// registry surface without weakening shared-contract coverage elsewhere.
#[test]
fn link_ownership_builtins_follow_build_target_availability() {
    for name in ["lchown", "lchgrp"] {
        let contract = elephc_builtin_contract::lookup(name).unwrap();
        assert_eq!(
            builtin_contract_is_available_on_build_target(contract),
            !cfg!(windows),
            "{name}",
        );
        assert_eq!(
            eval_raw_declared_builtin_spec(name).is_some(),
            !cfg!(windows),
            "{name}",
        );
        assert_eq!(eval_declared_builtin_exists(name), !cfg!(windows), "{name}");
        assert_eq!(
            eval_declared_builtin_function_names().contains(&name),
            !cfg!(windows),
            "{name}",
        );
    }
    for name in ["chown", "chgrp"] {
        assert!(builtin_contract_is_available_on_build_target(
            elephc_builtin_contract::lookup(name).unwrap()
        ));
    }
}

/// Windows SAPI bindings remain inventoried for parity validation but are
/// visible only when the shared contract admits the build platform.
#[test]
fn windows_sapi_builtins_follow_shared_contract_availability() {
    for name in [
        "sapi_windows_vt100_support",
        "sapi_windows_cp_set",
        "sapi_windows_cp_get",
        "sapi_windows_cp_conv",
        "sapi_windows_cp_is_utf8",
        "sapi_windows_set_ctrl_handler",
        "sapi_windows_generate_ctrl_event",
    ] {
        let contract = elephc_builtin_contract::lookup(name).unwrap();
        assert_eq!(
            builtin_contract_is_available_on_build_target(contract),
            cfg!(windows),
            "{name}",
        );
        assert_eq!(eval_declared_builtin_exists(name), cfg!(windows), "{name}");
        assert!(
            eval_unfiltered_declared_builtin_spec(name).is_some(),
            "{name} must remain in the cross-target implementation inventory",
        );
    }
}

/// Platform filtering changes runtime exposure, never the implementation
/// inventory consumed by cross-target documentation and parity audits.
#[test]
fn unavailable_builtins_keep_unfiltered_implementation_metadata() {
    for name in ["lchown", "lchgrp"] {
        assert!(eval_unfiltered_declared_builtin_spec(name).is_some(), "{name}");
        assert_eq!(eval_raw_declared_builtin_spec(name).is_some(), !cfg!(windows), "{name}");
    }
}
