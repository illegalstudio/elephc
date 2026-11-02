//! Purpose:
//! Describes the methods of PHP's builtin `Closure` class for reflection.
//!
//! Called from:
//! - `class_construction` for the method list of `new ReflectionClass('Closure')`.
//! - `member_metadata` and `member_construction` for `ReflectionMethod('Closure', ...)`.
//!
//! Key details:
//! - `Closure` is neither an eval class nor a compiled one, so no declaration carries these
//!   signatures; they are php 8.5's, in php's declaration order.
//! - Symfony's `CheckTypeDeclarationsPass` (`lint:container`) reflects the routing condition
//!   service's `[Closure, fromCallable]` factory; with an empty method list it reported
//!   `method "Closure::fromCallable()" does not exist`.

use super::*;

/// One parameter of a builtin `Closure` method.
struct ClosureParam {
    name: &'static str,
    ty: fn() -> EvalParameterType,
    default: Option<fn() -> EvalExpr>,
    variadic: bool,
}

/// One builtin `Closure` method signature.
struct ClosureMethod {
    name: &'static str,
    visibility: EvalVisibility,
    is_static: bool,
    params: &'static [ClosureParam],
    return_type: Option<fn() -> EvalParameterType>,
}

fn closure_type() -> EvalParameterType {
    EvalParameterType::new(vec![EvalParameterTypeVariant::Class("Closure".to_string())], false)
}

fn nullable_closure_type() -> EvalParameterType {
    EvalParameterType::new(vec![EvalParameterTypeVariant::Class("Closure".to_string())], true)
}

fn object_type() -> EvalParameterType {
    EvalParameterType::new(vec![EvalParameterTypeVariant::Object], false)
}

fn nullable_object_type() -> EvalParameterType {
    EvalParameterType::new(vec![EvalParameterTypeVariant::Object], true)
}

fn scope_type() -> EvalParameterType {
    EvalParameterType::new(
        vec![EvalParameterTypeVariant::Object, EvalParameterTypeVariant::String],
        true,
    )
}

fn mixed_type() -> EvalParameterType {
    EvalParameterType::new(vec![EvalParameterTypeVariant::Mixed], false)
}

fn callable_type() -> EvalParameterType {
    EvalParameterType::new(vec![EvalParameterTypeVariant::Callable], false)
}

fn static_scope_default() -> EvalExpr {
    EvalExpr::Const(EvalConst::String("static".to_string()))
}

const NEW_SCOPE: ClosureParam = ClosureParam {
    name: "newScope",
    ty: scope_type,
    default: Some(static_scope_default),
    variadic: false,
};

/// php 8.5's `Closure` methods, in declaration order.
const CLOSURE_METHODS: &[ClosureMethod] = &[
    ClosureMethod {
        name: "__construct",
        visibility: EvalVisibility::Private,
        is_static: false,
        params: &[],
        return_type: None,
    },
    ClosureMethod {
        name: "bind",
        visibility: EvalVisibility::Public,
        is_static: true,
        params: &[
            ClosureParam { name: "closure", ty: closure_type, default: None, variadic: false },
            ClosureParam { name: "newThis", ty: nullable_object_type, default: None, variadic: false },
            NEW_SCOPE,
        ],
        return_type: Some(nullable_closure_type),
    },
    ClosureMethod {
        name: "bindTo",
        visibility: EvalVisibility::Public,
        is_static: false,
        params: &[
            ClosureParam { name: "newThis", ty: nullable_object_type, default: None, variadic: false },
            NEW_SCOPE,
        ],
        return_type: Some(nullable_closure_type),
    },
    ClosureMethod {
        name: "call",
        visibility: EvalVisibility::Public,
        is_static: false,
        params: &[
            ClosureParam { name: "newThis", ty: object_type, default: None, variadic: false },
            ClosureParam { name: "args", ty: mixed_type, default: None, variadic: true },
        ],
        return_type: Some(mixed_type),
    },
    ClosureMethod {
        name: "fromCallable",
        visibility: EvalVisibility::Public,
        is_static: true,
        params: &[ClosureParam { name: "callback", ty: callable_type, default: None, variadic: false }],
        return_type: Some(closure_type),
    },
    ClosureMethod {
        name: "getCurrent",
        visibility: EvalVisibility::Public,
        is_static: true,
        params: &[],
        return_type: Some(closure_type),
    },
    ClosureMethod {
        name: "__invoke",
        visibility: EvalVisibility::Public,
        is_static: false,
        params: &[],
        return_type: None,
    },
];

/// Returns whether `class_name` names the builtin `Closure` rather than an eval class.
pub(super) fn eval_reflection_is_builtin_closure(class_name: &str, context: &ElephcEvalContext) -> bool {
    class_name.trim_start_matches('\\').eq_ignore_ascii_case("Closure")
        && !eval_reflection_class_like_exists(class_name, context)
}

/// Returns `Closure`'s method names, in php's order.
pub(super) fn eval_reflection_builtin_closure_method_names() -> Vec<String> {
    CLOSURE_METHODS.iter().map(|method| method.name.to_string()).collect()
}

/// Returns the declared spelling and reflection metadata of one `Closure` method.
pub(super) fn eval_reflection_builtin_closure_method_metadata(
    method_name: &str,
) -> Option<(String, EvalReflectionMemberMetadata)> {
    let method = CLOSURE_METHODS
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(method_name))?;
    let names: Vec<String> = method.params.iter().map(|param| param.name.to_string()).collect();
    let has_types = vec![true; method.params.len()];
    let types: Vec<Option<EvalParameterType>> =
        method.params.iter().map(|param| Some((param.ty)())).collect();
    let attributes = vec![Vec::new(); method.params.len()];
    let defaults: Vec<Option<EvalExpr>> =
        method.params.iter().map(|param| param.default.map(|default| default())).collect();
    let by_ref = vec![false; method.params.len()];
    let variadic: Vec<bool> = method.params.iter().map(|param| param.variadic).collect();
    let required_parameter_count = eval_reflection_required_parameter_count(&defaults, &variadic);
    let flags = eval_reflection_member_flags(method.visibility, method.is_static, false, false, false);
    let declaring_function = EvalReflectionDeclaringFunctionMetadata {
        name: method.name.to_string(),
        declaring_class_name: Some("Closure".to_string()),
        magic_scope: Some(eval_reflection_method_parameter_magic_scope(
            "Closure",
            method.name,
            &format!("Closure::{}", method.name),
            None,
        )),
        attributes: Vec::new(),
        flags,
        required_parameter_count,
    };
    let parameters = eval_reflection_parameters_from_names_and_type_flags(
        Some("Closure"),
        Some(&declaring_function),
        &names,
        &has_types,
        &types,
        &attributes,
        &defaults,
        &by_ref,
        &variadic,
        &[],
    );
    let metadata = EvalReflectionMemberMetadata {
        declaring_class_name: Some("Closure".to_string()),
        source_file: None,
        source_location: None,
        attributes: Vec::new(),
        visibility: method.visibility,
        is_static: method.is_static,
        is_final: false,
        is_abstract: false,
        is_readonly: false,
        is_promoted: false,
        is_dynamic: false,
        modifiers: eval_reflection_method_modifiers(method.visibility, method.is_static, false, false),
        type_metadata: None,
        settable_type_metadata: None,
        return_type_metadata: method
            .return_type
            .and_then(|return_type| eval_reflection_parameter_type_metadata(&return_type())),
        default_value: None,
        default_value_trait_origin: None,
        required_parameter_count,
        parameters,
    };
    Some((method.name.to_string(), metadata))
}
