//! Purpose:
//! Structural parity gates for registry visibility, extension classification,
//! strict-PHP behavior, and compiler prelude usage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (unit test module).
//!
//! Key details:
//! - Registry signatures are authoritative; no parallel name-based golden table exists.
//! - Extension and internal visibility derive from shared contracts.

use crate::builtins::registry;
use elephc_builtin_contract::{
    aot_signature_profile, aot_support, Area, BackendImplementation, BackendSupport,
    BuiltinContract,
};

/// Returns the PHP-visible extension builtins a prelude must never call directly.
fn php_visible_extension_builtins() -> Vec<String> {
    let mut names: Vec<String> = vec!["buffer_new".to_string()];
    for name in registry::names() {
        let def = registry::lookup(name).expect("names() yields registered builtins");
        if def.spec.extension && !def.spec.internal {
            names.push(def.name.to_string());
        }
    }
    names
}

/// Every injected prelude that can declare a PHP-visible builtin, by name.
///
/// `prelude_contracts_match_their_injected_signatures` requires each
/// `BackendImplementation::Prelude` contract to be declared by exactly one of these,
/// so a second prelude quietly redeclaring a name is a failure, not a coin toss.
const PRELUDE_SOURCES: &[(&str, &str)] = &[
    ("hash_prelude", crate::hash_prelude::HASH_PRELUDE_SRC),
    ("curl_prelude", crate::curl_prelude::CURL_PRELUDE_SRC),
    ("pdo_prelude", crate::pdo_prelude::PDO_PRELUDE_SRC),
    ("tz_prelude", crate::tz_prelude::TZ_PRELUDE_SRC),
    ("var_export_prelude", crate::var_export_prelude::VAR_EXPORT_PRELUDE_SRC),
    ("image_prelude", crate::image_prelude::IMAGE_PRELUDE_SRC),
    ("web_prelude", crate::web_prelude::WEB_PRELUDE_SRC),
];

/// Verifies no injected compiler prelude calls a PHP-visible extension builtin.
///
/// `--strict-php` hides extension builtins at the catalog level with no notion of code origin,
/// so a prelude calling one (instead of its `internal: true` `__elephc_*` alias) would break
/// strict-mode compiles of programs that trigger that prelude's injection.
///
/// THIS USED TO BE HALF A GATE. Every prelude was PHP text, and the audit was a `grep` for
/// `name(` that tolerated bare mentions in comments and had to reason about the character before
/// the match to tell a call from `elephc_pdo_column_data_ptr(` or `->ptr(`. Now that every
/// prelude builds its declarations in Rust, the audit just reads the call sites off the AST —
/// and `called_function_names` panics on any node it does not model, so a prelude that grows a
/// construct this audit cannot see fails loudly instead of silently leaving the net.
#[test]
fn preludes_built_in_rust_never_call_php_visible_extension_builtins() {
    let extension_names = php_visible_extension_builtins();

    let built: &[(&str, crate::parser::ast::Program)] = &[
        ("hash_prelude", crate::hash_prelude::hash_declarations()),
        ("tz_prelude", crate::tz_prelude::tz_declarations()),
        (
            "var_export_prelude",
            crate::var_export_prelude::var_export_declarations(),
        ),
        (
            "list_id_prelude",
            crate::list_id_prelude::list_id_declarations(),
        ),
        (
            "pdo_prelude",
            crate::pdo_prelude::build::pdo_declarations(
                crate::php_version::PhpVersion::default(),
                crate::pdo_prelude::OptionalDrivers::from_build_environment(),
            ),
        ),
        ("image_prelude", crate::image_prelude::image_declarations()),
        (
            "version_prelude",
            crate::version_prelude::version_declarations(
                &["zend_version", "php_sapi_name", "ini_restore"],
                crate::web_prelude::PhpVersion::Php85,
            ),
        ),
        (
            "opcache_prelude(env)",
            crate::opcache_prelude::env_override_declarations(),
        ),
        (
            "opcache_prelude(ini)",
            crate::opcache_prelude::ini_helper_declarations(
                crate::web_prelude::PhpVersion::Php85,
                &[],
            ),
        ),
        (
            "opcache_prelude(state)",
            crate::opcache_prelude::build::state_helper_decls(),
        ),
        (
            "web_prelude",
            crate::web_prelude::build::web_declarations(
                crate::web_prelude::PhpVersion::Php85,
                &[],
            ),
        ),
        ("web_prelude(wrap)", vec![crate::web_prelude::web_wrap_stmt()]),
    ];

    let mut violations: Vec<String> = Vec::new();
    for (prelude, program) in built {
        let called = crate::synthetic_class::called_function_names(program);
        for name in &extension_names {
            if called.iter().any(|call| call.eq_ignore_ascii_case(name)) {
                violations.push(format!("{prelude} calls {name}()"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "preludes must call `__elephc_*` internal aliases, not PHP-visible extension builtins:\n{}",
        violations.join("\n"),
    );
}

/// Returns shared extension contracts visible through an AOT PHP call surface.
fn shared_php_visible_extension_contracts(
) -> impl Iterator<Item = &'static BuiltinContract> {
    elephc_builtin_contract::contracts()
        .iter()
        .filter(|contract| contract.extension && !contract.internal)
        .filter(|contract| {
            matches!(
                aot_support(contract),
                BackendSupport::Implemented(
                    BackendImplementation::Registry | BackendImplementation::DedicatedSyntax
                )
            )
        })
}

/// Verifies the AOT registry exposes exactly the shared registry-route extensions.
#[test]
fn extension_builtin_set_matches_shared_contracts() {
    let mut tagged: Vec<&str> = Vec::new();
    for name in registry::names() {
        let def = registry::lookup(name).expect("names() yields registered builtins");
        if def.spec.extension && !def.spec.internal {
            tagged.push(def.name);
        }
    }
    tagged.sort_unstable();
    let expected = shared_php_visible_extension_contracts()
        .filter(|contract| {
            matches!(
                aot_support(contract),
                BackendSupport::Implemented(BackendImplementation::Registry)
            )
        })
        .map(|contract| contract.name)
        .collect::<Vec<_>>();
    assert_eq!(tagged, expected, "AOT extension contract join drifted");
}

/// One PHP parameter exactly as an injected prelude declares it.
#[derive(Debug, PartialEq, Eq)]
struct PreludeParam {
    /// Variable name without the leading `$`.
    name: String,
    /// Declared `&$name`.
    by_ref: bool,
    /// Carries a PHP default expression.
    optional: bool,
}

/// Verifies every prelude-provided contract matches its injected PHP declaration.
///
/// A `BuiltinKind::PreludeProvided` contract deliberately has NO `builtin!` binding, so
/// the cross-backend audit in `tests/builtin_parity_tests.rs` has no AOT registry
/// metadata to compare its catalog signature against — the compiler-side signature such
/// a contract really has is the PHP function its prelude injects. This test reads that
/// declaration and compares it with the catalog (through `aot_signature_profile`, so a
/// documented subset like `hash_init`'s is honoured), closing the one structural hole in
/// prelude parity.
///
/// Covers the four `hash_*` contracts in every configuration and the thirty-four `curl_*`
/// contracts whenever the root `curl` feature publishes them.
#[test]
fn prelude_contracts_match_their_injected_signatures() {
    let mut checked: Vec<&str> = Vec::new();
    let mut curl_checked = 0usize;
    for contract in elephc_builtin_contract::contracts() {
        if !matches!(
            aot_support(contract),
            BackendSupport::Implemented(BackendImplementation::Prelude)
        ) {
            continue;
        }

        let found = PRELUDE_SOURCES
            .iter()
            .filter_map(|(prelude, source)| {
                parse_prelude_declaration(source, contract.name).map(|params| (*prelude, params))
            })
            .collect::<Vec<_>>();
        let sources = found.iter().map(|(prelude, _)| *prelude).collect::<Vec<_>>();
        assert_eq!(
            found.len(),
            1,
            "{} must be declared by exactly one prelude, found {sources:?}",
            contract.name
        );

        let (prelude, actual) = &found[0];
        let signature = aot_signature_profile(contract).signature;
        let expected = signature
            .params
            .iter()
            .map(|param| PreludeParam {
                name: param.name.to_string(),
                by_ref: param.by_ref,
                optional: param.default.is_some(),
            })
            .chain(signature.variadic.map(|name| PreludeParam {
                name: name.to_string(),
                by_ref: false,
                optional: true,
            }))
            .collect::<Vec<_>>();
        assert_eq!(
            *actual, expected,
            "{} signature drifted from {prelude}",
            contract.name
        );

        checked.push(contract.name);
        if matches!(contract.area, Area::Curl) {
            curl_checked += 1;
        }
    }

    assert!(
        ["hash_copy", "hash_final", "hash_init", "hash_update"]
            .iter()
            .all(|name| checked.contains(name)),
        "the hash prelude contracts must always be audited, saw {checked:?}"
    );
    // The root `curl` feature (see `Cargo.toml`) is what relays
    // `elephc-builtin-contract/curl`; publishing that catalog slice any other way is
    // not a supported configuration.
    assert_eq!(
        curl_checked,
        if cfg!(feature = "curl") { 34 } else { 0 },
        "curl prelude contracts audited"
    );
}

/// Returns the parameters of `function <name>(...)` declared in one prelude source.
///
/// Returns `None` when the prelude does not declare the function at all. Only
/// top-level declarations count: the leading-character check keeps
/// `__elephc_curl_easy_body(` from matching the contract name `curl_easy_body`.
fn parse_prelude_declaration(source: &str, name: &str) -> Option<Vec<PreludeParam>> {
    let needle = format!("function {name}(");
    let start = source
        .match_indices(&needle)
        .find(|(index, _)| {
            matches!(source[..*index].chars().next_back(), None | Some('\n') | Some(' '))
        })?
        .0;
    let open = start + needle.len();

    let mut depth = 1usize;
    let mut close = None;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let list = &source[open..close.expect("prelude declaration must close its parameter list")];

    let mut raw_params: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in list.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => raw_params.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        raw_params.push(current);
    }

    Some(
        raw_params
            .iter()
            .map(|raw| {
                let dollar = raw
                    .find('$')
                    .unwrap_or_else(|| panic!("prelude parameter {raw:?} of {name} has no variable"));
                PreludeParam {
                    name: raw[dollar + 1..]
                        .chars()
                        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                        .collect(),
                    by_ref: raw[..dollar].trim_end().ends_with('&'),
                    optional: raw[dollar..].contains('='),
                }
            })
            .collect(),
    )
}
