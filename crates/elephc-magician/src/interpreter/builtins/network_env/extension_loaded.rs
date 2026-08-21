//! Purpose:
//! Eval registry entry and implementation for `extension_loaded`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Membership is resolved against a compile-time-known extension set, matching the native
//!   codegen behavior; there is no runtime extension state in this increment.
//! - Matching is case-insensitive over the canonical extension names.

use super::*;

/// Eval's compile-time-known set of "loaded" PHP extensions for `extension_loaded()`.
///
/// Most entries mirror AOT's `CORE_LOADED_EXTENSIONS`. BCMath deliberately differs: Magician
/// always implements every `bc*` function, so eval always reports `bcmath`; AOT reports it only
/// when `elephc_bcmath` is linked through static detection or `--with-bcmath`.
///
/// The native backend also reports the other bridge
/// staticlibs it links (e.g. `PDO`, `hash`, `openssl`), but the eval interpreter runs at compile
/// time with no AOT link manifest and therefore does not expose those extensions.
/// `extension_loaded('PDO')` is thus `false` under eval even when the surrounding program is
/// compiled `--with-pdo`.
const CORE_LOADED_EXTENSIONS: &[&str] = &[
    "Core",
    "standard",
    "SPL",
    "bcmath",
    "json",
    "pcre",
    "date",
    "ctype",
    "mbstring",
    "Reflection",
    "Zend OPcache",
];

eval_builtin! {
    contract: "extension_loaded",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `extension_loaded($extension)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_extension_loaded(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [extension] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let extension = eval_expr(extension, context, scope, values)?;
    eval_extension_loaded_result(extension, values)
}

/// Reports whether an already-evaluated extension name is in the known extension set.
pub(in crate::interpreter) fn eval_extension_loaded_result(
    extension: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = values.string_bytes(extension)?;
    let name = String::from_utf8_lossy(&name);
    values.bool_value(eval_extension_is_loaded(name.as_ref()))
}

/// Returns whether `name` is in eval's known extension set, compared case-insensitively.
///
/// The single membership predicate for the eval interpreter: `extension_loaded()` and
/// `phpversion($extension)` both go through it, so the two can never disagree — the same
/// invariant `extension_is_loaded` enforces on the native side.
///
/// `"curl"` is the ONE deliberate exception to this file's own "always false" rule for
/// bridge-backed extensions (`PDO`/`hash`/`openssl` above): it answers
/// `cfg!(feature = "curl")`, which is true exactly when `libelephc_magician.a` was built
/// WITH curl's eval homes compiled in — which only happens (`src/linker/bridges.rs`) for a
/// program that ALSO already links `elephc_curl` outside eval. So this can never disagree
/// with the surrounding AOT program: a curl-free program's `eval()` reports `false`,
/// matching `extension_loaded('curl')` in the same program's compiled code; a program that
/// already requires curl gets a magician build where it is `true` in eval too. See
/// `crate::interpreter::builtins::curl`'s module doc for the full argument.
pub(in crate::interpreter) fn eval_extension_is_loaded(name: &str) -> bool {
    if cfg!(feature = "curl") && name.eq_ignore_ascii_case("curl") {
        return true;
    }
    CORE_LOADED_EXTENSIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod curl_extension_tests {
    use super::*;

    /// `extension_loaded('curl')` inside eval must track EXACTLY whether this build of
    /// `libelephc_magician.a` compiled curl's eval homes in — never a static "always
    /// false" the way `PDO`/`hash`/`openssl` intentionally stay, and never a static
    /// "always true" that would lie for the (default) curl-free build. Case-insensitive,
    /// matching every other name in `CORE_LOADED_EXTENSIONS`.
    #[test]
    fn curl_reports_exactly_the_compiled_in_feature_state() {
        assert_eq!(eval_extension_is_loaded("curl"), cfg!(feature = "curl"));
        assert_eq!(eval_extension_is_loaded("CURL"), cfg!(feature = "curl"));
        assert_eq!(eval_extension_is_loaded("Curl"), cfg!(feature = "curl"));
    }

    /// The pre-existing bridge-backed extensions must still report `false` unconditionally
    /// — `curl` is a deliberate, singular exception, not a template for widening this list.
    #[test]
    fn other_bridge_backed_extensions_still_report_false() {
        assert!(!eval_extension_is_loaded("PDO"));
        assert!(!eval_extension_is_loaded("hash"));
        assert!(!eval_extension_is_loaded("openssl"));
    }
}
