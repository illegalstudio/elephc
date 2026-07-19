//! Purpose:
//! Detects property backing-slot usage inside parsed hook statements and expressions.
//!
//! Called from:
//! - Property-hook declaration parsing after hook bodies are available.
//!
//! Key details:
//! - The recursive walk covers every EvalIR statement and expression shape without executing code.

use super::*;

/// Returns whether any parsed property hook accessor uses its own backing slot.
pub(super) fn property_hook_methods_use_backing_slot(
    hook_methods: &[EvalClassMethod],
    property_name: &str,
) -> bool {
    hook_methods.iter().any(|method| {
        method
            .body()
            .iter()
            .any(|stmt| eval_stmt_uses_this_property(stmt, property_name))
    })
}

/// Returns whether one statement touches `$this->{$property_name}` directly.
pub(super) fn eval_stmt_uses_this_property(stmt: &EvalStmt, property_name: &str) -> bool {
    match stmt {
        EvalStmt::ArrayAppendVar { value, .. } => {
            eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::ArraySetVar { index, value, .. } => {
            eval_expr_uses_this_property(index, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::Break
        | EvalStmt::Continue
        | EvalStmt::ClassDecl(_)
        | EvalStmt::EnumDecl(_)
        | EvalStmt::FunctionDecl { .. }
        | EvalStmt::Global { .. }
        | EvalStmt::InterfaceDecl(_)
        | EvalStmt::ReferenceAssign { .. }
        | EvalStmt::TraitDecl(_)
        | EvalStmt::UnsetVar { .. } => false,
        EvalStmt::UnsetArrayElement { array, index } => {
            eval_expr_uses_this_property(array, property_name)
                || eval_expr_uses_this_property(index, property_name)
        }
        EvalStmt::DoWhile { body, condition } | EvalStmt::While { condition, body } => {
            eval_expr_uses_this_property(condition, property_name)
                || eval_stmt_list_uses_this_property(body, property_name)
        }
        EvalStmt::Echo(expr)
        | EvalStmt::Expr(expr)
        | EvalStmt::StaticVar { init: expr, .. }
        | EvalStmt::Throw(expr) => eval_expr_uses_this_property(expr, property_name),
        EvalStmt::For {
            init,
            condition,
            update,
            body,
        } => {
            eval_stmt_list_uses_this_property(init, property_name)
                || condition
                    .as_ref()
                    .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name))
                || eval_stmt_list_uses_this_property(update, property_name)
                || eval_stmt_list_uses_this_property(body, property_name)
        }
        EvalStmt::Foreach { array, body, .. } => {
            eval_expr_uses_this_property(array, property_name)
                || eval_stmt_list_uses_this_property(body, property_name)
        }
        EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            eval_expr_uses_this_property(condition, property_name)
                || eval_stmt_list_uses_this_property(then_branch, property_name)
                || eval_stmt_list_uses_this_property(else_branch, property_name)
        }
        EvalStmt::Return(expr) => expr
            .as_ref()
            .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name)),
        EvalStmt::PropertyReferenceBind {
            object, property, ..
        }
        | EvalStmt::UnsetProperty { object, property } => {
            eval_is_this_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
        }
        EvalStmt::DynamicPropertyReferenceBind {
            object, property, ..
        } => {
            eval_is_this_dynamic_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(property, property_name)
        }
        EvalStmt::UnsetDynamicProperty { object, property } => {
            eval_is_this_dynamic_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(property, property_name)
        }
        EvalStmt::UnsetDynamicStaticProperty { class_name, .. } => {
            eval_expr_uses_this_property(class_name, property_name)
        }
        EvalStmt::UnsetDynamicStaticPropertyName {
            class_name,
            property,
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(property, property_name)
        }
        EvalStmt::StaticPropertyIncDec { .. }
        | EvalStmt::StaticPropertyReferenceBind { .. }
        | EvalStmt::UnsetStaticProperty { .. } => false,
        EvalStmt::DynamicPropertySet {
            object,
            property,
            value,
        }
        | EvalStmt::DynamicPropertyArrayAppend {
            object,
            property,
            value,
        }
        | EvalStmt::DynamicPropertyCompoundAssign {
            object,
            property,
            value,
            ..
        } => {
            eval_is_this_dynamic_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(property, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::DynamicPropertyArraySet {
            object,
            property,
            index,
            value,
            ..
        } => {
            eval_is_this_dynamic_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(property, property_name)
                || eval_expr_uses_this_property(index, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::DynamicPropertyIncDec {
            object, property, ..
        } => {
            eval_is_this_dynamic_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(property, property_name)
        }
        EvalStmt::PropertySet {
            object,
            property,
            value,
        }
        | EvalStmt::PropertyArrayAppend {
            object,
            property,
            value,
        }
        | EvalStmt::PropertyCompoundAssign {
            object,
            property,
            value,
            ..
        } => {
            eval_is_this_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::PropertyArraySet {
            object,
            property,
            index,
            value,
            ..
        } => {
            eval_is_this_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(index, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::PropertyIncDec {
            object, property, ..
        } => {
            eval_is_this_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
        }
        EvalStmt::DynamicStaticPropertySet {
            class_name, value, ..
        }
        | EvalStmt::DynamicStaticPropertyArrayAppend {
            class_name, value, ..
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::DynamicStaticPropertyArraySet {
            class_name,
            index,
            value,
            ..
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(index, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::DynamicStaticPropertyIncDec { class_name, .. } => {
            eval_expr_uses_this_property(class_name, property_name)
        }
        EvalStmt::DynamicStaticPropertyReferenceBind { class_name, .. } => {
            eval_expr_uses_this_property(class_name, property_name)
        }
        EvalStmt::DynamicStaticPropertyNameSet {
            class_name,
            property,
            value,
        }
        | EvalStmt::DynamicStaticPropertyNameArrayAppend {
            class_name,
            property,
            value,
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(property, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::DynamicStaticPropertyNameArraySet {
            class_name,
            property,
            index,
            value,
            ..
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(property, property_name)
                || eval_expr_uses_this_property(index, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::DynamicStaticPropertyNameIncDec {
            class_name,
            property,
            ..
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(property, property_name)
        }
        EvalStmt::DynamicStaticPropertyNameReferenceBind {
            class_name,
            property,
            ..
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(property, property_name)
        }
        EvalStmt::StaticPropertySet { value, .. }
        | EvalStmt::StaticPropertyArrayAppend { value, .. }
        | EvalStmt::StoreVar { value, .. } => {
            eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::StaticPropertyArraySet { index, value, .. } => {
            eval_expr_uses_this_property(index, property_name)
                || eval_expr_uses_this_property(value, property_name)
        }
        EvalStmt::Switch { expr, cases } => {
            eval_expr_uses_this_property(expr, property_name)
                || cases.iter().any(|case| {
                    case.condition
                        .as_ref()
                        .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name))
                        || eval_stmt_list_uses_this_property(&case.body, property_name)
                })
        }
        EvalStmt::Try {
            body,
            catches,
            finally_body,
        } => {
            eval_stmt_list_uses_this_property(body, property_name)
                || catches
                    .iter()
                    .any(|catch| eval_stmt_list_uses_this_property(&catch.body, property_name))
                || eval_stmt_list_uses_this_property(finally_body, property_name)
        }
    }
}

/// Returns whether any statement in a list touches `$this->{$property_name}` directly.
pub(super) fn eval_stmt_list_uses_this_property(stmts: &[EvalStmt], property_name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| eval_stmt_uses_this_property(stmt, property_name))
}

/// Returns whether one expression touches `$this->{$property_name}` directly.
pub(super) fn eval_expr_uses_this_property(expr: &EvalExpr, property_name: &str) -> bool {
    match expr {
        EvalExpr::Array(elements) => elements.iter().any(|element| match element {
            EvalArrayElement::Value(value) => eval_expr_uses_this_property(value, property_name),
            EvalArrayElement::Reference(value) => {
                eval_expr_uses_this_property(value, property_name)
            }
            EvalArrayElement::KeyValue { key, value } => {
                eval_expr_uses_this_property(key, property_name)
                    || eval_expr_uses_this_property(value, property_name)
            }
            EvalArrayElement::KeyReference { key, value } => {
                eval_expr_uses_this_property(key, property_name)
                    || eval_expr_uses_this_property(value, property_name)
            }
        }),
        EvalExpr::ArrayGet { array, index } => {
            eval_expr_uses_this_property(array, property_name)
                || eval_expr_uses_this_property(index, property_name)
        }
        EvalExpr::Call { args, .. }
        | EvalExpr::NamespacedCall { args, .. }
        | EvalExpr::NewObject { args, .. }
        | EvalExpr::StaticMethodCall { args, .. } => args
            .iter()
            .any(|arg| eval_expr_uses_this_property(arg.value(), property_name)),
        EvalExpr::DynamicCall { callee, args } => {
            eval_expr_uses_this_property(callee, property_name)
                || args
                    .iter()
                    .any(|arg| eval_expr_uses_this_property(arg.value(), property_name))
        }
        EvalExpr::DynamicNewObject { class_name, args } => {
            eval_expr_uses_this_property(class_name, property_name)
                || args
                    .iter()
                    .any(|arg| eval_expr_uses_this_property(arg.value(), property_name))
        }
        EvalExpr::DynamicStaticMethodCall {
            class_name,
            method,
            args,
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(method, property_name)
                || args
                    .iter()
                    .any(|arg| eval_expr_uses_this_property(arg.value(), property_name))
        }
        EvalExpr::DynamicStaticPropertyGet { class_name, .. }
        | EvalExpr::DynamicClassConstantFetch { class_name, .. }
        | EvalExpr::DynamicClassNameFetch { class_name } => {
            eval_expr_uses_this_property(class_name, property_name)
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(property, property_name)
        }
        EvalExpr::DynamicClassConstantNameFetch {
            class_name,
            constant,
        } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(constant, property_name)
        }
        EvalExpr::DynamicMethodCall {
            object,
            method,
            args,
        }
        | EvalExpr::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(method, property_name)
                || args
                    .iter()
                    .any(|arg| eval_expr_uses_this_property(arg.value(), property_name))
        }
        EvalExpr::Const(_)
        | EvalExpr::ConstFetch(_)
        | EvalExpr::Closure { .. }
        | EvalExpr::FunctionCallable { .. }
        | EvalExpr::ClassConstantFetch { .. }
        | EvalExpr::ClassNameFetch { .. }
        | EvalExpr::LoadVar(_)
        | EvalExpr::Magic(_)
        | EvalExpr::NamespacedConstFetch { .. }
        | EvalExpr::StaticPropertyGet { .. } => false,
        EvalExpr::MethodCallable { object, method } => {
            eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(method, property_name)
        }
        EvalExpr::StaticMethodCallable { method, .. } => {
            eval_expr_uses_this_property(method, property_name)
        }
        EvalExpr::InvokableCallable { object } => {
            eval_expr_uses_this_property(object, property_name)
        }
        EvalExpr::DynamicStaticMethodCallable { class_name, method } => {
            eval_expr_uses_this_property(class_name, property_name)
                || eval_expr_uses_this_property(method, property_name)
        }
        EvalExpr::Include { path, .. }
        | EvalExpr::Cast { expr: path, .. }
        | EvalExpr::Clone(path)
        | EvalExpr::Print(path)
        | EvalExpr::Unary { expr: path, .. } => eval_expr_uses_this_property(path, property_name),
        EvalExpr::InstanceOf { value, target } => {
            eval_expr_uses_this_property(value, property_name)
                || matches!(
                    target,
                    EvalInstanceOfTarget::Expr(target)
                        if eval_expr_uses_this_property(target, property_name)
                )
        }
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => {
            eval_expr_uses_this_property(subject, property_name)
                || arms.iter().any(|arm| {
                    arm.patterns
                        .iter()
                        .any(|pattern| eval_expr_uses_this_property(pattern, property_name))
                        || eval_expr_uses_this_property(&arm.value, property_name)
                })
                || default
                    .as_ref()
                    .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name))
        }
        EvalExpr::MethodCall { object, args, .. } => {
            eval_expr_uses_this_property(object, property_name)
                || args
                    .iter()
                    .any(|arg| eval_expr_uses_this_property(arg.value(), property_name))
        }
        EvalExpr::NullsafeMethodCall { object, args, .. } => {
            eval_expr_uses_this_property(object, property_name)
                || args
                    .iter()
                    .any(|arg| eval_expr_uses_this_property(arg.value(), property_name))
        }
        EvalExpr::NewAnonymousClass { args, .. } => args
            .iter()
            .any(|arg| eval_expr_uses_this_property(arg.value(), property_name)),
        EvalExpr::NullCoalesce { value, default } => {
            eval_expr_uses_this_property(value, property_name)
                || eval_expr_uses_this_property(default, property_name)
        }
        EvalExpr::PropertyGet { object, property } => {
            eval_is_this_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
        }
        EvalExpr::NullsafePropertyGet { object, property } => {
            eval_is_this_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
        }
        EvalExpr::DynamicPropertyGet { object, property }
        | EvalExpr::NullsafeDynamicPropertyGet { object, property } => {
            eval_is_this_dynamic_property(object, property, property_name)
                || eval_expr_uses_this_property(object, property_name)
                || eval_expr_uses_this_property(property, property_name)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            eval_expr_uses_this_property(condition, property_name)
                || then_branch
                    .as_ref()
                    .is_some_and(|expr| eval_expr_uses_this_property(expr, property_name))
                || eval_expr_uses_this_property(else_branch, property_name)
        }
        EvalExpr::Binary { left, right, .. } => {
            eval_expr_uses_this_property(left, property_name)
                || eval_expr_uses_this_property(right, property_name)
        }
    }
}

/// Returns whether one object/property pair is exactly `$this->{$property_name}`.
pub(super) fn eval_is_this_property(object: &EvalExpr, property: &str, property_name: &str) -> bool {
    matches!(object, EvalExpr::LoadVar(name) if name == "this") && property == property_name
}

/// Returns whether one dynamic object/property pair is exactly `$this->{"property"}`.
pub(super) fn eval_is_this_dynamic_property(
    object: &EvalExpr,
    property: &EvalExpr,
    property_name: &str,
) -> bool {
    matches!(
        (object, property),
        (
            EvalExpr::LoadVar(object_name),
            EvalExpr::Const(EvalConst::String(property))
        ) if object_name == "this" && property == property_name
    )
}

/// Returns the synthetic get-hook method name for one property.
pub(super) fn property_hook_get_method(property_name: &str) -> String {
    format!("__propget_{property_name}")
}

/// Returns the synthetic set-hook method name for one property.
pub(super) fn property_hook_set_method(property_name: &str) -> String {
    format!("__propset_{property_name}")
}

/// Builds the implicit constructor assignment or alias for a promoted parameter.
pub(super) fn promoted_property_assignment(name: &str, is_by_ref: bool) -> EvalStmt {
    if is_by_ref {
        EvalStmt::PropertyReferenceBind {
            object: EvalExpr::LoadVar("this".to_string()),
            property: name.to_string(),
            source: name.to_string(),
        }
    } else {
        EvalStmt::PropertySet {
            object: EvalExpr::LoadVar("this".to_string()),
            property: name.to_string(),
            value: EvalExpr::LoadVar(name.to_string()),
        }
    }
}
