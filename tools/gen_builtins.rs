//! Purpose:
//! Standalone documentation exporter for the shared PHP builtin contracts and
//! their independent AOT and Magician implementation bindings.
//!
//! Called from:
//! - `cargo run --example gen_builtins` during documentation generation and CI.
//!
//! Key details:
//! - The neutral contract supplies every PHP-visible surface, including constructs,
//!   dedicated syntax, preludes, and intentionally eval-only functions.
//! - AOT registry metadata contributes compiler semantics; backend availability and
//!   effective signatures come from the shared support and signature profiles.
//! - The example can read Magician through a dev-dependency without linking the
//!   interpreter into the compiler binary.

use elephc_builtin_contract::{
    aot_class_support, aot_constant_support, aot_signature_profile, aot_support, classes,
    constants, contracts, eval_class_support, eval_constant_support, eval_signature,
    eval_support, lookup, Area, AotSignatureOverrideReason, BackendImplementation,
    BackendSupport, BuiltinContract, BuiltinKind, BuiltinSignature, ConstValue, ConstantRoute,
    DefaultSpec, TypeSpec, UnsupportedReason,
};
use serde_json::{json, Value};

/// Prints the complete dual-backend builtin catalog as formatted JSON, or with `--symbols`
/// the shared class-like and global-constant catalogs.
fn main() {
    if std::env::args().any(|argument| argument == "--symbols") {
        print_symbol_catalogs();
        return;
    }
    let include_internal = std::env::args().any(|argument| argument == "--include-internal");
    let value = if include_internal {
        elephc::builtins::docs::export_builtins_json_all()
    } else {
        elephc::builtins::docs::export_builtins_json()
    };
    let Value::Array(mut records) = value else {
        panic!("builtins JSON export must be a top-level array");
    };

    for record in &mut records {
        let name = record["name"]
            .as_str()
            .expect("builtin record carries a string name");
        let contract = lookup(name).expect("AOT registry record must resolve its shared contract");
        let entry = record
            .as_object_mut()
            .expect("builtin record is a JSON object");
        entry.insert("surface_kind".to_string(), json!(kind_name(contract.kind)));
        entry.insert("aot".to_string(), aot_support_json(contract));
        entry.insert("eval".to_string(), eval_support_json(contract));
        entry.insert(
            "eval_only".to_string(),
            json!(matches!(aot_support(contract), BackendSupport::Unsupported(_))),
        );
    }

    append_non_registry_contracts(&mut records, include_internal);
    records.sort_by(|left, right| {
        left["name"]
            .as_str()
            .cmp(&right["name"].as_str())
    });
    let expected = contracts()
        .iter()
        .filter(|contract| include_internal || !contract.internal)
        .count();
    assert_eq!(
        records.len(),
        expected,
        "documentation export must contain every selected shared contract exactly once"
    );

    let output =
        serde_json::to_string_pretty(&Value::Array(records)).expect("serialize builtins JSON");
    println!("{output}");
}

/// Appends contracts implemented outside the ordinary AOT `builtin!` inventory.
fn append_non_registry_contracts(records: &mut Vec<Value>, include_internal: bool) {
    for contract in contracts() {
        if (!include_internal && contract.internal)
            || elephc::builtins::registry::lookup(contract.name).is_some()
        {
            continue;
        }
        records.push(contract_record_json(contract));
    }
}

/// Builds one top-level documentation record directly from a neutral contract.
fn contract_record_json(contract: &BuiltinContract) -> Value {
    let signature = contract.signature();
    json!({
        "name": contract.name,
        "area": area_name(contract.area),
        "module": contract.module.php_name(),
        "since": contract.since.map(|version| version.as_str()),
        "surface_kind": kind_name(contract.kind),
        "internal": contract.internal,
        "extension": contract.extension,
        "params": signature_params_json(signature),
        "variadic": signature.variadic,
        "variadic_by_ref": contract.variadic_by_ref,
        "returns": type_name(contract.returns),
        "by_ref_return": contract.by_ref_return,
        "min_args": contract.min_args,
        "max_args": contract.max_args,
        "arity_error": contract.arity_error,
        "semantics": Value::Null,
        "summary": contract.summary,
        "examples": contract.examples,
        "php_manual": contract.php_manual,
        "deprecated": contract.deprecation,
        "eval_only": matches!(aot_support(contract), BackendSupport::Unsupported(_)),
        "aot": aot_support_json(contract),
        "eval": eval_support_json(contract),
    })
}

/// Builds the compiler support block from shared support and signature contracts.
fn aot_support_json(contract: &BuiltinContract) -> Value {
    let profile = aot_signature_profile(contract);
    let signature = profile.signature;
    let common = json!({
        "params": signature_params_json(signature),
        "variadic": signature.variadic,
        "required_param_count": signature.required_param_count(),
        "signature_override_reason": profile.override_reason.map(aot_override_reason_name),
    });
    match aot_support(contract) {
        BackendSupport::Implemented(implementation) => merge_json(
            common,
            json!({
                "supported": true,
                "kind": implementation_name(implementation),
            }),
        ),
        BackendSupport::Unsupported(reason) => merge_json(
            common,
            json!({
                "supported": false,
                "kind": "none",
                "unsupported_reason": unsupported_reason_name(reason),
            }),
        ),
    }
}

/// Builds the eval-interpreter support block for one shared contract.
fn eval_support_json(contract: &BuiltinContract) -> Value {
    if let Some(meta) = elephc_magician::builtin_metadata::builtin_docs_metadata(contract.name) {
        let signature = eval_signature(contract);
        assert_eq!(
            meta.params.len(),
            signature.params.len(),
            "eval metadata and shared signature differ for {}",
            contract.name
        );
        let params = meta
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                json!({
                    "name": param.name,
                    "type": type_name(signature.params[index].ty),
                    "by_ref": param.by_ref,
                    "optional": param.default.is_some(),
                    "default": param.default,
                })
            })
            .collect::<Vec<_>>();
        let mut hooks = Vec::new();
        if meta.has_direct_hook {
            hooks.push("direct");
        }
        if meta.has_values_hook {
            hooks.push("values");
        }
        return json!({
            "supported": true,
            "kind": "registry",
            "area": meta.area,
            "hooks": hooks,
            "execution": meta.execution,
            "runtime_builtin_id": meta.runtime_builtin_id,
            "adapter_reason": meta.adapter_reason,
            "signature_override_reason": meta.signature_override_reason,
            "params": params,
            "variadic": meta.variadic,
            "required_param_count": meta.required_param_count,
            "home_file": meta.home_file,
        });
    }

    if elephc_magician::builtin_metadata::date_procedural_alias_names()
        .iter()
        .any(|alias| *alias == contract.name)
    {
        return json!({
            "supported": true,
            "kind": "date-alias",
            "home_file": "crates/elephc-magician/src/interpreter/builtins/time/aliases.rs",
        });
    }

    match eval_support(contract) {
        BackendSupport::Unsupported(reason) => json!({
            "supported": false,
            "kind": "none",
            "unsupported_reason": unsupported_reason_name(reason),
        }),
        BackendSupport::Implemented(_) => {
            panic!("eval-supported contract {} has no implementation metadata", contract.name)
        }
    }
}

/// Renders all fixed parameters in one backend signature.
fn signature_params_json(signature: BuiltinSignature) -> Vec<Value> {
    signature
        .params
        .iter()
        .map(|param| {
            json!({
                "name": param.name,
                "type": type_name(param.ty),
                "by_ref": param.by_ref,
                "optional": param.default.is_some(),
                "default": param.default.map(default_json).unwrap_or(Value::Null),
            })
        })
        .collect()
}

/// Renders one neutral default as a JSON value.
fn default_json(default: DefaultSpec) -> Value {
    match default {
        DefaultSpec::Null => Value::Null,
        DefaultSpec::Int(value) => json!(value),
        DefaultSpec::Bool(value) => json!(value),
        DefaultSpec::Float(value) => json!(value),
        DefaultSpec::Str(value) => json!(value),
        DefaultSpec::IntMax => json!("PHP_INT_MAX"),
        DefaultSpec::EmptyArray => json!([]),
        DefaultSpec::Constant(name) => json!({ "constant": name }),
        DefaultSpec::Expr(source) => json!({ "expr": source }),
    }
}

/// Returns the documentation spelling for a neutral PHP type.
fn type_name(ty: TypeSpec) -> String {
    match ty {
        TypeSpec::Int => "int".to_string(),
        TypeSpec::Float => "float".to_string(),
        TypeSpec::Str => "string".to_string(),
        TypeSpec::Bool => "bool".to_string(),
        TypeSpec::Mixed => "mixed".to_string(),
        TypeSpec::Void => "void".to_string(),
        // elephc extensions to the neutral spelling. Without these the generated pages would
        // document `mixed` for a raw address — the same wrong answer the declaration itself
        // used to give, moved one step downstream into the docs.
        TypeSpec::Ptr => "pointer".to_string(),
        TypeSpec::Callable => "callable".to_string(),
        TypeSpec::Array => "array".to_string(),
        TypeSpec::Nullable(inner) => format!("?{}", type_name(*inner)),
    }
}

/// Returns the lowercase documentation spelling for a contract area.
fn area_name(area: Area) -> &'static str {
    match area {
        Area::String => "string",
        Area::Array => "array",
        Area::Math => "math",
        Area::Io => "io",
        Area::System => "system",
        Area::Types => "types",
        Area::Callables => "callables",
        Area::Spl => "spl",
        Area::Pointers => "pointers",
        Area::Curl => "curl",
        Area::Date => "date",
        Area::Calendar => "calendar",
        Area::Mysqli => "mysqli",
        Area::Pdo => "pdo",
        Area::Web => "web",
        Area::Image => "image",
        Area::Opcache => "opcache",
    }
}

/// Returns the stable documentation spelling for a surface kind.
fn kind_name(kind: BuiltinKind) -> &'static str {
    match kind {
        BuiltinKind::Function => "function",
        BuiltinKind::LanguageConstruct => "language-construct",
        BuiltinKind::DedicatedSyntax => "dedicated-syntax",
        BuiltinKind::PreludeProvided => "prelude-provided",
        BuiltinKind::NameResolverRewrite => "name-resolver-rewrite",
    }
}

/// Returns the stable documentation spelling for an implementation route.
fn implementation_name(implementation: BackendImplementation) -> &'static str {
    match implementation {
        BackendImplementation::Registry => "registry",
        BackendImplementation::LanguageConstruct => "language-construct",
        BackendImplementation::DedicatedSyntax => "dedicated-syntax",
        BackendImplementation::Prelude => "prelude",
        BackendImplementation::CheckerInjected => "checker-injected",
        BackendImplementation::LanguageIntrinsic => "language-intrinsic",
        BackendImplementation::Interpreter => "interpreter",
        BackendImplementation::NameResolverRewrite => "name-resolver-rewrite",
    }
}

/// Returns the stable documentation spelling for an unsupported backend reason.
fn unsupported_reason_name(reason: UnsupportedReason) -> &'static str {
    match reason {
        UnsupportedReason::InternalCompilerSurface => "internal-compiler-surface",
        UnsupportedReason::EvalImplementationPending => "eval-implementation-pending",
        UnsupportedReason::EvalOnlyReflection => "eval-only-reflection",
    }
}

/// Returns the stable documentation spelling for an AOT signature override.
fn aot_override_reason_name(reason: AotSignatureOverrideReason) -> &'static str {
    match reason {
        AotSignatureOverrideReason::PreludeSignatureSubset => "prelude-signature-subset",
    }
}

/// Merges two JSON objects, with fields from `extension` winning on collision.
fn merge_json(mut base: Value, extension: Value) -> Value {
    let base = base.as_object_mut().expect("base JSON must be an object");
    let extension = extension.as_object().expect("extension JSON must be an object");
    base.extend(extension.clone());
    Value::Object(base.clone())
}

/// Prints `{"classes": [...], "constants": [...]}`: every shared class-like and global-constant
/// contract with its module, first PHP version, backend routes, and value.
fn print_symbol_catalogs() {
    let classes: Vec<Value> = classes()
        .iter()
        .map(|class| {
            json!({
                "name": class.name,
                "kind": class.kind.keyword(),
                "module": class.module.php_name(),
                "bundled": class.module.is_bundled(),
                "since": class.since.map(|version| version.as_str()),
                "aot": route_json(aot_class_support(class)),
                "eval": route_json(eval_class_support(class)),
                "extension": class.extension,
                "internal": class.internal,
                "php_manual": class.php_manual,
            })
        })
        .collect();
    let constants: Vec<Value> = constants()
        .iter()
        .map(|constant| {
            json!({
                "name": constant.name,
                "module": constant.module.php_name(),
                "bundled": constant.module.is_bundled(),
                "since": constant.since.map(|version| version.as_str()),
                "value": const_value_json(constant.value),
                "route": match constant.route {
                    ConstantRoute::Predefined => "predefined",
                    ConstantRoute::Prelude => "prelude",
                    ConstantRoute::Dynamic => "dynamic",
                },
                "aot": route_json(aot_constant_support(constant)),
                "eval": route_json(eval_constant_support(constant)),
                "extension": constant.extension,
                "internal": constant.internal,
            })
        })
        .collect();
    let value = json!({ "classes": classes, "constants": constants });
    println!(
        "{}",
        serde_json::to_string_pretty(&value).expect("serialize symbol catalogs JSON")
    );
}

/// Renders one backend route as `{"supported": bool, "kind" | "reason": ...}`.
fn route_json(support: BackendSupport) -> Value {
    match support {
        BackendSupport::Implemented(implementation) => json!({
            "supported": true,
            "kind": implementation_name(implementation),
        }),
        BackendSupport::Unsupported(reason) => json!({
            "supported": false,
            "reason": unsupported_reason_name(reason),
        }),
    }
}

/// Renders a catalogued constant value; target-dependent values carry only their type.
fn const_value_json(value: ConstValue) -> Value {
    match value {
        ConstValue::Int(value) => json!(value),
        ConstValue::Float(value) if value.is_finite() => json!(value),
        ConstValue::Float(value) => json!({ "float": format!("{value}") }),
        ConstValue::Str(value) => json!(value),
        ConstValue::Bool(value) => json!(value),
        ConstValue::Null => Value::Null,
        ConstValue::StreamResource(fd) => json!({ "resource": "stream", "fd": fd }),
        ConstValue::TargetDependent(ty) => json!({ "target_dependent": format!("{ty:?}").to_ascii_lowercase() }),
    }
}
