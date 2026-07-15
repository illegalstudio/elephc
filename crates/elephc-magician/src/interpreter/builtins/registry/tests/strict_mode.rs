//! Purpose:
//! Registry tests for strict-PHP mode: extension builtins must disappear from
//! declarative lookup (and therefore from dispatch, `function_exists`, and
//! `is_callable`) when the compiled binary was built with `--strict-php`.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - The strict flag is thread-local: strictness is a property of the whole
//!   compiled binary, but elephc programs run eval on a single thread, and the
//!   thread-local keeps parallel unit tests isolated from each other.

use super::*;
use crate::strict_php_mode::{set_strict_php_mode, strict_php_mode};

/// Verifies strict mode hides extension builtins from declarative lookup while
/// genuine PHP builtins stay resolvable.
#[test]
fn strict_mode_hides_extension_builtins_from_lookup() {
    set_strict_php_mode(true);
    let ptr_get = eval_declared_builtin_spec("ptr_get").is_some();
    let buffer_new = eval_declared_builtin_spec("buffer_new").is_some();
    let class_attrs = eval_declared_builtin_spec("class_attribute_names").is_some();
    let strlen = eval_declared_builtin_spec("strlen").is_some();
    let exists_probe = eval_php_visible_builtin_exists("ptr_read8");
    set_strict_php_mode(false);

    assert!(!ptr_get, "strict mode must hide ptr_get from eval lookup");
    assert!(!buffer_new, "strict mode must hide buffer_new from eval lookup");
    assert!(
        !class_attrs,
        "strict mode must hide class_attribute_names from eval lookup"
    );
    assert!(strlen, "strlen must stay resolvable in strict mode");
    assert!(!exists_probe, "existence probes must honor strict mode");
}

/// Verifies the flag defaults to off and round-trips, so non-strict binaries
/// keep the full extension surface.
#[test]
fn strict_mode_defaults_off_and_roundtrips() {
    assert!(!strict_php_mode(), "strict mode must default to off");
    assert!(
        eval_declared_builtin_spec("ptr_get").is_some(),
        "extension builtins stay visible without strict mode"
    );
    set_strict_php_mode(true);
    let enabled = strict_php_mode();
    set_strict_php_mode(false);
    assert!(enabled, "set_strict_php_mode(true) must be observable");
    assert!(!strict_php_mode(), "set_strict_php_mode(false) must restore");
}
