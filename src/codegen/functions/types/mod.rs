//! Purpose:
//! Collects codegen-facing type queries for arrays, objects, unions, builtins, and type expressions.
//! Presents a narrow interface for result-type decisions needed before instructions are emitted.
//!
//! Called from:
//! - `crate::codegen::functions` and expression lowering
//!
//! Key details:
//! - These helpers must stay consistent with type checker signatures and runtime value layouts.

use crate::codegen::context::Context;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{merge_array_key_types, normalized_array_key_type, FunctionSig, PhpType};

mod arrays;
mod builtins;
mod objects;
mod type_expr;
mod union;

use arrays::{
    array_union_type, is_empty_indexed_array_literal, mixed_container_value_type, wider_of,
};
use builtins::infer_function_call_type;
use objects::{
    infer_method_call_type, infer_nullsafe_method_call_type, infer_nullsafe_property_access_type,
    infer_property_access_type, infer_static_method_call_type, infer_static_property_access_type,
    infer_this_type,
};
pub(crate) use objects::singular_object_class;
pub(crate) use type_expr::{codegen_declared_type, codegen_static_type};
use type_expr::resolve_buffer_element_type;

/// Infers the static PHP type of `expr` using the given function signature and context.
///
/// A convenience wrapper that always passes `Some(ctx)` to `infer_local_type`.
pub fn infer_local_type_with_ctx(expr: &Expr, sig: &FunctionSig, ctx: &Context) -> PhpType {
    infer_local_type(expr, sig, Some(ctx))
}

/// Infers the contextual type for `expr` when used as the callee in a context such as property access.
///
/// Constructs an empty function signature (no parameters, void return) and delegates to
/// `infer_local_type`. Used when the expression itself determines the type based on its context,
/// not its position as a function argument.
pub fn infer_contextual_type(expr: &Expr, ctx: &Context) -> PhpType {
    let empty_sig = FunctionSig {
        params: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::Void,
        declared_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
    };
    infer_local_type(expr, &empty_sig, Some(ctx))
}

/// Infers the static PHP type of `expr` for codegen decisions.
///
/// This is the core type-inference entry point. It recurses over the expression AST,
/// using `sig` to resolve parameter types and `ctx` to resolve variable types and class metadata.
/// Returns the narrowest known type that is consistent with the PHP semantics of the construct.
/// Falls back to `PhpType::Int` for unknown variables and unhandled expression kinds.
pub(super) fn infer_local_type(
    expr: &Expr,
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    match &expr.kind {
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::Variable(name) => {
            for (pname, pty) in &sig.params {
                if pname == name {
                    return pty.clone();
                }
            }
            if let Some(c) = ctx {
                if let Some(var) = c.variables.get(name) {
                    return var.static_ty.clone();
                }
            }
            PhpType::Int
        }
        ExprKind::ArrayLiteral(elems) => {
            if elems.iter().any(|elem| {
                matches!(
                    &elem.kind,
                    ExprKind::Spread(inner)
                        if matches!(
                            infer_local_type(inner, sig, ctx),
                            PhpType::AssocArray { .. }
                        )
                )
            }) {
                return PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(assoc_spread_literal_value_type(elems, sig, ctx)),
                };
            }
            let mut elem_ty = PhpType::Never;
            for (i, elem) in elems.iter().enumerate() {
                let next_ty = indexed_literal_element_type(elem, sig, ctx);
                if i == 0 {
                    elem_ty = next_ty;
                } else {
                    elem_ty = merge_indexed_literal_element_type(&elem_ty, &next_ty, ctx);
                }
            }
            PhpType::Array(Box::new(elem_ty))
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            let mut key_ty = pairs
                .first()
                .map(|(key, _)| normalized_array_key_type(key, infer_local_type(key, sig, ctx)))
                .unwrap_or(PhpType::Mixed);
            let mut value_ty = pairs
                .first()
                .map(|(_, value)| mixed_container_value_type(infer_local_type(value, sig, ctx)))
                .unwrap_or(PhpType::Mixed);
            for (key, value) in pairs.iter().skip(1) {
                key_ty = merge_array_key_types(
                    key_ty,
                    normalized_array_key_type(key, infer_local_type(key, sig, ctx)),
                );
                let next_ty = mixed_container_value_type(infer_local_type(value, sig, ctx));
                if next_ty != value_ty {
                    value_ty = PhpType::Mixed;
                }
            }
            PhpType::AssocArray {
                key: Box::new(key_ty),
                value: Box::new(value_ty),
            }
        }
        ExprKind::ArrayAccess { array, .. } => match infer_local_type(array, sig, ctx) {
            PhpType::Str => PhpType::Str,
            PhpType::Array(t) => *t,
            PhpType::AssocArray { value, .. } => *value,
            PhpType::Object(class_name) => ctx
                .filter(|ctx| ctx.object_type_implements_interface(&class_name, "ArrayAccess"))
                .map(|ctx| array_access_offset_get_type(ctx, &class_name))
                .unwrap_or(PhpType::Int),
            PhpType::Union(members) => {
                let mut result_members = Vec::new();
                for member in members {
                    match member {
                        PhpType::Void => result_members.push(PhpType::Void),
                        PhpType::Str => result_members.push(PhpType::Str),
                        PhpType::Array(t) => result_members.push(*t),
                        PhpType::AssocArray { value, .. } => result_members.push(*value),
                        PhpType::Object(class_name) => {
                            if let Some(c) = ctx {
                                if c.object_type_implements_interface(&class_name, "ArrayAccess") {
                                    result_members
                                        .push(array_access_offset_get_type(c, &class_name));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if result_members.is_empty() {
                    PhpType::Int
                } else {
                    union::merge_union_members(result_members)
                }
            }
            PhpType::Buffer(t) => match *t {
                PhpType::Packed(name) => PhpType::Pointer(Some(name)),
                other => other,
            },
            _ => PhpType::Int,
        },
        ExprKind::Negate(inner) => {
            let inner_ty = infer_local_type(inner, sig, ctx);
            if inner_ty == PhpType::Float {
                PhpType::Float
            } else {
                PhpType::Int
            }
        }
        ExprKind::Not(_) => PhpType::Bool,
        ExprKind::BitNot(_) => PhpType::Int,
        ExprKind::ErrorSuppress(inner) => infer_local_type(inner, sig, ctx),
        ExprKind::NullCoalesce { value, default } => {
            let left = infer_local_type(value, sig, ctx);
            let right = infer_local_type(default, sig, ctx);
            wider_of(&left, &right)
        }
        ExprKind::Pipe { callable, .. } => infer_pipe_result_type(callable, sig, ctx),
        ExprKind::Assignment { value, .. } => infer_local_type(value, sig, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = infer_local_type(then_expr, sig, ctx);
            let else_ty = infer_local_type(else_expr, sig, ctx);
            wider_of(&then_ty, &else_ty)
        }
        ExprKind::ShortTernary { value, default } => {
            let value_ty = infer_local_type(value, sig, ctx);
            let default_ty = infer_local_type(default, sig, ctx);
            wider_of(&value_ty, &default_ty)
        }
        ExprKind::BinaryOp { left, op, right } => {
            use crate::parser::ast::BinOp;
            match op {
                BinOp::Concat => PhpType::Str,
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::LtEq
                | BinOp::GtEq
                | BinOp::StrictEq
                | BinOp::StrictNotEq
                | BinOp::And
                | BinOp::Or
                | BinOp::Xor => PhpType::Bool,
                BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::ShiftLeft
                | BinOp::ShiftRight
                | BinOp::Spaceship => PhpType::Int,
                BinOp::NullCoalesce => {
                    let lt = infer_local_type(left, sig, ctx);
                    let rt = infer_local_type(right, sig, ctx);
                    wider_of(&lt, &rt)
                }
                BinOp::Div | BinOp::Pow => PhpType::Float,
                BinOp::Add => {
                    let lt = infer_local_type(left, sig, ctx);
                    let rt = infer_local_type(right, sig, ctx);
                    if matches!((&lt, &rt), (PhpType::Array(_), PhpType::Array(_)))
                        && is_empty_indexed_array_literal(left)
                    {
                        rt
                    } else if matches!((&lt, &rt), (PhpType::Array(_), PhpType::Array(_)))
                        && is_empty_indexed_array_literal(right)
                    {
                        lt
                    } else if let Some(ty) = array_union_type(&lt, &rt) {
                        ty
                    } else if lt == PhpType::Float || rt == PhpType::Float {
                        PhpType::Float
                    } else {
                        PhpType::Int
                    }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Mod => {
                    let lt = infer_local_type(left, sig, ctx);
                    let rt = infer_local_type(right, sig, ctx);
                    if lt == PhpType::Float || rt == PhpType::Float {
                        PhpType::Float
                    } else {
                        PhpType::Int
                    }
                }
            }
        }
        ExprKind::InstanceOf { .. } => PhpType::Bool,
        ExprKind::FunctionCall { name, args } => {
            infer_function_call_type(name.as_str(), args, sig, ctx)
        }
        ExprKind::Cast { target, .. } => {
            use crate::parser::ast::CastType;
            match target {
                CastType::Int => PhpType::Int,
                CastType::Float => PhpType::Float,
                CastType::String => PhpType::Str,
                CastType::Bool => PhpType::Bool,
                CastType::Array => PhpType::Array(Box::new(PhpType::Int)),
            }
        }
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => PhpType::Callable,
        ExprKind::ClosureCall { var, .. } => {
            if let Some(c) = ctx {
                if let Some(class_name) = c
                    .variables
                    .get(var)
                    .and_then(|info| singular_object_class(&info.static_ty))
                {
                    if let Some(sig) = c
                        .classes
                        .get(class_name)
                        .and_then(|class_info| class_info.methods.get("__invoke"))
                    {
                        return sig.return_type.clone();
                    }
                }
                if let Some(sig) = c.closure_sigs.get(var) {
                    return sig.return_type.clone();
                }
            }
            PhpType::Int
        }
        ExprKind::ExprCall { callee, .. } => {
            if let Some(c) = ctx {
                let callee_ty = infer_local_type(callee, sig, Some(c));
                if let Some(class_name) = singular_object_class(&callee_ty) {
                    if let Some(sig) = c
                        .classes
                        .get(class_name)
                        .and_then(|class_info| class_info.methods.get("__invoke"))
                    {
                        return sig.return_type.clone();
                    }
                }
                match &callee.kind {
                    ExprKind::Variable(var_name) => {
                        if let Some(sig) = c.closure_sigs.get(var_name) {
                            return sig.return_type.clone();
                        }
                    }
                    ExprKind::ArrayAccess { array, .. } => {
                        if let ExprKind::Variable(arr_name) = &array.kind {
                            if let Some(sig) = c.closure_sigs.get(arr_name) {
                                return sig.return_type.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let ExprKind::Closure {
                return_type: Some(type_ann),
                ..
            } = &callee.kind
            {
                return ctx
                    .map(|c| codegen_static_type(type_ann, c))
                    .unwrap_or(PhpType::Mixed);
            }
            if let ExprKind::Closure { body, .. } = &callee.kind {
                return crate::types::checker::infer_return_type_syntactic(body);
            }
            PhpType::Int
        }
        ExprKind::ConstRef(name) => ctx
            .and_then(|c| c.constants.get(name.as_str()).map(|(_, ty)| ty.clone()))
            .unwrap_or(PhpType::Int),
        ExprKind::Spread(inner) => infer_local_type(inner, sig, ctx),
        ExprKind::NamedArg { value, .. } => infer_local_type(value, sig, ctx),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.as_str().to_string()),
        ExprKind::NewDynamic { name_expr, .. } => {
            literal_dynamic_new_object_type(name_expr, ctx).unwrap_or(PhpType::Mixed)
        }
        ExprKind::BufferNew { element_type, .. } => {
            if let Some(c) = ctx {
                let elem_ty = resolve_buffer_element_type(element_type, c);
                PhpType::Buffer(Box::new(elem_ty))
            } else {
                PhpType::Buffer(Box::new(PhpType::Int))
            }
        }
        ExprKind::PropertyAccess { object, property } => {
            infer_property_access_type(object, property, sig, ctx)
        }
        ExprKind::DynamicPropertyAccess { .. } => PhpType::Mixed,
        ExprKind::NullsafePropertyAccess { object, property } => {
            infer_nullsafe_property_access_type(object, property, sig, ctx)
        }
        ExprKind::NullsafeDynamicPropertyAccess { .. } => PhpType::Mixed,
        ExprKind::StaticPropertyAccess { receiver, property } => {
            infer_static_property_access_type(receiver, property, ctx)
        }
        ExprKind::MethodCall { object, method, .. } => {
            infer_method_call_type(object, method, sig, ctx)
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            infer_nullsafe_method_call_type(object, method, sig, ctx)
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => {
            infer_static_method_call_type(receiver, method, ctx)
        }
        ExprKind::This => infer_this_type(ctx),
        ExprKind::PtrCast { target_type, .. } => PhpType::Pointer(Some(target_type.clone())),
        _ => PhpType::Int,
    }
}

/// Resolves a literal `new $class` class-string to an object type for codegen-local inference.
fn literal_dynamic_new_object_type(name_expr: &Expr, ctx: Option<&Context>) -> Option<PhpType> {
    let ExprKind::StringLiteral(class_name) = &name_expr.kind else {
        return None;
    };
    let ctx = ctx?;
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(|class_name| PhpType::Object(class_name.clone()))
}

/// Infers the return type of a pipe (`|>`) expression given the callable at the pipe's RHS.
///
/// Handles `Variable`, `FirstClassCallable`, `Closure` (with or without a return type annotation),
/// and `Assignment` (unwrapping the RHS). Falls back to `PhpType::Int` for unknown callables.
fn infer_pipe_result_type(
    callable: &Expr,
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    match &callable.kind {
        ExprKind::Variable(var_name) => ctx
            .and_then(|c| c.closure_sigs.get(var_name).map(|sig| sig.return_type.clone()))
            .unwrap_or(PhpType::Int),
        ExprKind::FirstClassCallable(target) => ctx
            .and_then(|c| crate::codegen::expr::calls::first_class_callable_sig(target, c))
            .map(|sig| sig.return_type)
            .unwrap_or(PhpType::Int),
        ExprKind::Closure {
            return_type: Some(type_ann),
            ..
        } => ctx
            .map(|c| codegen_static_type(type_ann, c))
            .unwrap_or(PhpType::Mixed),
        ExprKind::Closure { body, .. } => crate::types::checker::infer_return_type_syntactic(body),
        ExprKind::Assignment { value, .. } => infer_pipe_result_type(value, sig, ctx),
        _ => PhpType::Int,
    }
}

/// Returns the element type for `array[key]` on `ArrayAccess` implementations of `class_name`.
///
/// Looks up the `offsetGet` method on the class and returns its return type.
/// Falls back to `PhpType::Mixed` if the class or interface does not define `offsetGet`.
fn array_access_offset_get_type(ctx: &Context, class_name: &str) -> PhpType {
    ctx.classes
        .get(class_name)
        .and_then(|class_info| class_info.methods.get("offsetget"))
        .map(|method_sig| method_sig.return_type.clone())
        .or_else(|| {
            ctx.interfaces
                .get("ArrayAccess")
                .and_then(|interface_info| interface_info.methods.get("offsetget"))
                .map(|method_sig| method_sig.return_type.clone())
        })
        .unwrap_or(PhpType::Mixed)
}

/// Determines the value type of an associative array literal that contains spread elements.
///
/// Iterates over spread elements within `elems`, infers the type of each inner expression,
/// and merges them. Non-spread elements are ignored. Returns `PhpType::Never` when no
/// spread elements exist, which callers treat as "use the regular element type path".
fn assoc_spread_literal_value_type(
    elems: &[Expr],
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    let mut value_ty = PhpType::Never;
    for elem in elems {
        let ExprKind::Spread(inner) = &elem.kind else {
            continue;
        };
        let next = match infer_local_type(inner, sig, ctx) {
            PhpType::Array(elem) => *elem,
            PhpType::AssocArray { value, .. } => *value,
            _ => PhpType::Mixed,
        };
        if matches!(value_ty, PhpType::Never) {
            value_ty = next;
        } else if value_ty != next {
            value_ty = PhpType::Mixed;
        }
    }
    if matches!(value_ty, PhpType::Never) {
        PhpType::Mixed
    } else {
        value_ty
    }
}

/// Returns the element type for one position in an indexed (non-associative) array literal.
///
/// Handles spread elements by unwrapping the inner array type. For regular elements,
/// applies `mixed_container_value_type` to normalize the element type for heterogeneous arrays.
fn indexed_literal_element_type(
    elem: &Expr,
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    match &elem.kind {
        ExprKind::Spread(inner) => match infer_local_type(inner, sig, ctx) {
            PhpType::Array(inner_ty) => mixed_container_value_type(*inner_ty),
            _ => PhpType::Mixed,
        },
        _ => mixed_container_value_type(infer_local_type(elem, sig, ctx)),
    }
}

/// Merges two element types from adjacent positions in an indexed array literal.
///
/// Returns `existing` if types match, handles `Never` identity cases, promotes to `Mixed`
/// when either type is `Mixed` or `Union`, and falls back to `PhpType::Mixed` for mismatched
/// scalar types or unrelated object types. Uses `ctx` to find common object type via `common_object_type`.
fn merge_indexed_literal_element_type(
    existing: &PhpType,
    next: &PhpType,
    ctx: Option<&Context>,
) -> PhpType {
    if existing == next {
        return existing.clone();
    }
    if matches!(existing, PhpType::Never) {
        return next.clone();
    }
    if matches!(next, PhpType::Never) {
        return existing.clone();
    }
    if matches!(existing, PhpType::Mixed | PhpType::Union(_))
        || matches!(next, PhpType::Mixed | PhpType::Union(_))
    {
        return PhpType::Mixed;
    }
    if let (Some(ctx), PhpType::Object(left), PhpType::Object(right)) = (ctx, existing, next) {
        return ctx
            .common_object_type(left, right)
            .unwrap_or(PhpType::Mixed);
    }
    PhpType::Mixed
}
