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
        // This branch's four. They are PHP source like curl's, not built AST like the rest.
        (
            "dir_prelude",
            parsed_php_prelude("dir", crate::dir_prelude::DIR_PRELUDE_SRC),
        ),
        (
            "gz_prelude",
            parsed_php_prelude("gz", crate::gz_prelude::GZ_PRELUDE_SRC),
        ),
        (
            "similar_text_prelude",
            parsed_php_prelude(
                "similar_text",
                crate::similar_text_prelude::SIMILAR_TEXT_PRELUDE_SRC,
            ),
        ),
        (
            "scanf_prelude",
            parsed_php_prelude("scanf", crate::scanf_prelude::SCANF_PRELUDE_SRC),
        ),
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

/// Parses `CURL_PRELUDE_SRC` exactly as `curl_prelude::inject_if_used_for_version` does.
fn parsed_curl_prelude() -> crate::parser::ast::Program {
    parsed_php_prelude("curl", crate::curl_prelude::CURL_PRELUDE_SRC)
}

/// The preludes whose canonical form is PHP SOURCE TEXT, not a built Rust AST.
///
/// `prelude_sources` hands these out verbatim rather than through
/// `synthetic_class::print::print_program`: printing a parsed program back is faithful, but the
/// source is what a reader of the prelude sees, and for these it is the authority.
const PHP_SOURCE_PRELUDES: &[(&str, &str)] = &[
    ("curl_prelude", crate::curl_prelude::CURL_PRELUDE_SRC),
    ("dir_prelude", crate::dir_prelude::DIR_PRELUDE_SRC),
    ("gz_prelude", crate::gz_prelude::GZ_PRELUDE_SRC),
    ("similar_text_prelude", crate::similar_text_prelude::SIMILAR_TEXT_PRELUDE_SRC),
    ("scanf_prelude", crate::scanf_prelude::SCANF_PRELUDE_SRC),
];

/// Tokenizes and parses one PHP-source prelude, naming it in the failure.
fn parsed_php_prelude(label: &str, source: &str) -> crate::parser::ast::Program {
    let tokens = crate::lexer::tokenize(source)
        .unwrap_or_else(|error| panic!("the {label} prelude must tokenize: {error:?}"));
    crate::parser::parse_internal(&tokens)
        .unwrap_or_else(|error| panic!("the {label} prelude must parse: {error:?}"))
}

/// Parameters a prelude declares LOOSER than its contract, on purpose, with the measurement.
///
/// A contract records what PHP DECLARES. A prelude declaration is what elephc's CHECKER
/// enforces, and the two are not the same instrument: php's coercive mode converts at the
/// boundary, elephc's checker refuses there. Where php's own declaration would make elephc
/// reject a program php runs, the prelude keeps the looser spelling and the divergence is
/// written down here.
///
/// SHRINK-ONLY. An entry whose prelude and contract have come to agree FAILS, so this list
/// cannot quietly outlive its reason.
const LOOSER_THAN_CONTRACT_PARAMS: &[(&str, &str, &str)] = &[
    (
        "gzencode",
        "data",
        "MEASURED on `php -n` 8.5.6: php declares `string $data` and still runs \
         `gzdecode(gzencode($s))`, because its encoders answer `string|false` and coercive mode \
         converts the `false` to `\"\"`. elephc's checker has no such coercion, so declaring \
         `string` here refuses at COMPILE TIME a program php executes.",
    ),
    (
        "zlib_encode",
        "data",
        "the encode half of the same measurement as gzencode()",
    ),
    (
        "gzdecode",
        "data",
        "the decode half: `gzdecode(gzencode($s))` is the shape php runs and a `string` \
         declaration rejects",
    ),
    (
        "zlib_decode",
        "data",
        "the decode half of the same measurement as zlib_encode()",
    ),
];

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
            let source = match PHP_SOURCE_PRELUDES.iter().find(|(known, _)| *known == name) {
                Some((_, php)) => (*php).to_string(),
                None => crate::synthetic_class::print::print_program(&program),
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
        .filter_map(|(name, source)| match name {
            "hash_prelude" => Some((name, source, true)),
            "curl_prelude" => Some((name, source, cfg!(feature = "curl"))),
            _ => None,
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
        assert_eq!(
            found.len(),
            1,
            "{} must be declared by exactly one prelude, found {declaring:?}",
            contract.name
        );

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
            let recorded = LOOSER_THAN_CONTRACT_PARAMS
                .iter()
                .find(|(fn_name, param_name, _)| {
                    *fn_name == name && *param_name == param.name
                })
                .map(|(_, _, reason)| *reason);
            if let Some(reason) = recorded {
                assert!(
                    !php_type_matches(param.ty, &actual.php_type),
                    "{at}: the prelude and the contract agree now — drop it from \
                     LOOSER_THAN_CONTRACT_PARAMS (recorded reason: {reason})"
                );
            } else {
                assert!(
                    php_type_matches(param.ty, &actual.php_type),
                    "{at}: declared type `{}` is not the contract's {:?}",
                    actual.php_type,
                    param.ty
                );
            }
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
            assert!(!actual.by_ref, "{at}: contract has no by-reference variadic");
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
/// `TypeSpec` has no object or array vocabulary, so those surfaces are spelled `Mixed` in the
/// catalog and the prelude is free to declare `CurlHandle`, `array` or `mixed` for one. The check
/// is therefore compatibility, not equality — but it is not vacuous either: a `Mixed` contract
/// must NOT be declared as a scalar in the prelude, because a scalar is precisely what the catalog
/// can express and deliberately did not. A leading `?` is stripped: nullability is carried by the
/// parameter's default, which is compared separately.
///
/// `Nullable` and `Union` ARE spellable, so neither collapses into `Mixed`'s open surface —
/// collapsing is what those variants exist to prevent. `Nullable(T)` compares as `T`, the `?`
/// having already been stripped, and a `Union` must be declared as exactly its own members: php
/// spells `chown()`'s parameter `string|int` and tells the two apart, so a prelude narrowing it to
/// one member is the divergence this test is here to catch, not a compatible spelling.
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
        TypeSpec::Nullable(inner) => return php_type_matches(*inner, declared),
        TypeSpec::Union(members) => {
            let mut declared_members: Vec<&str> =
                declared.split('|').map(str::trim).filter(|part| !part.is_empty()).collect();
            declared_members.sort_unstable();
            declared_members.dedup();
            let mut expected_members: Vec<&str> = Vec::with_capacity(members.len());
            for member in members.iter() {
                match member {
                    TypeSpec::Int => expected_members.push("int"),
                    TypeSpec::Float => expected_members.push("float"),
                    TypeSpec::Str => expected_members.push("string"),
                    TypeSpec::Bool => expected_members.push("bool"),
                    // A union member with no single spelling cannot be compared member by
                    // member, and answering `true` would make the whole union vacuous.
                    _ => return false,
                }
            }
            expected_members.sort_unstable();
            expected_members.dedup();
            return declared_members == expected_members;
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
        // A php constant is as legitimate a spelling as the literal, and often the honest one:
        // php's own manual writes `gzseek(..., int $whence = SEEK_SET)`. The name is resolved
        // through the shared table rather than special-cased, so `SEEK_END` against a contract
        // of `0` still fails and an undeclared name still fails.
        DefaultSpec::Int(value) => {
            declared.parse::<i64>() == Ok(*value)
                || elephc_builtin_contract::php_constants::int_constant(declared) == Some(*value)
        }
        DefaultSpec::Str(value) => {
            declared == format!("\"{value}\"") || declared == format!("'{value}'")
        }
        DefaultSpec::IntMax => declared == "PHP_INT_MAX",
        DefaultSpec::EmptyArray => declared == "[]" || declared.replace(' ', "") == "array()",
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
    assert_eq!(
        audited,
        if cfg!(feature = "curl") { 2 } else { 1 },
        "the hash prelude is always audited; the curl prelude whenever its catalog slice \
         is published"
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
