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
    BuiltinContract, DefaultSpec, TypeSpec,
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

/// Every injected prelude's declarations, as the AST the pipeline really injects.
///
/// Most are built in Rust. `curl_prelude` and the mysqli fragments still tokenize and
/// parse PHP text at injection time, so this function parses them exactly as the pipeline
/// does and keeps both surfaces inside the same structural audits.
fn injected_prelude_programs() -> Vec<(&'static str, crate::parser::ast::Program)> {
    let mut built = vec![
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
        ("curl_prelude", parsed_curl_prelude()),
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
        ("opcache_prelude(api)", opcache_api_program()),
        (
            "opcache_prelude(cli-ini)",
            vec![
                crate::opcache_prelude::build::cli_ini_get_decl(),
                crate::opcache_prelude::build::cli_ini_set_decl(),
            ],
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
    // The mysqli prelude is a second bridge surface whose PHP fragments are
    // parsed (not built as Rust AST like the others), so parse each fragment and
    // scan it too — the gate must cover mysqli's `__elephc_*` internal-alias
    // discipline. The shared `elephc_pdo` extern block declares only symbols and
    // calls nothing, and the PDO build above already carries it, so it needs no
    // separate entry.
    for &(name, src) in crate::mysqli_prelude::fragment_sources() {
        // Fragments carry no `<?php` header (they are concatenated after one in
        // `source_for_version`), so add it before tokenizing this one on its own.
        let source = format!("<?php\n{src}");
        let tokens = crate::lexer::tokenize(&source).expect("mysqli fragment must tokenize");
        let program =
            crate::parser::parse_internal(&tokens).expect("mysqli fragment must parse");
        built.push((name, program));
    }
    built
}

/// The eight PHP-visible `opcache_*` declarations, obtained by injecting the OPcache prelude
/// into a program that references every one of them (the injector declares on demand, so
/// no single builder returns the whole surface).
fn opcache_api_program() -> crate::parser::ast::Program {
    let source = "<?php opcache_get_configuration(); opcache_reset(); opcache_get_status(); \
                  opcache_is_script_cached('a.php'); opcache_invalidate('a.php'); \
                  opcache_compile_file('a.php'); opcache_is_script_cached_in_file_cache('a.php'); \
                  opcache_jit_blacklist(null);";
    let tokens = crate::lexer::tokenize(source).expect("opcache probe must tokenize");
    let program = crate::parser::parse(&tokens).expect("opcache probe must parse");
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let (program, _sites) = crate::opcache_prelude::inject_if_used(
        program,
        crate::php_version::PhpVersion::Php85,
        false,
        None,
        &[],
        &[],
        None,
        false,
        &mut inventory,
    );
    // Keep the injected declarations only: the probe's own call statements are not prelude
    // code and must not enter the call-site audits.
    program
        .into_iter()
        .filter(|stmt| {
            matches!(
                stmt.kind,
                crate::parser::ast::StmtKind::FunctionDecl { .. }
                    | crate::parser::ast::StmtKind::Synthetic(_)
            )
        })
        .collect()
}

/// Parses `CURL_PRELUDE_SRC` exactly as `curl_prelude::inject_if_used_for_version` does.
fn parsed_curl_prelude() -> crate::parser::ast::Program {
    let tokens = crate::lexer::tokenize(crate::curl_prelude::CURL_PRELUDE_SRC)
        .expect("curl prelude must tokenize");
    crate::parser::parse_internal(&tokens).expect("curl prelude must parse")
}

/// Every injected prelude rendered back to PHP source, for the two audits that read
/// DECLARATION TEXT rather than call sites.
///
/// The signature audits compare a declared parameter's type spelling and its default
/// EXPRESSION against the catalog, and `crate::synthetic_class::print` is the faithful
/// rendering of a built program — `printing_round_trips` re-parses its output and compares
/// node for node over every built prelude, so an assertion made against this text is as
/// strong as one made against hand-written source. The curl prelude contributes its real
/// source, which is the artifact in its case.
///
/// `prelude_contracts_match_their_injected_signatures` requires each
/// `BackendImplementation::Prelude` contract to be declared by exactly one of these, so a
/// second prelude quietly redeclaring a name is a failure, not a coin toss.
fn prelude_sources() -> Vec<(&'static str, String)> {
    injected_prelude_programs()
        .into_iter()
        .map(|(name, program)| {
            let source = if name == "curl_prelude" {
                crate::curl_prelude::CURL_PRELUDE_SRC.to_string()
            } else {
                crate::synthetic_class::print::print_program(&program)
            };
            (name, source)
        })
        .collect()
}

/// The preludes whose PHP-visible surface the shared builtin catalog claims, each with
/// whether this build's catalog actually publishes that surface.
///
/// The curl slice is feature-gated (`catalog_curl.rs`), so with the root `curl` feature
/// off the catalog does not claim the curl prelude's PHP surface AT ALL and
/// "declared implies contracted" is simply not an invariant of that configuration —
/// asserting it there would fail on all thirty-four names for no defect. See
/// `catalog_hosted_preludes_declare_no_uncontracted_php_function` for why the other
/// preludes in `prelude_sources` are not in this list in any configuration.
fn catalog_hosted_preludes() -> Vec<(&'static str, String, bool)> {
    prelude_sources()
        .into_iter()
        .map(|(name, source)| {
            let published = name != "curl_prelude" || cfg!(feature = "curl");
            (name, source, published)
        })
        .collect()
}

/// PHP-visible functions a catalog-hosted prelude declares WITHOUT a shared contract,
/// with the reason each exclusion is correct rather than an oversight.
const UNCONTRACTED_PRELUDE_DECLARATIONS: &[(&str, &str)] = &[(
    "curl_file_create",
    "a plain alias of the CURLFile constructor. It has no `builtin!` binding (the prelude \
     is the whole implementation) and no `eval_builtin!` home either — inside eval() it \
     resolves through the native-class fallback, see elephc-magician's \
     interpreter::builtins::curl module doc. A contract would need an eval binding that \
     does not exist, or an EvalImplementationPending label that would be false because \
     eval() can already call it. Consequence, accepted: the PHP comparison page counts \
     the curl module 32/33 rather than 33/33.",
)];

/// Verifies no injected compiler prelude calls a PHP-visible extension builtin.
///
/// `--strict-php` hides extension builtins at the catalog level with no notion of code origin,
/// so a prelude calling one (instead of its `internal: true` `__elephc_*` alias) would break
/// strict-mode compiles of programs that trigger that prelude's injection.
///
/// THIS USED TO BE HALF A GATE. Every prelude was PHP text, and the audit was a `grep` for
/// `name(` that tolerated bare mentions in comments and had to reason about the character before
/// the match to tell a call from `elephc_pdo_column_data_ptr(` or `->ptr(`. Now the audit reads
/// the call sites off the AST — and `called_function_names` panics on any node it does not
/// model, so a prelude that grows a construct this audit cannot see fails loudly instead of
/// silently leaving the net.
///
/// `curl_prelude` and the mysqli fragments are parsed rather than built (see
/// `injected_prelude_programs`); they are audited through the same AST path, because the gate
/// is about what a prelude CALLS, not about how its declarations were produced.
#[test]
fn injected_preludes_never_call_php_visible_extension_builtins() {
    let extension_names = php_visible_extension_builtins();

    let mut violations: Vec<String> = Vec::new();
    for (prelude, program) in &injected_prelude_programs() {
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
    /// Declared PHP type, `?` included; empty when the parameter is untyped.
    php_type: String,
    /// Declared `&$name`.
    by_ref: bool,
    /// Declared `...$name`.
    variadic: bool,
    /// PHP source text of the default expression, when one is written.
    default: Option<String>,
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
/// WHY DEFAULT VALUES AND DECLARED TYPES ARE COMPARED, not just names and arity: these are
/// the fields where prelude drift changes behaviour silently and NOTHING else would catch
/// it. Compiled code executes the PHP default the prelude writes; `eval()` executes the
/// catalog's. A prelude `float $timeout = 5.0` against a catalog `Float(1.0)` leaves both
/// backends' registries, both coverage audits and the generated docs green while
/// `curl_multi_select($mh)` waits five seconds in one backend and one in the other. The
/// catalog side has the committed `builtin_registry.json` as a review backstop; the
/// prelude side has none but this test.
///
/// Covers the four `hash_*` contracts in every configuration and the thirty-four `curl_*`
/// contracts whenever the root `curl` feature publishes them.
#[test]
fn prelude_contracts_match_their_injected_signatures() {
    let sources = prelude_sources();
    let mut checked: Vec<&str> = Vec::new();
    let mut curl_checked = 0usize;
    for contract in elephc_builtin_contract::contracts() {
        if !matches!(
            aot_support(contract),
            BackendSupport::Implemented(BackendImplementation::Prelude)
        ) {
            continue;
        }

        let found = sources
            .iter()
            .filter_map(|(prelude, source)| {
                parse_prelude_declaration(source, contract.name).map(|params| (*prelude, params))
            })
            .collect::<Vec<_>>();
        let declaring = found.iter().map(|(prelude, _)| *prelude).collect::<Vec<_>>();
        assert!(
            !found.is_empty(),
            "{} has a prelude route but no prelude declares it",
            contract.name
        );
        // Mode-exclusive preludes may each declare the same function (`ini_get` in the
        // `--web` prelude and in the OPcache CLI shim); every declaration must then agree.
        for (other, declared) in &found[1..] {
            assert_eq!(
                declared, &found[0].1,
                "{} is declared differently by {} and {other}",
                contract.name, found[0].0
            );
        }
        let _ = &declaring;

        let (prelude, declared) = &found[0];
        let name = contract.name;
        let signature = aot_signature_profile(contract).signature;
        let expected_names = signature
            .params
            .iter()
            .map(|param| param.name)
            .chain(signature.variadic)
            .collect::<Vec<_>>();
        let declared_names = declared
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            declared_names, expected_names,
            "{name} parameter names drifted from {prelude}"
        );

        for (param, actual) in signature.params.iter().zip(declared) {
            let at = format!("{name}(${}) in {prelude}", param.name);
            assert_eq!(actual.by_ref, param.by_ref, "{at}: by-reference marker");
            assert!(!actual.variadic, "{at}: fixed parameter declared variadic");
            assert!(
                php_type_matches(param.ty, &actual.php_type),
                "{at}: declared type `{}` is not the contract's {:?}",
                actual.php_type,
                param.ty
            );
            match (param.default, actual.default.as_deref()) {
                (None, None) => {}
                (Some(expected), Some(text)) => assert!(
                    default_matches(&expected, text),
                    "{at}: default `{text}` is not the contract's `{}`",
                    default_text(&expected)
                ),
                (Some(expected), None) => panic!(
                    "{at}: contract has default `{}`, the prelude declares none",
                    default_text(&expected)
                ),
                (None, Some(text)) => {
                    panic!("{at}: prelude declares default `{text}`, the contract has none")
                }
            }
        }

        // A PHP variadic is spelled `...$rest` and never carries `= default`, so it is
        // checked here rather than folded into the fixed-parameter loop above.
        if let Some(variadic) = signature.variadic {
            let actual = declared.last().expect("names matched, so a tail exists");
            let at = format!("{name}(...${variadic}) in {prelude}");
            assert!(actual.variadic, "{at}: must be declared `...${variadic}`");
            assert_eq!(
                actual.by_ref, contract.variadic_by_ref,
                "{at}: by-reference marker on the variadic"
            );
            assert_eq!(actual.default, None, "{at}: a variadic takes no default");
        }

        checked.push(name);
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
/// Returns `None` when the prelude does not declare the function at all. A match
/// counts only when nothing but whitespace precedes it on its line, which rejects
/// both a longer identifier (`__elephc_curl_easy_body(` for the contract name
/// `curl_easy_body`) and a prose mention inside a `//` or `*` comment line.
fn parse_prelude_declaration(source: &str, name: &str) -> Option<Vec<PreludeParam>> {
    let needle = format!("function {name}(");
    let start = source
        .match_indices(&needle)
        .find(|(index, _)| {
            let line_start = source[..*index].rfind('\n').map_or(0, |offset| offset + 1);
            source[line_start..*index].chars().all(char::is_whitespace)
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

    Some(raw_params.iter().map(|raw| parse_prelude_param(raw, name)).collect())
}

/// Parses one `type &...$name = default` parameter out of a prelude declaration.
fn parse_prelude_param(raw: &str, function: &str) -> PreludeParam {
    let dollar = raw
        .find('$')
        .unwrap_or_else(|| panic!("prelude parameter {raw:?} of {function} has no variable"));

    // Strip the passing markers off the end of the type, in either PHP order
    // (`int &...$rest` and `int ...$rest` both occur; `int ...&$rest` does not).
    let mut head = raw[..dollar].trim();
    let mut by_ref = false;
    let mut variadic = false;
    loop {
        if let Some(rest) = head.strip_suffix("...") {
            variadic = true;
            head = rest.trim_end();
        } else if let Some(rest) = head.strip_suffix('&') {
            by_ref = true;
            head = rest.trim_end();
        } else {
            break;
        }
    }

    let tail = &raw[dollar + 1..];
    let identifier_len = tail
        .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .unwrap_or(tail.len());
    let after = tail[identifier_len..].trim_start();

    PreludeParam {
        name: tail[..identifier_len].to_string(),
        php_type: head.to_string(),
        by_ref,
        variadic,
        default: after
            .strip_prefix('=')
            .map(|value| value.trim().to_string()),
    }
}

/// Returns whether a prelude's declared PHP type is the contract's neutral type.
///
/// `TypeSpec` has no object, array or union vocabulary, so every non-scalar surface is
/// spelled `Mixed` in the catalog and the prelude is free to declare `CurlHandle`,
/// `array`, `mixed` or a union for it. The check is therefore compatibility, not
/// equality — but it is not vacuous either: a `Mixed` contract must NOT be declared as a
/// scalar in the prelude, because a scalar is precisely what the catalog can express and
/// deliberately did not. A leading `?` is stripped: nullability is carried by the
/// parameter's default, which is compared separately.
fn php_type_matches(expected: TypeSpec, declared: &str) -> bool {
    let declared = declared.trim().trim_start_matches('?').trim();
    let scalar = match expected {
        TypeSpec::Int => "int",
        TypeSpec::Float => "float",
        TypeSpec::Str => "string",
        TypeSpec::Bool => "bool",
        TypeSpec::Void => "void",
        // Neither is a PHP scalar, and neither is `Mixed`'s open surface: `Ptr` is elephc's
        // own `ptr` type and `Callable` is the owned descriptor `callable` lowers to. Both
        // have exactly one spelling a prelude may use, so they compare like the scalars
        // rather than like `Mixed`.
        TypeSpec::Ptr => "ptr",
        TypeSpec::Callable => "callable",
        TypeSpec::Mixed => {
            return !matches!(
                declared,
                "int" | "float" | "string" | "bool" | "ptr" | "callable"
            );
        }
    };
    declared == scalar
}

/// Returns whether a prelude's default expression is the contract's default value.
fn default_matches(expected: &DefaultSpec, declared: &str) -> bool {
    let declared = declared.trim();
    match expected {
        DefaultSpec::Null => declared.eq_ignore_ascii_case("null"),
        DefaultSpec::Bool(value) => {
            declared.eq_ignore_ascii_case(if *value { "true" } else { "false" })
        }
        // Parsed rather than string-compared so `1.0`, `1.00` and `1e0` all agree with
        // `Float(1.0)` while `5.0` does not.
        DefaultSpec::Float(value) => declared.parse::<f64>() == Ok(*value),
        DefaultSpec::Int(value) => declared.parse::<i64>() == Ok(*value),
        DefaultSpec::Str(value) => {
            declared == format!("\"{value}\"") || declared == format!("'{value}'")
        }
        // A built prelude carries the folded literal; PHP text spells the constant.
        DefaultSpec::IntMax => declared == "PHP_INT_MAX" || declared.parse::<i64>() == Ok(i64::MAX),
        DefaultSpec::EmptyArray => declared == "[]" || declared.replace(' ', "") == "array()",
        DefaultSpec::Constant(name) => declared == *name,
        DefaultSpec::Expr(source) => {
            declared.split_whitespace().collect::<String>()
                == source.split_whitespace().collect::<String>()
        }
    }
}

/// Renders a contract default the way PHP source spells it, for failure messages.
fn default_text(default: &DefaultSpec) -> String {
    match default {
        DefaultSpec::Null => "null".to_string(),
        DefaultSpec::Bool(value) => value.to_string(),
        DefaultSpec::Int(value) => value.to_string(),
        DefaultSpec::Float(value) => format!("{value:?}"),
        DefaultSpec::Str(value) => format!("\"{value}\""),
        DefaultSpec::IntMax => "PHP_INT_MAX".to_string(),
        DefaultSpec::EmptyArray => "[]".to_string(),
        DefaultSpec::Constant(name) => (*name).to_string(),
        DefaultSpec::Expr(source) => (*source).to_string(),
    }
}

/// Verifies no PHP-visible declaration in a catalog-hosted prelude escapes the catalog.
///
/// `prelude_contracts_match_their_injected_signatures` audits contract -> prelude. This is
/// the inverse: a function the prelude declares but the catalog does not carry is
/// invisible to BOTH parity suites and to the generated documentation, because every one
/// of them iterates `contracts()`. Without this test that omission is silent.
///
/// SCOPE. Only the preludes whose PHP surface the shared catalog claims IN THIS BUILD are
/// audited (see `catalog_hosted_preludes`). `pdo_prelude`, `tz_prelude`,
/// `var_export_prelude`, `image_prelude` and `web_prelude` ship PHP-visible functions that
/// have no shared contracts AT ALL by design (`var_export`, `imagecreate`, `setcookie`,
/// `session_*`, `pdo_drivers`, `timezone_*` — verified: none of them appears in
/// `contracts()`), so "declared implies contracted" is simply not their invariant and
/// asserting it here would be wrong rather than strict.
#[test]
fn catalog_hosted_preludes_declare_no_uncontracted_php_function() {
    let mut audited = 0usize;
    for (prelude, source, published) in catalog_hosted_preludes() {
        if !published {
            continue;
        }
        audited += 1;
        for name in php_visible_declarations(&source) {
            if let Some(reason) = UNCONTRACTED_PRELUDE_DECLARATIONS
                .iter()
                .find(|(allowed, _)| *allowed == name)
                .map(|(_, reason)| *reason)
            {
                assert!(
                    elephc_builtin_contract::lookup(&name).is_none(),
                    "{name} now has a shared contract; drop it from \
                     UNCONTRACTED_PRELUDE_DECLARATIONS (recorded reason: {reason})"
                );
                continue;
            }
            let contract = elephc_builtin_contract::lookup(&name).unwrap_or_else(|| {
                panic!(
                    "{prelude} declares PHP-visible {name}() with no shared contract, so no \
                     parity suite and no generated page can see it — add a contract, or add \
                     it to UNCONTRACTED_PRELUDE_DECLARATIONS with the reason"
                )
            });
            assert_eq!(
                aot_support(contract),
                BackendSupport::Implemented(BackendImplementation::Prelude),
                "{name} is declared by {prelude} but its contract claims another AOT route"
            );
        }
    }
    assert!(
        audited >= 12,
        "every injected prelude is catalog-hosted and audited, saw only {audited}"
    );
}

/// Verifies the prelude parameter parser reads every PHP passing form it may meet.
///
/// The catalog has no variadic prelude contract today, so the variadic and by-reference
/// variadic arms cannot be exercised by real data — and a parser that silently mis-reads
/// `int &...$rest` as a fixed `$rest` would make the first such contract fail for a
/// confusing reason instead of a real one. These are the forms PHP can spell.
#[test]
fn prelude_parameters_parse_every_php_passing_form() {
    let cases: &[(&str, PreludeParam)] = &[
        (
            "CurlHandle $handle",
            PreludeParam {
                name: "handle".to_string(),
                php_type: "CurlHandle".to_string(),
                by_ref: false,
                variadic: false,
                default: None,
            },
        ),
        (
            " ?int $option = null ",
            PreludeParam {
                name: "option".to_string(),
                php_type: "?int".to_string(),
                by_ref: false,
                variadic: false,
                default: Some("null".to_string()),
            },
        ),
        (
            "int &$still_running",
            PreludeParam {
                name: "still_running".to_string(),
                php_type: "int".to_string(),
                by_ref: true,
                variadic: false,
                default: None,
            },
        ),
        (
            "float $timeout = 1.0",
            PreludeParam {
                name: "timeout".to_string(),
                php_type: "float".to_string(),
                by_ref: false,
                variadic: false,
                default: Some("1.0".to_string()),
            },
        ),
        (
            "mixed ...$values",
            PreludeParam {
                name: "values".to_string(),
                php_type: "mixed".to_string(),
                by_ref: false,
                variadic: true,
                default: None,
            },
        ),
        (
            "array &...$rest",
            PreludeParam {
                name: "rest".to_string(),
                php_type: "array".to_string(),
                by_ref: true,
                variadic: true,
                default: None,
            },
        ),
        (
            "$untyped = []",
            PreludeParam {
                name: "untyped".to_string(),
                php_type: String::new(),
                by_ref: false,
                variadic: false,
                default: Some("[]".to_string()),
            },
        ),
    ];
    for (raw, expected) in cases {
        assert_eq!(&parse_prelude_param(raw, "fixture"), expected, "parsing {raw:?}");
    }

    assert!(php_type_matches(TypeSpec::Mixed, "CurlHandle"));
    assert!(php_type_matches(TypeSpec::Int, "?int"));
    assert!(!php_type_matches(TypeSpec::Mixed, "int"));
    assert!(!php_type_matches(TypeSpec::Float, "int"));
    assert!(default_matches(&DefaultSpec::Float(1.0), "1.00"));
    assert!(!default_matches(&DefaultSpec::Float(1.0), "5.0"));
    assert!(default_matches(&DefaultSpec::Null, "NULL"));
    assert!(!default_matches(&DefaultSpec::Bool(false), "true"));
}

/// Returns the PHP-visible function names one prelude source declares at top level.
///
/// `__elephc_`-prefixed helpers are the preludes' own internals — they are not PHP
/// surface and carry no contract by design.
fn php_visible_declarations(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| line.strip_prefix("function "))
        .filter_map(|rest| rest.split('(').next())
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.starts_with("__elephc_"))
        .map(str::to_string)
        .collect()
}

/// Migration tool, not an audit: dumps every PHP-visible function each injected prelude
/// declares, with its declared signature, as JSON — the seed for their shared contracts.
///
/// Runs only when `ELEPHC_CONTRACT_SEED_OUT` names the output path, exactly like
/// `synthetic_class::transcribe::tests::dump_prelude_on_request`. Once the contracts exist the
/// two-way prelude audits above own the invariant; this dump is how a new prelude surface gets
/// its first draft without hand-transcribing hundreds of signatures.
#[test]
fn dump_prelude_contract_seed_on_request() {
    let Ok(out) = std::env::var("ELEPHC_CONTRACT_SEED_OUT") else {
        return;
    };
    use crate::parser::ast::{Expr, Stmt, StmtKind, TypeExpr};

    fn type_text(ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Int => "int".to_string(),
            TypeExpr::Float => "float".to_string(),
            TypeExpr::Bool => "bool".to_string(),
            TypeExpr::False => "false".to_string(),
            TypeExpr::Str => "string".to_string(),
            TypeExpr::Void => "void".to_string(),
            TypeExpr::Never => "never".to_string(),
            TypeExpr::Iterable => "iterable".to_string(),
            TypeExpr::Array(_) => "array".to_string(),
            TypeExpr::Ptr(_) => "ptr".to_string(),
            TypeExpr::Buffer(_) => "buffer".to_string(),
            TypeExpr::Named(name) => name.as_str().to_string(),
            TypeExpr::Nullable(inner) => format!("?{}", type_text(inner)),
            TypeExpr::Union(members) => members.iter().map(type_text).collect::<Vec<_>>().join("|"),
            TypeExpr::Intersection(members) => {
                members.iter().map(type_text).collect::<Vec<_>>().join("&")
            }
        }
    }
    fn default_text(expr: &Expr) -> String {
        crate::synthetic_class::print::print_expr(expr)
    }
    fn collect(stmts: &[Stmt], prelude: &str, out: &mut Vec<serde_json::Value>) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::FunctionDecl {
                    name,
                    params,
                    variadic,
                    variadic_by_ref,
                    variadic_type,
                    return_type,
                    by_ref_return,
                    ..
                } => {
                    let params: Vec<serde_json::Value> = params
                        .iter()
                        .map(|(name, ty, default, by_ref)| {
                            serde_json::json!({
                                "name": name,
                                "type": ty.as_ref().map(type_text),
                                "default": default.as_ref().map(default_text),
                                "by_ref": by_ref,
                            })
                        })
                        .collect();
                    out.push(serde_json::json!({
                        "prelude": prelude,
                        "name": name,
                        "params": params,
                        "variadic": variadic,
                        "variadic_by_ref": variadic_by_ref,
                        "variadic_type": variadic_type.as_ref().map(type_text),
                        "returns": return_type.as_ref().map(type_text),
                        "by_ref_return": by_ref_return,
                    }));
                }
                StmtKind::IncludeOnceGuard { body, .. } | StmtKind::Synthetic(body) => {
                    collect(body, prelude, out);
                }
                _ => {}
            }
        }
    }

    let mut records = Vec::new();
    for (prelude, program) in injected_prelude_programs() {
        collect(&program, prelude, &mut records);
    }
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&records).expect("serialize seed"),
    )
    .expect("write seed");
    eprintln!("wrote {} prelude function declarations to {out}", records.len());
}
