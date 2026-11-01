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

/// Eval's set of "loaded" PHP extensions, DERIVED from the shared catalog exactly as the AOT
/// backend derives its own.
///
/// An extension is reported only when BOTH shared facts hold:
///
/// 1. `PhpModule::covers_php_function_surface()` — elephc's catalog declares php-src's entire
///    function list for it. `extension_loaded()` promises that all of an extension's functions
///    are callable, and a partial surface cannot make that promise. This is why `mbstring`
///    (2 of php's 65 `mb_*`), `ctype` (4 of 11) and `posix` (2 of 41) are no longer listed.
///    `PhpModule::is_engine_surface()` admits `Core`, `standard` and `pcre` despite partial
///    coverage; its doc comment carries the argument.
/// 2. Every one of the module's functions has a Magician implementation
///    (`eval_support(...) == Implemented`), which is the interpreter's OWN declaration of its
///    surface and is already enforced against real bindings by the registry gate.
///
/// Fact 2 is what keeps this list from claiming a surface the AOT backend provides through a
/// linked bridge or an injected prelude that eval has no access to: the eval interpreter runs at
/// compile time with no AOT link manifest, so `extension_loaded('PDO')` stays `false` under eval
/// even when the surrounding program is compiled `--with-pdo`. `xml` / `xmlwriter` and `curl` are
/// the two host-dependent answers, handled in [`eval_extension_is_loaded_in`] and below.
fn core_loaded_extensions() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES
        .get_or_init(|| {
            // The Zend extension elephc emulates is present in every program, compiled or
            // interpreted, and `get_loaded_extensions(true)` has always said so. Its `opcache_*`
            // contracts are `PreludeProvided`, which fact 2 below cannot see, so it is named
            // before the catalog walk rather than derived through it.
            let mut names: Vec<&'static str> = vec!["Zend OPcache"];
            for module in elephc_builtin_contract::complete_surface_modules() {
                let name = module.display_name();
                if names.iter().any(|existing| existing.eq_ignore_ascii_case(name)) {
                    continue;
                }
                // `xml`/`xmlwriter` are loaded exactly when the HOST linked the xml bridge
                // (`eval_extension_is_loaded_in`), which `get_loaded_extensions` appends itself.
                // Their eval homes exist in every magician build, so the catalog walk would claim
                // them unconditionally -- and list them twice once the bridge is linked.
                if name.eq_ignore_ascii_case("xml") || name.eq_ignore_ascii_case("xmlwriter") {
                    continue;
                }
                let implemented = module.is_engine_surface()
                    || elephc_builtin_contract::module_php_functions(module).all(|contract| {
                        // A name-resolver rewrite (`date_create`, `cal_days_in_month`, …) reaches
                        // eval through the HOST's native-function bridge, not through an eval
                        // binding, so `eval_support` reports it unsupported and is wrong about
                        // availability here. MEASURED: inside `eval()`,
                        // `function_exists('date_create')` is true and
                        // `date_create('2020-01-02')` returns a `DateTime`. Counting those
                        // contracts would have dropped `date` and `calendar` from eval's list
                        // while the compiled half of the same program still reported them.
                        contract.kind == elephc_builtin_contract::BuiltinKind::NameResolverRewrite
                            || matches!(
                                elephc_builtin_contract::eval_support(contract),
                                elephc_builtin_contract::BackendSupport::Implemented(_)
                            )
                    });
                if implemented {
                    names.push(name);
                }
            }
            names.sort_by_key(|name| elephc_builtin_contract::php_registration_rank(name));
            names
        })
        .as_slice()
}

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
    eval_extension_loaded_result(extension, context, values)
}

/// Reports whether an already-evaluated extension name is in the known extension set.
pub(in crate::interpreter) fn eval_extension_loaded_result(
    extension: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = values.string_bytes(extension)?;
    let name = String::from_utf8_lossy(&name);
    values.bool_value(eval_extension_is_loaded_in(name.as_ref(), context))
}

/// `eval_extension_is_loaded` plus the one answer that depends on the HOST program:
/// `xml` / `xmlwriter` are loaded exactly when the compiled program registered the xml
/// prelude's functions into this eval context, i.e. when it linked `elephc_xml`. That
/// keeps `extension_loaded('xml')` identical inside and outside `eval()` in one program.
pub(in crate::interpreter) fn eval_extension_is_loaded_in(
    name: &str,
    context: &ElephcEvalContext,
) -> bool {
    if name.eq_ignore_ascii_case("xml") || name.eq_ignore_ascii_case("xmlwriter") {
        return context.native_function("xml_parser_create").is_some();
    }
    if eval_gmp_bridge_extension(name, context) {
        return true;
    }
    if name.eq_ignore_ascii_case("session") {
        return eval_session_bridge_linked(context);
    }
    eval_extension_is_loaded(name)
}

/// Returns whether the host program carries PHP's `session` extension.
///
/// The session surface is a compiled prelude (`web_prelude::build::session_declarations`),
/// present under `--web` and in the CLI profile, and the compiled half of the program answers
/// `extension_loaded('session')` from its link manifest. Eval sees that prelude only as the
/// native AOT callbacks the host registered, so `session_start` being one of them is the same
/// fact. Without this, code the runtime loads (a class file left to the autoloader) was told
/// the extension is missing while `session_start()` was callable from that very code.
pub(in crate::interpreter) fn eval_session_bridge_linked(context: &ElephcEvalContext) -> bool {
    context.native_function("session_start").is_some()
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
    core_loaded_extensions()
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

/// The regular (non-Zend) extension-name list `get_loaded_extensions(false)` reports in eval.
///
/// Spelled once here so `in_array($e, get_loaded_extensions())` and `extension_loaded($e)` cannot
/// disagree about any name — they used to: `pcntl` and `posix` were in this file's membership list
/// and absent from `get_loaded_extensions.rs`'s, so eval answered `true` for `extension_loaded`
/// and `false` for the array on the same two names.
pub(in crate::interpreter) fn eval_loaded_extension_names() -> &'static [&'static str] {
    core_loaded_extensions()
}

#[cfg(test)]
mod curl_extension_tests {
    use super::*;

    /// `extension_loaded('curl')` inside eval must track EXACTLY whether this build of
    /// `libelephc_magician.a` compiled curl's eval homes in — never a static "always
    /// false" the way `PDO`/`hash`/`openssl` intentionally stay, and never a static
    /// "always true" that would lie for the (default) curl-free build. Case-insensitive,
    /// matching every other name in `core_loaded_extensions()`.
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

    /// No extension whose php-src function surface elephc declares only PARTLY may be reported.
    ///
    /// This is the teeth on the derivation: it fails the moment someone puts a name back into the
    /// membership path by hand. `mbstring` and `ctype` are named explicitly because they are the
    /// two the old hand-written list claimed while declaring 2/65 and 4/11 of their functions.
    #[test]
    fn no_partially_covered_extension_is_reported() {
        for name in core_loaded_extensions() {
            let module = elephc_builtin_contract::PhpModule::parse(name)
                .unwrap_or_else(|| panic!("reported extension {name} is not a known PHP module"));
            assert!(
                module.covers_php_function_surface() || module.is_engine_surface(),
                "eval reports {name} loaded but elephc declares only part of its php function \
                 surface, and it is not one of the three engine surfaces php-src always builds"
            );
        }
        assert!(!eval_extension_is_loaded("mbstring"));
        assert!(!eval_extension_is_loaded("ctype"));
        assert!(eval_extension_is_loaded("json"), "json is 5/5 and must stay loaded");
    }

    /// `extension_loaded()` and `get_loaded_extensions()` answer from one list.
    #[test]
    fn membership_and_listing_share_one_set() {
        for name in eval_loaded_extension_names() {
            assert!(
                eval_extension_is_loaded(name),
                "{name} is listed by get_loaded_extensions() but extension_loaded() denies it"
            );
        }
    }
}
