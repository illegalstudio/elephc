//! Purpose:
//! Unit tests for invoker-local default normalization and target assembly emission.
//!
//! Called from:
//! - `crate::codegen::runtime_callable_invoker::defaults` under `cfg(test)`.
//!
//! Key details:
//! - The tests pin source-to-physical constructor argument shapes before runtime dispatch.
//! - Target-sensitive assertions cover every supported assembly target without executing code.

use super::*;
use crate::codegen::platform::Target;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;

/// Every target this project supports. A default materializer that regressed on one of them
/// would ship a working compiler for the others, so each assertion runs over the whole set.
const SUPPORTED_TARGETS: [&str; 5] = [
    "macos-aarch64",
    "ios-arm64",
    "ios-sim-arm64",
    "linux-aarch64",
    "linux-x86_64",
];

/// Builds one physical signature for normalization tests.
fn signature(
    params: Vec<(String, PhpType)>,
    defaults: Vec<Option<Expr>>,
    declared_params: Vec<bool>,
    variadic: Option<&str>,
) -> FunctionSig {
    let len = params.len();
    FunctionSig {
        params,
        param_type_exprs: vec![None; len],
        param_attributes: vec![Vec::new(); len],
        defaults,
        return_type: PhpType::Void,
        declared_return: true,
        by_ref_return: false,
        ref_params: vec![false; len],
        declared_params,
        variadic: variadic.map(str::to_string),
        deprecation: None,
    }
}

/// Emits one resolved default into a fresh invoker body and returns the assembly.
fn emit_default_asm(target_name: &str, default: &InvokerDefaultValue) -> String {
    let target = Target::parse(target_name).unwrap();
    let mut emitter = Emitter::new(target);
    let mut data = DataSection::new();
    let owners = super::super::InvokerArgumentOwners::new(
        super::super::INVOKER_BOUNDARY_FRAME_SIZE,
        1,
    );
    let mut ctx = InvokerEmitContext::new("object_default", owners, false, Vec::new());
    emit_const_default_to_result(default, None, &mut emitter, &mut ctx, &mut data);
    emitter.output()
}

/// Returns one positional shared string argument.
fn positional_string(value: &str) -> ConstDefaultObjectArg {
    ConstDefaultObjectArg {
        name: None,
        default: ConstDefaultValue::String(value.to_string()),
    }
}

/// Nine source variadic arguments become one physical array slot on every supported target.
#[test]
fn a_large_source_variadic_object_default_uses_one_physical_array_slot() {
    let module = Module::new(Target::parse("linux-x86_64").unwrap());
    let sig = signature(
        vec![(
            "parts".to_string(),
            PhpType::Array(Box::new(PhpType::Str)),
        )],
        vec![None],
        vec![true],
        Some("parts"),
    );
    let source_args = (0..9)
        .map(|index| positional_string(&format!("part{index}")))
        .collect();
    let physical = normalize_object_args_for_signature(&module, "LargeCtor", &sig, source_args)
        .expect("a positional source variadic has one representable physical collector");
    assert_eq!(physical.len(), 1);
    assert_eq!(
        physical[0].target_ty,
        PhpType::Array(Box::new(PhpType::Str))
    );
    let InvokerDefaultValue::Array(elements) = &physical[0].default else {
        panic!("the physical source-variadic slot must be an array");
    };
    assert_eq!(elements.len(), 9);
    let default = InvokerDefaultValue::Object {
        class_name: "LargeCtor".to_string(),
        args: physical,
    };
    for name in SUPPORTED_TARGETS {
        let asm = emit_default_asm(name, &default);
        assert_eq!(
            asm.matches("__rt_array_push_refcounted").count(),
            1,
            "{name}: the outer bridge container must receive one physical slot",
        );
        assert_eq!(
            asm.matches("__rt_array_push_str").count(),
            9,
            "{name}: all nine source strings must populate the physical variadic array",
        );
    }
}

/// Named regular arguments, omitted defaults, and hidden collector count stay aligned.
#[test]
fn named_regular_and_default_fill_precede_the_hidden_collector() {
    let module = Module::new(Target::parse("linux-x86_64").unwrap());
    let sig = signature(
        vec![
            ("left".to_string(), PhpType::Str),
            ("right".to_string(), PhpType::Str),
            (
                crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            ),
        ],
        vec![
            None,
            Some(Expr::new(
                ExprKind::StringLiteral("R".to_string()),
                Span::dummy(),
            )),
            None,
        ],
        vec![true, true, false],
        Some(crate::func_args::HIDDEN_ARGS_PARAM),
    );
    let physical = normalize_object_args_for_signature(
        &module,
        "CollectorCtor",
        &sig,
        vec![ConstDefaultObjectArg {
            name: Some("left".to_string()),
            default: ConstDefaultValue::String("L".to_string()),
        }],
    )
    .expect("named binding and an omitted optional have a physical representation");
    assert_eq!(physical.len(), 3);
    assert!(matches!(
        &physical[0].default,
        InvokerDefaultValue::String(value) if value == "L"
    ));
    assert!(matches!(
        &physical[1].default,
        InvokerDefaultValue::String(value) if value == "R"
    ));
    let InvokerDefaultValue::Array(collector) = &physical[2].default else {
        panic!("the generated collector must occupy its physical array slot");
    };
    assert!(matches!(
        collector.as_slice(),
        [InvokerDefaultArrayElement {
            key: None,
            default: InvokerDefaultValue::Scalar {
                kind: CONST_DEFAULT_INT,
                payload: 1,
            },
        }]
    ));
}

/// A source variadic keeps its hidden argc and physical collector in signature order.
#[test]
fn source_variadic_tail_follows_hidden_argc() {
    let module = Module::new(Target::parse("linux-x86_64").unwrap());
    let sig = signature(
        vec![
            ("prefix".to_string(), PhpType::Str),
            (crate::func_args::HIDDEN_ARGC_PARAM.to_string(), PhpType::Int),
            (
                "rest".to_string(),
                PhpType::Array(Box::new(PhpType::Str)),
            ),
        ],
        vec![
            Some(Expr::new(
                ExprKind::StringLiteral("P".to_string()),
                Span::dummy(),
            )),
            Some(Expr::new(ExprKind::IntLiteral(0), Span::dummy())),
            None,
        ],
        vec![true, false, true],
        Some("rest"),
    );
    let physical = normalize_object_args_for_signature(
        &module,
        "VariadicCtor",
        &sig,
        vec![
            positional_string("X"),
            positional_string("Y"),
            positional_string("Z"),
        ],
    )
    .expect("a positional source variadic has a physical representation");
    assert_eq!(physical.len(), 3);
    assert!(matches!(
        &physical[1].default,
        InvokerDefaultValue::Scalar {
            kind: CONST_DEFAULT_INT,
            payload: 3,
        }
    ));
    let InvokerDefaultValue::Array(tail) = &physical[2].default else {
        panic!("the source variadic must occupy its physical container slot");
    };
    assert!(matches!(
        tail.as_slice(),
        [
            InvokerDefaultArrayElement {
                key: None,
                default: InvokerDefaultValue::String(first),
            },
            InvokerDefaultArrayElement {
                key: None,
                default: InvokerDefaultValue::String(second),
            }
        ] if first == "Y" && second == "Z"
    ));
}

/// A required-only fixed signature collects only surplus values, with no count prefix.
#[test]
fn hidden_collector_without_optional_regular_has_no_count_prefix() {
    let module = Module::new(Target::parse("linux-x86_64").unwrap());
    let sig = signature(
        vec![
            ("head".to_string(), PhpType::Str),
            (
                crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            ),
        ],
        vec![None, None],
        vec![true, false],
        Some(crate::func_args::HIDDEN_ARGS_PARAM),
    );
    let physical = normalize_object_args_for_signature(
        &module,
        "RequiredCollectorCtor",
        &sig,
        vec![positional_string("head"), positional_string("tail")],
    )
    .expect("a required-only hidden collector has a physical representation");
    assert_eq!(physical.len(), 2);
    let InvokerDefaultValue::Array(collector) = &physical[1].default else {
        panic!("the hidden collector must occupy its physical array slot");
    };
    assert!(matches!(
        collector.as_slice(),
        [InvokerDefaultArrayElement {
            key: None,
            default: InvokerDefaultValue::String(value),
        }] if value == "tail"
    ));
}
