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
    aot_signature_profile, aot_support, contracts, eval_signature, eval_support, lookup, Area,
    AotSignatureOverrideReason, BackendImplementation, BackendSupport, BuiltinContract,
    BuiltinKind, BuiltinSignature, DefaultSpec, TypeSpec, UnsupportedReason,
};
use serde_json::{json, Value};

/// Prints the complete dual-backend builtin catalog as formatted JSON.
fn main() {
    let include_internal = std::env::args().any(|argument| argument == "--include-internal");
    if std::env::args().any(|argument| argument == "--streams-compliance") {
        let Some(target_name) = std::env::args()
            .find_map(|argument| argument.strip_prefix("--target=").map(str::to_string))
        else {
            eprintln!("--streams-compliance requires --target=<supported-target>");
            std::process::exit(2);
        };
        let target = match target_name.as_str() {
            "macos-aarch64" => elephc::codegen::platform::Target::new(
                elephc::codegen::platform::Platform::MacOS,
                elephc::codegen::platform::Arch::AArch64,
            ),
            "linux-aarch64" => elephc::codegen::platform::Target::new(
                elephc::codegen::platform::Platform::Linux,
                elephc::codegen::platform::Arch::AArch64,
            ),
            "linux-x86_64" => elephc::codegen::platform::Target::new(
                elephc::codegen::platform::Platform::Linux,
                elephc::codegen::platform::Arch::X86_64,
            ),
            _ => {
                eprintln!("unsupported compliance target: {target_name}");
                std::process::exit(2);
            }
        };
        let value = elephc::stream_compliance::export_json(target);
        let json = serde_json::to_string_pretty(&value)
            .expect("serialize stream compliance JSON");
        println!("{}", json);
        return;
    }
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
        "surface_kind": kind_name(contract.kind),
        "internal": contract.internal,
        "extension": contract.extension,
        "params": signature_params_json(signature),
        "variadic": signature.variadic,
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
    }
}

/// Returns the documentation spelling for a neutral PHP type.
fn type_name(ty: TypeSpec) -> &'static str {
    match ty {
        TypeSpec::Int => "int",
        TypeSpec::Float => "float",
        TypeSpec::Str => "string",
        TypeSpec::Bool => "bool",
        TypeSpec::Mixed => "mixed",
        TypeSpec::Void => "void",
        // elephc extensions to the neutral spelling. Without these the generated pages would
        // document `mixed` for a raw address — the same wrong answer the declaration itself
        // used to give, moved one step downstream into the docs.
        TypeSpec::Ptr => "pointer",
        TypeSpec::Callable => "callable",
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
    }
}

/// Returns the stable documentation spelling for a surface kind.
fn kind_name(kind: BuiltinKind) -> &'static str {
    match kind {
        BuiltinKind::Function => "function",
        BuiltinKind::LanguageConstruct => "language-construct",
        BuiltinKind::DedicatedSyntax => "dedicated-syntax",
        BuiltinKind::PreludeProvided => "prelude-provided",
    }
}

/// Returns the stable documentation spelling for an implementation route.
fn implementation_name(implementation: BackendImplementation) -> &'static str {
    match implementation {
        BackendImplementation::Registry => "registry",
        BackendImplementation::LanguageConstruct => "language-construct",
        BackendImplementation::DedicatedSyntax => "dedicated-syntax",
        BackendImplementation::Prelude => "prelude",
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
