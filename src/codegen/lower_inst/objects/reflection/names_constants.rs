//! Purpose:
//! Class-like names, constant evaluation, and empty metadata construction.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Returns the `__construct` member object metadata when the reflected class-like symbol has one.
pub(super) fn reflection_constructor_member(
    method_members: &[ReflectionListedMember],
) -> Option<ReflectionListedMember> {
    method_members
        .iter()
        .find(|member| php_symbol_key(&member.name) == "__construct")
        .cloned()
}

/// Builds common ReflectionMethod/ReflectionProperty predicate flags.
pub(super) fn reflection_member_flags(
    is_static: bool,
    visibility: &Visibility,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_promoted: bool,
) -> ReflectionMemberFlags {
    ReflectionMemberFlags {
        is_static,
        is_public: visibility == &Visibility::Public,
        is_protected: visibility == &Visibility::Protected,
        is_private: visibility == &Visibility::Private,
        is_final,
        is_abstract,
        is_readonly,
        is_promoted,
        is_virtual: false,
        is_dynamic: false,
    }
}

/// Returns PHP case-insensitive method names declared by an interface and its parents.
pub(super) fn reflection_interface_method_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    if let Some(method_names) = crate::types::php_src_date_method_names(interface_name) {
        return method_names
            .iter()
            .map(|method_name| (*method_name).to_string())
            .collect();
    }
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    push_unique_method_names(info.methods.keys(), &mut names, &mut seen);
    push_unique_method_names(info.static_methods.keys(), &mut names, &mut seen);
    names
}

/// Returns PHP case-sensitive property names declared by an interface and its parents.
pub(super) fn reflection_interface_property_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for property in info.properties.keys() {
        push_unique_property_name(property, &mut names, &mut seen);
    }
    names
}

/// Returns PHP case-sensitive constant names declared by an interface and its parents.
pub(super) fn reflection_interface_constant_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for constant in info.constants.keys() {
        push_unique_constant_name(constant, &mut names, &mut seen);
    }
    names
}

/// Returns PHP case-insensitive direct method names declared by a trait.
pub(super) fn reflection_trait_method_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_method_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Returns PHP case-sensitive direct property names declared by a trait.
pub(super) fn reflection_trait_property_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_property_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Returns PHP case-sensitive direct constant names declared by a trait.
pub(super) fn reflection_trait_constant_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_constant_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Appends lower-case method names while preserving first-seen order.
pub(super) fn push_unique_method_names<'a>(
    method_names: impl Iterator<Item = &'a String>,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    for method_name in method_names {
        let key = php_symbol_key(method_name);
        if seen.insert(key.clone()) {
            names.push(key);
        }
    }
}

/// Appends one case-sensitive property name while preserving first-seen order.
pub(super) fn push_unique_property_name(
    property_name: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(property_name.to_string()) {
        names.push(property_name.to_string());
    }
}

/// Appends one case-sensitive class constant name while preserving first-seen order.
pub(super) fn push_unique_constant_name(
    constant_name: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(constant_name.to_string()) {
        names.push(constant_name.to_string());
    }
}

/// Appends one constant metadata member while preserving first-seen order.
pub(super) fn push_unique_constant_member(
    constant_name: &str,
    value: ReflectionConstantValue,
    members: &mut Vec<ReflectionConstantMember>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(constant_name.to_string()) {
        members.push(ReflectionConstantMember {
            name: constant_name.to_string(),
            value,
        });
    }
}

/// Evaluates one class/interface/trait constant expression for static Reflection metadata.
pub(super) fn reflection_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    expr: &Expr,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    if depth > 16 {
        return Err(CodegenIrError::unsupported(
            "deep recursive ReflectionClass constant metadata",
        ));
    }
    match &expr.kind {
        ExprKind::IntLiteral(value) => Ok(ReflectionConstantValue::Int(*value)),
        ExprKind::BoolLiteral(value) => Ok(ReflectionConstantValue::Bool(*value)),
        ExprKind::FloatLiteral(value) => Ok(ReflectionConstantValue::Float(*value)),
        ExprKind::StringLiteral(value) => Ok(ReflectionConstantValue::Str(value.clone())),
        ExprKind::Null => Ok(ReflectionConstantValue::Null),
        ExprKind::Negate(inner) => {
            match reflection_constant_value(ctx, current_class, current_info, inner, depth + 1)? {
                ReflectionConstantValue::Int(value) => Ok(ReflectionConstantValue::Int(-value)),
                ReflectionConstantValue::Float(value) => Ok(ReflectionConstantValue::Float(-value)),
                other => Err(unsupported_reflection_constant_value(other)),
            }
        }
        ExprKind::BinaryOp { left, op, right } => reflection_binary_constant_value(
            ctx,
            current_class,
            current_info,
            left,
            op,
            right,
            depth + 1,
        ),
        ExprKind::ClassConstant { receiver } => {
            let class_name =
                reflection_static_receiver_name(current_class, current_info, receiver)?;
            Ok(ReflectionConstantValue::Str(class_name))
        }
        ExprKind::ScopedConstantAccess { receiver, name } => reflection_scoped_constant_value(
            ctx,
            current_class,
            current_info,
            receiver,
            name,
            depth + 1,
        ),
        other => Err(CodegenIrError::unsupported(format!(
            "ReflectionClass constant metadata expression {:?}",
            other
        ))),
    }
}

/// Evaluates one supported binary operator in a static Reflection constant expression.
pub(super) fn reflection_binary_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    let left = reflection_constant_value(ctx, current_class, current_info, left, depth)?;
    let right = reflection_constant_value(ctx, current_class, current_info, right, depth)?;
    match (&left, op, &right) {
        (
            ReflectionConstantValue::Int(left),
            BinOp::Add,
            ReflectionConstantValue::Int(right),
        ) => {
            Ok(ReflectionConstantValue::Int(*left + *right))
        }
        (
            ReflectionConstantValue::Int(left),
            BinOp::Sub,
            ReflectionConstantValue::Int(right),
        ) => {
            Ok(ReflectionConstantValue::Int(*left - *right))
        }
        (
            ReflectionConstantValue::Int(left),
            BinOp::Mul,
            ReflectionConstantValue::Int(right),
        ) => {
            Ok(ReflectionConstantValue::Int(*left * *right))
        }
        (
            ReflectionConstantValue::Int(left),
            BinOp::Mod,
            ReflectionConstantValue::Int(right),
        ) if *right != 0 => {
            Ok(ReflectionConstantValue::Int(*left % *right))
        }
        (
            ReflectionConstantValue::Int(left),
            BinOp::Pow,
            ReflectionConstantValue::Int(right),
        ) if *right >= 0 =>
        {
            Ok(ReflectionConstantValue::Int((*left).pow(*right as u32)))
        }
        (
            ReflectionConstantValue::Str(left),
            BinOp::Concat,
            ReflectionConstantValue::Str(right),
        ) => Ok(ReflectionConstantValue::Str(format!("{}{}", left, right))),
        (
            left,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow,
            right,
        ) => reflection_float_binary_constant_value(left, op, right).ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "ReflectionClass constant metadata binary value {:?} {:?}",
                reflection_constant_value_kind(left),
                reflection_constant_value_kind(right)
            ))
        }),
        (left, _, right) => Err(CodegenIrError::unsupported(format!(
            "ReflectionClass constant metadata binary value {:?} {:?}",
            reflection_constant_value_kind(left),
            reflection_constant_value_kind(right)
        ))),
    }
}

/// Evaluates a numeric binary operator that must produce a float Reflection value.
pub(super) fn reflection_float_binary_constant_value(
    left: &ReflectionConstantValue,
    op: &BinOp,
    right: &ReflectionConstantValue,
) -> Option<ReflectionConstantValue> {
    let left = reflection_constant_value_as_float(left)?;
    let right = reflection_constant_value_as_float(right)?;
    let value = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div if right != 0.0 => left / right,
        BinOp::Pow => left.powf(right),
        _ => return None,
    };
    Some(ReflectionConstantValue::Float(value))
}

/// Returns the float representation of numeric Reflection constant metadata.
pub(super) fn reflection_constant_value_as_float(value: &ReflectionConstantValue) -> Option<f64> {
    match value {
        ReflectionConstantValue::Int(value) => Some(*value as f64),
        ReflectionConstantValue::Float(value) => Some(*value),
        _ => None,
    }
}

/// Resolves and evaluates one scoped class/interface/trait constant value.
pub(super) fn reflection_scoped_constant_value(
    ctx: &FunctionContext<'_>,
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    receiver: &StaticReceiver,
    constant_name: &str,
    depth: usize,
) -> Result<ReflectionConstantValue> {
    let class_name = reflection_static_receiver_name(current_class, current_info, receiver)?;
    if let Some((resolved_name, info)) = resolve_reflection_class(ctx, &class_name) {
        if let Some(value_expr) = info.constants.get(constant_name) {
            return reflection_constant_value(ctx, resolved_name, Some(info), value_expr, depth);
        }
        for interface_name in &info.interfaces {
            if let Some(value_expr) =
                reflection_interface_constant_expr(ctx, interface_name, constant_name)
            {
                return reflection_constant_value(ctx, interface_name, None, &value_expr, depth);
            }
        }
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, &class_name) {
        if let Some(value_expr) =
            reflection_interface_constant_expr(ctx, interface_name, constant_name)
        {
            return reflection_constant_value(ctx, interface_name, None, &value_expr, depth);
        }
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &class_name) {
        if let Some(value_expr) = ctx
            .module
            .declared_trait_constants
            .get(trait_name)
            .and_then(|constants| constants.get(constant_name))
        {
            return reflection_constant_value(ctx, trait_name, None, value_expr, depth);
        }
    }
    if ctx
        .module
        .enum_infos
        .get(&class_name)
        .is_some_and(|info| info.cases.iter().any(|case| case.name == constant_name))
    {
        return Ok(ReflectionConstantValue::EnumCase {
            enum_name: class_name,
            case_name: constant_name.to_string(),
        });
    }
    Err(CodegenIrError::unsupported(format!(
        "ReflectionClass constant metadata for {}::{}",
        current_class, constant_name
    )))
}

/// Returns an interface constant expression, including inherited parent interfaces.
pub(super) fn reflection_interface_constant_expr(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    constant_name: &str,
) -> Option<Expr> {
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![interface_name.to_string()];
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        if let Some(info) = ctx.module.interface_infos.get(&name) {
            if let Some(value) = info.constants.get(constant_name) {
                return Some(value.clone());
            }
            queue.extend(info.parents.iter().cloned());
        }
    }
    None
}

/// Resolves a static receiver against the current reflected declaration.
pub(super) fn reflection_static_receiver_name(
    current_class: &str,
    current_info: Option<&crate::types::ClassInfo>,
    receiver: &StaticReceiver,
) -> Result<String> {
    match receiver {
        StaticReceiver::Named(name) => Ok(name.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => Ok(current_class.to_string()),
        StaticReceiver::Parent => current_info
            .and_then(|info| info.parent.clone())
            .ok_or_else(|| {
                CodegenIrError::unsupported(format!(
                    "ReflectionClass constant metadata parent receiver in {}",
                    current_class
                ))
            }),
    }
}

/// Returns a small label for unsupported constant-value diagnostics.
pub(super) fn reflection_constant_value_kind(value: &ReflectionConstantValue) -> &'static str {
    match value {
        ReflectionConstantValue::Int(_) => "int",
        ReflectionConstantValue::Bool(_) => "bool",
        ReflectionConstantValue::Float(_) => "float",
        ReflectionConstantValue::Str(_) => "string",
        ReflectionConstantValue::Null => "null",
        ReflectionConstantValue::EnumCase { .. } => "enum-case",
    }
}

/// Reports an unsupported unary constant value while avoiding large debug output.
pub(super) fn unsupported_reflection_constant_value(value: ReflectionConstantValue) -> CodegenIrError {
    CodegenIrError::unsupported(format!(
        "ReflectionClass constant metadata unary value {}",
        reflection_constant_value_kind(&value)
    ))
}

/// Looks up class-constant metadata by PHP-style class name and case-sensitive constant name.
pub(super) fn resolve_reflection_class_constant<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a crate::types::ClassInfo)> {
    let (resolved_name, info) = resolve_reflection_class(ctx, class_name)?;
    if info.constants.contains_key(constant_name) {
        return Some((resolved_name, info));
    }
    let parent = info.parent.as_deref()?;
    resolve_reflection_class_constant(ctx, parent, constant_name)
}

/// Looks up enum-case metadata by PHP-style enum name and case-sensitive case name.
pub(super) fn resolve_reflection_enum_case<'a>(
    ctx: &'a FunctionContext<'_>,
    enum_name: &str,
    case_name: &str,
) -> Option<(&'a str, &'a crate::types::EnumCaseInfo)> {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
        .and_then(|(name, info)| {
            info.cases
                .iter()
                .find(|case| case.name == case_name)
                .map(|case| (name.as_str(), case))
        })
}

/// Returns a static Reflection value for a backed enum case, when present.
pub(super) fn reflection_enum_case_backing_value(case: &EnumCaseInfo) -> Option<ReflectionConstantValue> {
    match case.value.as_ref()? {
        EnumCaseValue::Int(value) => Some(ReflectionConstantValue::Int(*value)),
        EnumCaseValue::Str(value) => Some(ReflectionConstantValue::Str(value.clone())),
    }
}
