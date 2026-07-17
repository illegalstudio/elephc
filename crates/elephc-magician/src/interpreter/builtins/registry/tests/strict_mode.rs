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
use crate::strict_php_mode::{scoped_enable, strict_php_mode};

/// Verifies strict mode hides extension builtins from declarative lookup while
/// genuine PHP builtins stay resolvable. The RAII guard restores the previous
/// state even if an assertion panics mid-test.
#[test]
fn strict_mode_hides_extension_builtins_from_lookup() {
    let _guard = scoped_enable();
    assert!(
        eval_declared_builtin_spec("ptr_get").is_none(),
        "strict mode must hide ptr_get from eval lookup"
    );
    assert!(
        eval_declared_builtin_spec("buffer_new").is_none(),
        "strict mode must hide buffer_new from eval lookup"
    );
    assert!(
        eval_declared_builtin_spec("class_attribute_names").is_none(),
        "strict mode must hide class_attribute_names from eval lookup"
    );
    assert!(
        eval_declared_builtin_spec("strlen").is_some(),
        "strlen must stay resolvable in strict mode"
    );
    assert!(
        !eval_php_visible_builtin_exists("ptr_read8"),
        "existence probes must honor strict mode"
    );
}

/// Verifies the flag defaults to off and the guard restores it, so non-strict
/// binaries keep the full extension surface.
#[test]
fn strict_mode_defaults_off_and_guard_restores() {
    assert!(!strict_php_mode(), "strict mode must default to off");
    assert!(
        eval_declared_builtin_spec("ptr_get").is_some(),
        "extension builtins stay visible without strict mode"
    );
    {
        let _guard = scoped_enable();
        assert!(strict_php_mode(), "scoped_enable must be observable");
    }
    assert!(!strict_php_mode(), "dropping the guard must restore the state");
}
