//! Purpose:
//! Folds representable PHP default expressions into one resolved constant value tree.
//! Owns the single definition of which defaults the backend can materialize at all.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::eval` default registration.
//! - `crate::codegen::runtime_callable_invoker::defaults` descriptor invoker metadata.
//!
//! Key details:
//! - Resolution is PURE. Global constants come from `Module::global_constants`, which already
//!   holds the builtin catalog, and class-like constants follow PHP class, interface and trait
//!   ancestry, so a folded value never depends on runtime state.
//! - The resolved tree is target-neutral and carries no ABI encoding beyond the compact scalar
//!   `kind`/`payload` pair the eval bridge already registers, so both consumers read one shape:
//!   the eval registration path encodes it, and the descriptor invoker materializes it.
//! - Constant recursion is bounded by `MAX_CONST_DEFAULT_DEPTH`, so a self-referential constant
//!   declines instead of recursing forever.

use crate::names::php_symbol_key;
use crate::ir::Module;
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver};
use crate::types::{is_php_integer_array_key, ClassInfo, InterfaceInfo};

/// Compact scalar kind for PHP `null` in a resolved default.
pub(in crate::codegen) const CONST_DEFAULT_NULL: i64 = 0;
/// Compact scalar kind for a PHP bool in a resolved default.
pub(in crate::codegen) const CONST_DEFAULT_BOOL: i64 = 1;
/// Compact scalar kind for a PHP int in a resolved default.
pub(in crate::codegen) const CONST_DEFAULT_INT: i64 = 2;
/// Compact scalar kind for a PHP float in a resolved default, carried as raw `f64` bits.
pub(in crate::codegen) const CONST_DEFAULT_FLOAT: i64 = 3;
/// Compact scalar kind for the empty PHP array in a resolved default.
pub(in crate::codegen) const CONST_DEFAULT_EMPTY_ARRAY: i64 = 4;

/// Largest constructor argument count an object-valued default may carry.
pub(in crate::codegen) const MAX_CONST_DEFAULT_OBJECT_ARGS: usize = u8::MAX as usize;
/// Recursion bound for constant-backed default resolution.
pub(in crate::codegen) const MAX_CONST_DEFAULT_DEPTH: usize =
    crate::types::signatures::COMPACT_NATIVE_DEFAULT_MAX_DEPTH;

/// One fully resolved PHP default value.
#[derive(Clone, PartialEq)]
pub(in crate::codegen) enum ConstDefaultValue {
    Scalar {
        kind: i64,
        payload: i64,
    },
    String(String),
    Array(Vec<ConstDefaultArrayElement>),
    Object {
        class_name: String,
        args: Vec<ConstDefaultObjectArg>,
    },
}

/// One element of an array-valued resolved default.
#[derive(Clone, PartialEq)]
pub(in crate::codegen) struct ConstDefaultArrayElement {
    pub(in crate::codegen) key: Option<ConstDefaultArrayKey>,
    pub(in crate::codegen) default: ConstDefaultValue,
}

/// Static PHP array key retained for an array-valued resolved default.
#[derive(Clone, PartialEq)]
pub(in crate::codegen) enum ConstDefaultArrayKey {
    Int(i64),
    String(String),
}

/// One constructor argument of an object-valued resolved default.
#[derive(Clone, PartialEq)]
pub(in crate::codegen) struct ConstDefaultObjectArg {
    pub(in crate::codegen) name: Option<String>,
    pub(in crate::codegen) default: ConstDefaultValue,
}

/// Scope a default expression resolves in: the module plus the enclosing class, if any.
#[derive(Clone, Copy)]
pub(in crate::codegen) struct ConstDefaultContext<'a> {
    pub(in crate::codegen) module: &'a Module,
    pub(in crate::codegen) current_class: Option<&'a str>,
}

impl<'a> ConstDefaultContext<'a> {
    /// Builds a resolution context for a class-like member default.
    pub(in crate::codegen) fn for_class(module: &'a Module, class_name: &'a str) -> Self {
        Self {
            module,
            current_class: Some(class_name),
        }
    }
}

/// Resolves one PHP default expression into a materializable constant value.
pub(in crate::codegen) fn resolve_const_default(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
) -> Option<ConstDefaultValue> {
    resolve_const_default_at(expr, context, 0)
}

/// Resolves one PHP default expression while preserving the constant recursion limit.
pub(in crate::codegen) fn resolve_const_default_at(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<ConstDefaultValue> {
    if depth > MAX_CONST_DEFAULT_DEPTH {
        return None;
    }
    resolve_literal_const_default(expr)
        .or_else(|| resolve_object_const_default(expr, context, depth))
        .or_else(|| resolve_array_const_default(expr, context, depth))
        .or_else(|| resolve_constant_expression_const_default(expr, context, depth))
}

/// Resolves representable pure constant expressions into a constant value.
fn resolve_constant_expression_const_default(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<ConstDefaultValue> {
    match &expr.kind {
        ExprKind::ConstRef(name) => resolve_global_constant_default(context, name, depth + 1),
        ExprKind::ClassConstant { receiver } => {
            resolve_static_receiver_name(context, receiver).map(ConstDefaultValue::String)
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            resolve_scoped_constant_default(context, receiver, name, depth + 1)
        }
        ExprKind::BinaryOp { left, op, right } => {
            resolve_binary_expression_default(left, op, right, context, depth + 1)
        }
        ExprKind::Not(inner) => {
            const_default_truthy(&resolve_const_default_at(inner, context, depth + 1)?)
                .map(|value| const_default_bool(!value))
        }
        ExprKind::BitNot(inner) => {
            const_default_int(inner, context, depth + 1).map(|value| const_default_int_value(!value))
        }
        ExprKind::NullCoalesce { value, default } => {
            let value = resolve_const_default_at(value, context, depth + 1)?;
            if const_default_is_null(&value) {
                resolve_const_default_at(default, context, depth + 1)
            } else {
                Some(value)
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            if const_default_truthy(&resolve_const_default_at(condition, context, depth + 1)?)? {
                resolve_const_default_at(then_expr, context, depth + 1)
            } else {
                resolve_const_default_at(else_expr, context, depth + 1)
            }
        }
        ExprKind::ShortTernary { value, default } => {
            let value = resolve_const_default_at(value, context, depth + 1)?;
            if const_default_truthy(&value)? {
                Some(value)
            } else {
                resolve_const_default_at(default, context, depth + 1)
            }
        }
        _ => None,
    }
}

/// Resolves one supported binary constant expression into a constant value.
fn resolve_binary_expression_default(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<ConstDefaultValue> {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
            resolve_numeric_binary_default(left, op, right, context, depth + 1)
        }
        BinOp::Mod => {
            let left = const_default_int(left, context, depth + 1)?;
            let right = const_default_int(right, context, depth + 1)?;
            (right != 0).then(|| const_default_int_value(left % right))
        }
        BinOp::Concat => {
            let left = const_default_string(left, context, depth + 1)?;
            let right = const_default_string(right, context, depth + 1)?;
            Some(ConstDefaultValue::String(format!("{left}{right}")))
        }
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
            let left = const_default_int(left, context, depth + 1)?;
            let right = const_default_int(right, context, depth + 1)?;
            let value = match op {
                BinOp::BitAnd => left & right,
                BinOp::BitOr => left | right,
                BinOp::BitXor => left ^ right,
                _ => unreachable!("bitwise default operator was prefiltered"),
            };
            Some(const_default_int_value(value))
        }
        BinOp::ShiftLeft | BinOp::ShiftRight => {
            let left = const_default_int(left, context, depth + 1)?;
            let right = u32::try_from(const_default_int(right, context, depth + 1)?).ok()?;
            let value = match op {
                BinOp::ShiftLeft => left.checked_shl(right),
                BinOp::ShiftRight => left.checked_shr(right),
                _ => unreachable!("shift default operator was prefiltered"),
            }?;
            Some(const_default_int_value(value))
        }
        BinOp::And | BinOp::Or | BinOp::Xor => {
            let left =
                const_default_truthy(&resolve_const_default_at(left, context, depth + 1)?)?;
            let right =
                const_default_truthy(&resolve_const_default_at(right, context, depth + 1)?)?;
            let value = match op {
                BinOp::And => left && right,
                BinOp::Or => left || right,
                BinOp::Xor => left ^ right,
                _ => unreachable!("logical default operator was prefiltered"),
            };
            Some(const_default_bool(value))
        }
        BinOp::NullCoalesce => {
            let left = resolve_const_default_at(left, context, depth + 1)?;
            if const_default_is_null(&left) {
                resolve_const_default_at(right, context, depth + 1)
            } else {
                Some(left)
            }
        }
        _ => None,
    }
}

/// Resolves one supported arithmetic expression into a constant value.
fn resolve_numeric_binary_default(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<ConstDefaultValue> {
    if let (Some(left), Some(right)) = (
        const_default_int(left, context, depth + 1),
        const_default_int(right, context, depth + 1),
    ) {
        return match op {
            BinOp::Add => left.checked_add(right).map(const_default_int_value),
            BinOp::Sub => left.checked_sub(right).map(const_default_int_value),
            BinOp::Mul => left.checked_mul(right).map(const_default_int_value),
            BinOp::Div if right != 0 => Some(const_default_float(left as f64 / right as f64)),
            BinOp::Pow => {
                let value = (left as f64).powf(right as f64);
                value.is_finite().then(|| const_default_float(value))
            }
            _ => None,
        };
    }

    let left = const_default_numeric(left, context, depth + 1)?;
    let right = const_default_numeric(right, context, depth + 1)?;
    let value = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div if right != 0.0 => left / right,
        BinOp::Pow => left.powf(right),
        _ => return None,
    };
    value.is_finite().then(|| const_default_float(value))
}

/// Builds one bool constant value.
pub(in crate::codegen) fn const_default_bool(value: bool) -> ConstDefaultValue {
    ConstDefaultValue::Scalar {
        kind: CONST_DEFAULT_BOOL,
        payload: i64::from(value),
    }
}

/// Builds one int constant value.
pub(in crate::codegen) fn const_default_int_value(value: i64) -> ConstDefaultValue {
    ConstDefaultValue::Scalar {
        kind: CONST_DEFAULT_INT,
        payload: value,
    }
}

/// Builds one float constant value, carried as raw `f64` bits.
pub(in crate::codegen) fn const_default_float(value: f64) -> ConstDefaultValue {
    ConstDefaultValue::Scalar {
        kind: CONST_DEFAULT_FLOAT,
        payload: value.to_bits() as i64,
    }
}

/// Builds the PHP `null` constant value.
pub(in crate::codegen) fn const_default_null() -> ConstDefaultValue {
    ConstDefaultValue::Scalar {
        kind: CONST_DEFAULT_NULL,
        payload: 0,
    }
}

/// Returns true when one resolved constant value is PHP `null`.
pub(in crate::codegen) fn const_default_is_null(default: &ConstDefaultValue) -> bool {
    matches!(
        default,
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_NULL,
            ..
        }
    )
}

/// Returns PHP truthiness for one resolved constant value.
pub(in crate::codegen) fn const_default_truthy(default: &ConstDefaultValue) -> Option<bool> {
    match default {
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_NULL,
            ..
        } => Some(false),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_BOOL,
            payload,
        } => Some(*payload != 0),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_INT,
            payload,
        } => Some(*payload != 0),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_FLOAT,
            payload,
        } => Some(f64::from_bits(*payload as u64) != 0.0),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_EMPTY_ARRAY,
            ..
        } => Some(false),
        ConstDefaultValue::String(value) => Some(!value.is_empty() && value != "0"),
        ConstDefaultValue::Array(_) | ConstDefaultValue::Object { .. } => None,
        ConstDefaultValue::Scalar { .. } => None,
    }
}

/// Extracts an int value from one resolvable default expression.
fn const_default_int(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<i64> {
    match resolve_const_default_at(expr, context, depth)? {
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_INT,
            payload,
        } => Some(payload),
        _ => None,
    }
}

/// Extracts a numeric value from one resolvable default expression.
fn const_default_numeric(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<f64> {
    match resolve_const_default_at(expr, context, depth)? {
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_INT,
            payload,
        } => Some(payload as f64),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_FLOAT,
            payload,
        } => Some(f64::from_bits(payload as u64)),
        _ => None,
    }
}

/// Extracts a string value from one resolvable default expression.
fn const_default_string(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<String> {
    match resolve_const_default_at(expr, context, depth)? {
        ConstDefaultValue::String(value) => Some(value),
        _ => None,
    }
}

/// Resolves scalar, string and empty-array literal defaults without any module lookup.
pub(in crate::codegen) fn resolve_literal_const_default(expr: &Expr) -> Option<ConstDefaultValue> {
    match &expr.kind {
        ExprKind::Null => Some(const_default_null()),
        ExprKind::BoolLiteral(value) => Some(const_default_bool(*value)),
        ExprKind::IntLiteral(value) => Some(const_default_int_value(*value)),
        ExprKind::FloatLiteral(value) => Some(const_default_float(*value)),
        ExprKind::StringLiteral(value) => Some(ConstDefaultValue::String(value.clone())),
        ExprKind::ArrayLiteral(elements) if elements.is_empty() => {
            Some(ConstDefaultValue::Scalar {
                kind: CONST_DEFAULT_EMPTY_ARRAY,
                payload: 0,
            })
        }
        ExprKind::Negate(inner) => resolve_negated_literal_default(inner),
        _ => None,
    }
}

/// Resolves a negated numeric literal default.
fn resolve_negated_literal_default(expr: &Expr) -> Option<ConstDefaultValue> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => value.checked_neg().map(const_default_int_value),
        ExprKind::FloatLiteral(value) => Some(const_default_float(-*value)),
        _ => None,
    }
}

/// Resolves supported object-valued defaults into a constant value.
fn resolve_object_const_default(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<ConstDefaultValue> {
    let ExprKind::NewObject { class_name, args } = &expr.kind else {
        return None;
    };
    if args.len() > MAX_CONST_DEFAULT_OBJECT_ARGS {
        return None;
    }
    let mut default_args = Vec::with_capacity(args.len());
    for arg in args {
        default_args.push(resolve_object_const_default_arg(arg, context, depth + 1)?);
    }
    Some(ConstDefaultValue::Object {
        class_name: class_name.as_canonical(),
        args: default_args,
    })
}

/// Resolves one object-valued default constructor argument.
fn resolve_object_const_default_arg(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<ConstDefaultObjectArg> {
    match &expr.kind {
        ExprKind::NamedArg { name, value } => Some(ConstDefaultObjectArg {
            name: Some(name.clone()),
            default: resolve_const_default_at(value, context, depth + 1)?,
        }),
        ExprKind::Spread(_) => None,
        _ => Some(ConstDefaultObjectArg {
            name: None,
            default: resolve_const_default_at(expr, context, depth + 1)?,
        }),
    }
}

/// Resolves supported array-valued defaults into a constant value.
pub(in crate::codegen) fn resolve_array_const_default(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<ConstDefaultValue> {
    match &expr.kind {
        ExprKind::ArrayLiteral(elements) => {
            let mut default_elements = Vec::with_capacity(elements.len());
            for element in elements {
                if matches!(element.kind, ExprKind::Spread(_)) {
                    return None;
                }
                default_elements.push(ConstDefaultArrayElement {
                    key: None,
                    default: resolve_const_default_at(element, context, depth + 1)?,
                });
            }
            Some(ConstDefaultValue::Array(default_elements))
        }
        ExprKind::ArrayLiteralAssoc(elements) => {
            let mut default_elements = Vec::with_capacity(elements.len());
            for (key, value) in elements {
                default_elements.push(ConstDefaultArrayElement {
                    key: Some(resolve_array_const_default_key(key, context, depth + 1)?),
                    default: resolve_const_default_at(value, context, depth + 1)?,
                });
            }
            Some(ConstDefaultValue::Array(default_elements))
        }
        _ => None,
    }
}

/// Resolves one supported static array key.
fn resolve_array_const_default_key(
    expr: &Expr,
    context: &ConstDefaultContext<'_>,
    depth: usize,
) -> Option<ConstDefaultArrayKey> {
    if let Some(key) = resolve_literal_array_const_default_key(expr) {
        return Some(key);
    }
    match resolve_const_default_at(expr, context, depth + 1)? {
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_NULL,
            ..
        } => Some(ConstDefaultArrayKey::String(String::new())),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_BOOL,
            payload,
        } => Some(ConstDefaultArrayKey::Int((payload != 0) as i64)),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_INT,
            payload,
        } => Some(ConstDefaultArrayKey::Int(payload)),
        ConstDefaultValue::Scalar {
            kind: CONST_DEFAULT_FLOAT,
            payload,
        } => Some(ConstDefaultArrayKey::Int(
            f64::from_bits(payload as u64) as i64
        )),
        ConstDefaultValue::String(value) => const_default_string_array_key(&value),
        _ => None,
    }
}

/// Resolves one literal static array key.
pub(in crate::codegen) fn resolve_literal_array_const_default_key(
    expr: &Expr,
) -> Option<ConstDefaultArrayKey> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(ConstDefaultArrayKey::Int(*value)),
        ExprKind::BoolLiteral(value) => Some(ConstDefaultArrayKey::Int(i64::from(*value))),
        ExprKind::FloatLiteral(value) => Some(ConstDefaultArrayKey::Int(*value as i64)),
        ExprKind::StringLiteral(value) => const_default_string_array_key(value),
        ExprKind::Null => Some(ConstDefaultArrayKey::String(String::new())),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg().map(ConstDefaultArrayKey::Int),
            ExprKind::FloatLiteral(value) => Some(ConstDefaultArrayKey::Int((-*value) as i64)),
            _ => None,
        },
        _ => None,
    }
}

/// Normalizes one string array key to PHP's integer-key rules.
pub(in crate::codegen) fn const_default_string_array_key(
    value: &str,
) -> Option<ConstDefaultArrayKey> {
    if is_php_integer_array_key(value) {
        value.parse::<i64>().ok().map(ConstDefaultArrayKey::Int)
    } else {
        Some(ConstDefaultArrayKey::String(value.to_string()))
    }
}

/// Resolves one global constant default expression.
fn resolve_global_constant_default(
    context: &ConstDefaultContext<'_>,
    name: &str,
    depth: usize,
) -> Option<ConstDefaultValue> {
    let expr_kind = context
        .module
        .global_constants
        .get(name)
        .or_else(|| {
            context
                .module
                .global_constants
                .get(name.trim_start_matches('\\'))
        })
        .map(|(expr_kind, _)| expr_kind.clone())?;
    let expr = Expr::new(expr_kind, crate::span::Span::dummy());
    resolve_const_default_at(&expr, context, depth + 1)
}

/// Resolves one class-like constant default expression.
fn resolve_scoped_constant_default(
    context: &ConstDefaultContext<'_>,
    receiver: &StaticReceiver,
    constant_name: &str,
    depth: usize,
) -> Option<ConstDefaultValue> {
    let class_name = resolve_static_receiver_name(context, receiver)?;
    if let Some((declaring_name, value)) =
        const_default_class_constant_expr(context.module, &class_name, constant_name)
    {
        let nested = ConstDefaultContext::for_class(context.module, declaring_name);
        return resolve_const_default_at(value, &nested, depth + 1);
    }
    if let Some((declaring_name, value)) =
        const_default_interface_constant_expr(context.module, &class_name, constant_name)
    {
        let nested = ConstDefaultContext::for_class(context.module, declaring_name);
        return resolve_const_default_at(value, &nested, depth + 1);
    }
    if let Some((declaring_name, value)) =
        const_default_trait_constant_expr(context.module, &class_name, constant_name)
    {
        let nested = ConstDefaultContext::for_class(context.module, declaring_name);
        return resolve_const_default_at(value, &nested, depth + 1);
    }
    None
}

/// Resolves `self`, `static`, `parent`, or a named receiver for default constants.
pub(in crate::codegen) fn resolve_static_receiver_name(
    context: &ConstDefaultContext<'_>,
    receiver: &StaticReceiver,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => {
            Some(name.as_canonical().trim_start_matches('\\').to_string())
        }
        StaticReceiver::Self_ | StaticReceiver::Static => {
            context.current_class.map(str::to_string)
        }
        StaticReceiver::Parent => {
            let current = context.current_class?;
            resolve_const_default_class(context.module, current)
                .and_then(|(_, class_info)| class_info.parent.clone())
        }
    }
}

/// Looks up a class constant expression, including inherited interfaces and parent classes.
fn const_default_class_constant_expr<'a>(
    module: &'a Module,
    class_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a Expr)> {
    let (resolved_name, class_info) = resolve_const_default_class(module, class_name)?;
    if let Some(value) = class_info.constants.get(constant_name) {
        return Some((resolved_name, value));
    }
    for interface_name in &class_info.interfaces {
        if let Some(value) =
            const_default_interface_constant_expr(module, interface_name, constant_name)
        {
            return Some(value);
        }
    }
    if let Some(parent_name) = class_info.parent.as_deref() {
        return const_default_class_constant_expr(module, parent_name, constant_name);
    }
    None
}

/// Looks up an interface constant expression, including inherited interfaces.
fn const_default_interface_constant_expr<'a>(
    module: &'a Module,
    interface_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a Expr)> {
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![interface_name.to_string()];
    while let Some(name) = queue.pop() {
        let Some((resolved_name, interface_info)) = resolve_const_default_interface(module, &name)
        else {
            continue;
        };
        if !visited.insert(php_symbol_key(resolved_name.trim_start_matches('\\'))) {
            continue;
        }
        if let Some(value) = interface_info.constants.get(constant_name) {
            return Some((resolved_name, value));
        }
        queue.extend(interface_info.parents.iter().cloned());
    }
    None
}

/// Looks up a direct trait constant expression by PHP-style trait name.
fn const_default_trait_constant_expr<'a>(
    module: &'a Module,
    trait_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a Expr)> {
    let trait_key = php_symbol_key(trait_name.trim_start_matches('\\'));
    let resolved_name = module
        .trait_table
        .names
        .iter()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == trait_key)?;
    let value = module
        .declared_trait_constants
        .get(resolved_name)
        .and_then(|constants| constants.get(constant_name))?;
    Some((resolved_name.as_str(), value))
}

/// Looks up class metadata by PHP-style case-insensitive name.
pub(in crate::codegen) fn resolve_const_default_class<'a>(
    module: &'a Module,
    class_name: &str,
) -> Option<(&'a str, &'a ClassInfo)> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(name, info)| (name.as_str(), info))
}

/// Looks up interface metadata by PHP-style case-insensitive name.
pub(in crate::codegen) fn resolve_const_default_interface<'a>(
    module: &'a Module,
    interface_name: &str,
) -> Option<(&'a str, &'a InterfaceInfo)> {
    let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
    module
        .interface_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == interface_key)
        .map(|(name, info)| (name.as_str(), info))
}
