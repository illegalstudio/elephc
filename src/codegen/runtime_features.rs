//! Purpose:
//! Derives the runtime helper families a compiled program can reference.
//! Keeps runtime object generation aligned with user assembly so optional
//! helper groups do not force unrelated native link dependencies.
//!
//! Called from:
//! - `crate::pipeline::compile()` before runtime-cache preparation.
//! - `crate::codegen::generate()` when tests request combined user/runtime assembly.
//!
//! Key details:
//! - Direct `preg_*` calls and emitted regex iterator classes both enable regex
//!   helpers because generated SPL methods can call them.
//! - Emitted stream/archive classes enable PHAR bridge libraries because their
//!   generated methods route dynamic paths through `__rt_*_maybe_phar` helpers.
//! - The dynamic builtin dispatcher (descriptor invoker) emits per-builtin
//!   wrappers — including md5/sha1/hash — that reference the `elephc_crypto`
//!   staticlib, so its detection forces that crate to link.

use std::collections::HashMap;

use crate::names::php_symbol_key;
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, InstanceOfTarget, Program, StaticReceiver, Stmt, StmtKind,
};
use crate::types::ClassInfo;

use super::program_usage::{collect_required_class_names, program_has_dynamic_instanceof};

/// Runtime helper families that can be emitted independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeFeatures {
    pub regex: bool,
    pub phar_archive: bool,
    /// True when codegen can emit the runtime callable dispatcher (descriptor
    /// invoker) that builds per-builtin wrappers referencing `elephc_crypto`.
    pub descriptor_invoker: bool,
}

impl RuntimeFeatures {
    /// Returns an empty feature set for programs that need only the base runtime.
    pub const fn none() -> Self {
        Self {
            regex: false,
            phar_archive: false,
            descriptor_invoker: false,
        }
    }

    /// Returns every optional runtime feature for compatibility with legacy callers.
    #[allow(dead_code)]
    pub const fn all() -> Self {
        Self {
            regex: true,
            phar_archive: true,
            descriptor_invoker: true,
        }
    }
}

/// Returns the optional runtime features referenced by the given optimized program.
#[cfg(test)]
fn runtime_features_for_program(program: &Program) -> RuntimeFeatures {
    runtime_features_for_program_and_classes_opt(program, None)
}

/// Returns optional runtime features referenced by the program and emitted class metadata.
pub fn runtime_features_for_program_and_classes(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> RuntimeFeatures {
    runtime_features_for_program_and_classes_opt(program, Some(classes))
}

/// Returns native libraries required by the selected optional runtime features.
pub fn required_libraries_for_runtime_features(features: RuntimeFeatures) -> Vec<String> {
    let mut libs = Vec::new();
    if features.regex {
        libs.push("pcre2-posix".to_string());
        libs.push("pcre2-8".to_string());
    }
    if features.phar_archive {
        libs.push("elephc_phar".to_string());
        libs.push("z".to_string());
        libs.push("bz2".to_string());
    }
    if features.descriptor_invoker {
        // The dynamic builtin dispatcher emits md5/sha1/hash wrappers that
        // reference `elephc_crypto_hash`; force the crate to link on all targets.
        libs.push("elephc_crypto".to_string());
    }
    libs
}

/// Builds the optional runtime feature set, using class metadata when codegen has it.
fn runtime_features_for_program_and_classes_opt(
    program: &Program,
    classes: Option<&HashMap<String, ClassInfo>>,
) -> RuntimeFeatures {
    let mut features = RuntimeFeatures::none();
    features.regex = program_requires_regex(program, classes);
    features.phar_archive = class_emission_can_reference_phar_archive(program, classes);
    features.descriptor_invoker = program_requires_descriptor_invoker(program);
    features
}

/// Returns true when user code or emitted builtin class methods can call regex helpers.
fn program_requires_regex(program: &Program, classes: Option<&HashMap<String, ClassInfo>>) -> bool {
    body_has_regex_call(program) || class_emission_can_reference_regex(program, classes)
}

/// Returns true when class method emission can reference RegexIterator methods.
fn class_emission_can_reference_regex(
    program: &Program,
    classes: Option<&HashMap<String, ClassInfo>>,
) -> bool {
    match classes {
        Some(classes) => emitted_classes_include_regex_iterators(program, classes),
        None => required_classes_include_regex_iterators(program),
    }
}

/// Returns true when the actual emitted class set includes regex iterator classes.
fn emitted_classes_include_regex_iterators(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> bool {
    if program_has_dynamic_instanceof(program) {
        return classes.keys().any(|name| is_regex_iterator_name(name));
    }
    super::collect_emitted_class_names(program, classes)
        .iter()
        .any(|name| is_regex_iterator_name(name))
}

/// Returns true when required class metadata includes regex iterator classes.
fn required_classes_include_regex_iterators(program: &Program) -> bool {
    collect_required_class_names(program)
        .iter()
        .any(|name| is_regex_iterator_name(name))
}

/// Returns true when a canonical class name denotes a regex iterator class.
fn is_regex_iterator_name(name: &str) -> bool {
    matches!(
        php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "regexiterator" | "recursiveregexiterator"
    )
}

/// Returns true when class method emission can reference PHAR bridge helpers.
fn class_emission_can_reference_phar_archive(
    program: &Program,
    classes: Option<&HashMap<String, ClassInfo>>,
) -> bool {
    match classes {
        Some(classes) => emitted_classes_include_phar_archive_helpers(program, classes),
        None => required_classes_include_phar_archive_helpers(program),
    }
}

/// Returns true when the actual emitted class set includes stream/archive helpers.
fn emitted_classes_include_phar_archive_helpers(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> bool {
    if program_has_dynamic_instanceof(program) {
        return classes.keys().any(|name| is_phar_archive_helper_class_name(name));
    }
    super::collect_emitted_class_names(program, classes)
        .iter()
        .any(|name| is_phar_archive_helper_class_name(name))
}

/// Returns true when required class metadata includes stream/archive helpers.
fn required_classes_include_phar_archive_helpers(program: &Program) -> bool {
    collect_required_class_names(program)
        .iter()
        .any(|name| is_phar_archive_helper_class_name(name))
}

/// Returns true when a class has generated methods that publish PHAR bridge pointers.
fn is_phar_archive_helper_class_name(name: &str) -> bool {
    matches!(
        php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "phar" | "phardata" | "splfileobject" | "spltempfileobject"
    )
}

/// Returns true when a statement body contains a direct regex builtin call.
fn body_has_regex_call(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_has_regex_call)
}

/// Returns true when a statement contains a direct regex builtin call.
fn stmt_has_regex_call(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr))
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::PropertyAssign { value: expr, .. }
        | StmtKind::PropertyArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. }
        | StmtKind::Include { path: expr, .. } => expr_has_regex_call(expr),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_has_regex_call(index) || expr_has_regex_call(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_has_regex_call(target) || expr_has_regex_call(value)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_has_regex_call(condition)
                || body_has_regex_call(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(condition, body)| expr_has_regex_call(condition) || body_has_regex_call(body))
                || else_body.as_deref().is_some_and(body_has_regex_call)
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_has_regex_call(then_body)
                || else_body.as_deref().is_some_and(body_has_regex_call)
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_has_regex_call(condition) || body_has_regex_call(body)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_has_regex_call)
                || condition.as_ref().is_some_and(expr_has_regex_call)
                || update.as_deref().is_some_and(stmt_has_regex_call)
                || body_has_regex_call(body)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_has_regex_call(array) || body_has_regex_call(body)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_has_regex_call(subject)
                || cases.iter().any(|(patterns, body)| {
                    patterns.iter().any(expr_has_regex_call) || body_has_regex_call(body)
                })
                || default.as_deref().is_some_and(body_has_regex_call)
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::FunctionDecl { body, .. } => body_has_regex_call(body),
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => {
            methods.iter().any(|method| body_has_regex_call(&method.body))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_has_regex_call(try_body)
                || catches.iter().any(|catch| body_has_regex_call(&catch.body))
                || finally_body.as_deref().is_some_and(body_has_regex_call)
        }
        StmtKind::Return(None)
        | StmtKind::RefAssign { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

/// Returns true when an expression contains a direct regex builtin call.
fn expr_has_regex_call(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            is_regex_builtin_name(name.as_str())
                || regex_callback_dispatch_call(name.as_str(), args)
                || args.iter().any(expr_has_regex_call)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_has_regex_call(left) || expr_has_regex_call(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_has_regex_call(value)
                || match target {
                    InstanceOfTarget::Name(_) => false,
                    // Dynamic class targets make codegen include the builtin class universe.
                    // That universe contains RegexIterator methods that call preg runtime helpers.
                    InstanceOfTarget::Expr(_) => true,
                }
        }
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::PtrCast { expr, .. }
        | ExprKind::YieldFrom(expr) => expr_has_regex_call(expr),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default }
        | ExprKind::Pipe {
            value,
            callable: default,
        } => expr_has_regex_call(value) || expr_has_regex_call(default),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            body_has_regex_call(prelude)
                || expr_has_regex_call(target)
                || expr_has_regex_call(value)
                || result_target.as_deref().is_some_and(expr_has_regex_call)
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_has_regex_call),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| expr_has_regex_call(key) || expr_has_regex_call(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_has_regex_call(subject)
                || arms.iter().any(|(patterns, value)| {
                    patterns.iter().any(expr_has_regex_call) || expr_has_regex_call(value)
                })
                || default.as_deref().is_some_and(expr_has_regex_call)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_has_regex_call(array) || expr_has_regex_call(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_has_regex_call(condition)
                || expr_has_regex_call(then_expr)
                || expr_has_regex_call(else_expr)
        }
        ExprKind::Closure { body, .. } => body_has_regex_call(body),
        ExprKind::NamedArg { value, .. } => expr_has_regex_call(value),
        ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_has_regex_call),
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            is_regex_iterator_name(fallback_class.as_str())
                || is_regex_iterator_name(required_parent.as_str())
                || expr_has_regex_call(class_name)
                || args.iter().any(expr_has_regex_call)
        }
        ExprKind::ExprCall { callee, args } => {
            expr_has_regex_call(callee) || args.iter().any(expr_has_regex_call)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_has_regex_call(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_has_regex_call(object) || expr_has_regex_call(property)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_has_regex_call(object) || args.iter().any(expr_has_regex_call)
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => {
            expr_has_regex_call(object)
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            is_regex_builtin_name(name.as_str())
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { .. }) => false,
        ExprKind::StaticPropertyAccess { receiver, .. }
        | ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => {
            static_receiver_has_regex_call(receiver)
        }
        ExprKind::BufferNew { len, .. } => expr_has_regex_call(len),
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_has_regex_call)
                || value.as_deref().is_some_and(expr_has_regex_call)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_has_regex_call(name_expr) || args.iter().any(expr_has_regex_call)
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::This
        | ExprKind::MagicConstant(_) => false,
    }
}

/// Returns true when a dispatcher builtin directly targets a regex builtin by string.
fn regex_callback_dispatch_call(name: &str, args: &[Expr]) -> bool {
    if !matches!(
        php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "call_user_func" | "call_user_func_array"
    ) {
        return false;
    }
    args.first().is_some_and(expr_is_regex_callback_string)
}

/// Returns true when an expression is a literal callback name for a regex builtin.
fn expr_is_regex_callback_string(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(name) => is_regex_builtin_name(name),
        ExprKind::NamedArg { value, .. } => expr_is_regex_callback_string(value),
        _ => false,
    }
}

/// Returns true when a static receiver expression can contain a regex call.
fn static_receiver_has_regex_call(receiver: &StaticReceiver) -> bool {
    match receiver {
        StaticReceiver::Named(_) | StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => false,
    }
}

/// Returns true when a function name is one of the runtime regex builtins.
fn is_regex_builtin_name(name: &str) -> bool {
    matches!(
        php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "preg_match" | "preg_match_all" | "preg_replace" | "preg_replace_callback" | "preg_split"
    )
}

/// Returns true when codegen can emit the runtime callable dispatcher for this program.
///
/// The dispatcher (descriptor invoker / `runtime_callable_cases`) is emitted whenever a
/// dynamic or runtime-string callable must be dispatched by name to a builtin/user/extern
/// entry. Because that case table builds md5/sha1/hash wrappers referencing the
/// `elephc_crypto` staticlib, this scan is a conservative SUPERSET of the emission paths:
/// over-detecting only links the small crate needlessly, while under-detecting would
/// re-introduce the `_elephc_crypto_hash` link failure.
fn program_requires_descriptor_invoker(program: &Program) -> bool {
    body_needs_descriptor_invoker(program)
}

/// Returns true when any statement in a body can trigger descriptor-invoker emission.
fn body_needs_descriptor_invoker(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_needs_descriptor_invoker)
}

/// Returns true when a statement can trigger descriptor-invoker emission.
fn stmt_needs_descriptor_invoker(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr))
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::PropertyAssign { value: expr, .. }
        | StmtKind::PropertyArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. }
        | StmtKind::Include { path: expr, .. } => expr_needs_descriptor_invoker(expr),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_needs_descriptor_invoker(index) || expr_needs_descriptor_invoker(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_needs_descriptor_invoker(target) || expr_needs_descriptor_invoker(value)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_needs_descriptor_invoker(condition)
                || body_needs_descriptor_invoker(then_body)
                || elseif_clauses.iter().any(|(condition, body)| {
                    expr_needs_descriptor_invoker(condition) || body_needs_descriptor_invoker(body)
                })
                || else_body.as_deref().is_some_and(body_needs_descriptor_invoker)
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            body_needs_descriptor_invoker(then_body)
                || else_body.as_deref().is_some_and(body_needs_descriptor_invoker)
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_needs_descriptor_invoker(condition) || body_needs_descriptor_invoker(body)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_needs_descriptor_invoker)
                || condition.as_ref().is_some_and(expr_needs_descriptor_invoker)
                || update.as_deref().is_some_and(stmt_needs_descriptor_invoker)
                || body_needs_descriptor_invoker(body)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_needs_descriptor_invoker(array) || body_needs_descriptor_invoker(body)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_needs_descriptor_invoker(subject)
                || cases.iter().any(|(patterns, body)| {
                    patterns.iter().any(expr_needs_descriptor_invoker)
                        || body_needs_descriptor_invoker(body)
                })
                || default.as_deref().is_some_and(body_needs_descriptor_invoker)
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::FunctionDecl { body, .. } => body_needs_descriptor_invoker(body),
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. } => methods
            .iter()
            .any(|method| body_needs_descriptor_invoker(&method.body)),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_needs_descriptor_invoker(try_body)
                || catches
                    .iter()
                    .any(|catch| body_needs_descriptor_invoker(&catch.body))
                || finally_body
                    .as_deref()
                    .is_some_and(body_needs_descriptor_invoker)
        }
        StmtKind::Return(None)
        | StmtKind::RefAssign { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

/// Returns true when an expression can trigger descriptor-invoker emission.
fn expr_needs_descriptor_invoker(expr: &Expr) -> bool {
    if expr_is_descriptor_invoker_trigger(expr) {
        return true;
    }
    match &expr.kind {
        // A direct dynamic call on an arbitrary callee (e.g. `$callback(...)`) lowers
        // through runtime callable dispatch when the callee resolves to a string name.
        ExprKind::ExprCall { callee, args } => {
            expr_needs_descriptor_invoker(callee) || args.iter().any(expr_needs_descriptor_invoker)
        }
        ExprKind::FunctionCall { args, .. } => args.iter().any(expr_needs_descriptor_invoker),
        ExprKind::BinaryOp { left, right, .. } => {
            expr_needs_descriptor_invoker(left) || expr_needs_descriptor_invoker(right)
        }
        ExprKind::InstanceOf { value, .. } => expr_needs_descriptor_invoker(value),
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::PtrCast { expr, .. }
        | ExprKind::YieldFrom(expr) => expr_needs_descriptor_invoker(expr),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default }
        | ExprKind::Pipe {
            value,
            callable: default,
        } => expr_needs_descriptor_invoker(value) || expr_needs_descriptor_invoker(default),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            body_needs_descriptor_invoker(prelude)
                || expr_needs_descriptor_invoker(target)
                || expr_needs_descriptor_invoker(value)
                || result_target
                    .as_deref()
                    .is_some_and(expr_needs_descriptor_invoker)
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_needs_descriptor_invoker),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| {
                expr_needs_descriptor_invoker(key) || expr_needs_descriptor_invoker(value)
            }),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_needs_descriptor_invoker(subject)
                || arms.iter().any(|(patterns, value)| {
                    patterns.iter().any(expr_needs_descriptor_invoker)
                        || expr_needs_descriptor_invoker(value)
                })
                || default.as_deref().is_some_and(expr_needs_descriptor_invoker)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_needs_descriptor_invoker(array) || expr_needs_descriptor_invoker(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_needs_descriptor_invoker(condition)
                || expr_needs_descriptor_invoker(then_expr)
                || expr_needs_descriptor_invoker(else_expr)
        }
        ExprKind::Closure { body, .. } => body_needs_descriptor_invoker(body),
        ExprKind::NamedArg { value, .. } => expr_needs_descriptor_invoker(value),
        ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_needs_descriptor_invoker),
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => expr_needs_descriptor_invoker(class_name) || args.iter().any(expr_needs_descriptor_invoker),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_needs_descriptor_invoker(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_needs_descriptor_invoker(object) || expr_needs_descriptor_invoker(property)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_needs_descriptor_invoker(object) || args.iter().any(expr_needs_descriptor_invoker)
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => {
            expr_needs_descriptor_invoker(object)
        }
        ExprKind::FirstClassCallable(_) => false,
        ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. } => false,
        ExprKind::BufferNew { len, .. } => expr_needs_descriptor_invoker(len),
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_needs_descriptor_invoker)
                || value.as_deref().is_some_and(expr_needs_descriptor_invoker)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_needs_descriptor_invoker(name_expr)
                || args.iter().any(expr_needs_descriptor_invoker)
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::This
        | ExprKind::MagicConstant(_) => false,
    }
}

/// Returns true when this expression node itself is a descriptor-invoker emission trigger.
///
/// Triggers (conservative superset of the codegen `runtime_callable_cases` paths):
/// - any direct dynamic call `ExprCall` whose callee may be a runtime string name;
/// - `call_user_func`/`call_user_func_array` whose callback is not a statically resolved
///   form (closure / first-class callable / array literal);
/// - `iterator_apply` whose callback is not a statically resolved form;
/// - `preg_replace_callback` whose callback is a runtime (non-literal) string candidate;
/// - `new Fiber($cb)` whose callback is not a statically resolved form.
fn expr_is_descriptor_invoker_trigger(expr: &Expr) -> bool {
    match &expr.kind {
        // `$callback(...)` — a direct call on a variable or complex callee can dispatch a
        // runtime string name through the descriptor case table. `ClosureCall` covers the
        // simple-variable form (`$cb(...)`); `ExprCall` covers complex callee expressions.
        ExprKind::ClosureCall { .. } | ExprKind::ExprCall { .. } => true,
        ExprKind::FunctionCall { name, args } => {
            function_call_needs_descriptor_invoker(name.as_str(), args)
        }
        // `new Fiber($cb)` selects a runtime callable descriptor by name. Unlike the
        // call_user_func family, the Fiber path treats any string-typed callback — including
        // string literals — as runtime dispatch, so this uses the broader predicate.
        ExprKind::NewObject { class_name, args } => {
            is_fiber_class_name(class_name.as_str())
                && args.first().is_some_and(fiber_callback_may_be_runtime_dispatch)
        }
        _ => false,
    }
}

/// Returns true when a dispatcher builtin call can route a runtime callable by name.
///
/// The callback argument position differs by builtin: `call_user_func`/
/// `call_user_func_array` and `array_map` take it first, while the array
/// filter/reduce/walk/sort helpers, `iterator_apply`, and `preg_replace_callback`
/// take it second (after the source array, pattern, or iterator).
fn function_call_needs_descriptor_invoker(name: &str, args: &[Expr]) -> bool {
    let callback = match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "call_user_func" | "call_user_func_array" | "array_map" => args.first(),
        "array_filter" | "array_walk" | "array_reduce" | "usort" | "uasort" | "uksort"
        | "iterator_apply" | "preg_replace_callback" => args.get(1),
        _ => return false,
    };
    callback.is_some_and(callback_arg_may_be_runtime_dispatch)
}

/// Returns true when a callback argument may resolve to a runtime callable dispatch.
///
/// Statically resolved callback forms never go through the builtin string-name case table,
/// so they are excluded: closures, first-class callables, `[obj, "m"]` callable arrays, and
/// constant string-literal names (codegen resolves a literal callback name at compile time;
/// `callback_is_runtime_string` excludes string literals for the same reason). Everything
/// else (variables, concatenations, property reads, ternaries, calls) is conservatively
/// treated as a possible runtime-string dispatch.
fn callback_arg_may_be_runtime_dispatch(arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => callback_arg_may_be_runtime_dispatch(value),
        ExprKind::Closure { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::StringLiteral(_) => false,
        _ => true,
    }
}

/// Returns true when a `new Fiber($cb)` callback may resolve to a runtime callable dispatch.
///
/// The Fiber callable lowering selects a descriptor by runtime string name for any callback
/// whose contextual type is a string, so a string-literal callback (`new Fiber("strlen")`)
/// also uses the case table. Only closures, first-class callables, and callable arrays are
/// excluded because those build descriptors without consulting the string-name table.
fn fiber_callback_may_be_runtime_dispatch(arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => fiber_callback_may_be_runtime_dispatch(value),
        ExprKind::Closure { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_) => false,
        _ => true,
    }
}

/// Returns true when a constructed class name denotes a `Fiber`.
fn is_fiber_class_name(name: &str) -> bool {
    php_symbol_key(name.trim_start_matches('\\')) == "fiber"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a source string and returns the runtime features discovered after name resolution.
    fn features_for(source: &str) -> RuntimeFeatures {
        let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
        let ast = crate::parser::parse(&tokens).expect("parse failed");
        let ast = crate::name_resolver::resolve(ast).expect("name resolve failed");
        runtime_features_for_program(&ast)
    }

    /// Type-checks a source string and returns class-aware runtime features.
    fn checked_features_for(source: &str) -> RuntimeFeatures {
        let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
        let ast = crate::parser::parse(&tokens).expect("parse failed");
        let ast = crate::name_resolver::resolve(ast).expect("name resolve failed");
        let check_result =
            crate::types::check_with_target(&ast, crate::codegen::platform::Target::detect_host())
                .expect("typecheck failed");
        runtime_features_for_program_and_classes(&ast, &check_result.classes)
    }

    /// Verifies ordinary programs do not require the optional regex runtime helpers.
    #[test]
    fn test_runtime_features_omit_regex_for_plain_program() {
        assert_eq!(features_for("<?php echo \"plain\";"), RuntimeFeatures::none());
        assert_eq!(
            checked_features_for("<?php echo \"plain\";"),
            RuntimeFeatures::none()
        );
    }

    /// Verifies direct preg builtin calls enable regex runtime helpers.
    #[test]
    fn test_runtime_features_include_regex_for_preg_call() {
        assert_eq!(
            features_for("<?php echo preg_match(\"/a/\", \"cat\");"),
            RuntimeFeatures { regex: true, ..RuntimeFeatures::none() }
        );
    }

    /// Verifies regex runtime features request PCRE2 libraries for final linking.
    #[test]
    fn test_regex_runtime_features_require_pcre2_libraries() {
        assert_eq!(
            required_libraries_for_runtime_features(RuntimeFeatures {
                regex: true,
                ..RuntimeFeatures::none()
            }),
            vec!["pcre2-posix".to_string(), "pcre2-8".to_string()]
        );
        assert!(required_libraries_for_runtime_features(RuntimeFeatures::none()).is_empty());
    }

    /// Verifies literal callback dispatch to preg builtins enables regex helpers.
    #[test]
    fn test_runtime_features_include_regex_for_call_user_func_literal() {
        assert_eq!(
            features_for("<?php echo call_user_func(\"preg_match\", \"/a/\", \"cat\");"),
            RuntimeFeatures { regex: true, ..RuntimeFeatures::none() }
        );
        assert_eq!(
            features_for("<?php echo call_user_func_array(\"preg_split\", [\"/,/\", \"a,b\"]);"),
            RuntimeFeatures { regex: true, ..RuntimeFeatures::none() }
        );
    }

    /// Verifies first-class callable references to regex builtins enable regex helpers.
    #[test]
    fn test_runtime_features_include_regex_for_first_class_callable() {
        assert_eq!(
            features_for("<?php $cb = preg_replace_callback(...);"),
            RuntimeFeatures { regex: true, ..RuntimeFeatures::none() }
        );
    }

    /// Verifies dynamic class targets enable regex helpers for builtin class method emission.
    #[test]
    fn test_runtime_features_include_regex_for_dynamic_instanceof() {
        assert_eq!(
            features_for("<?php echo $value instanceof $className;"),
            RuntimeFeatures { regex: true, ..RuntimeFeatures::none() }
        );
    }

    /// Verifies RegexIterator usage enables regex helpers for generated SPL methods.
    #[test]
    fn test_runtime_features_include_regex_for_regex_iterator() {
        assert_eq!(
            features_for(
                "<?php $it = new RegexIterator(new ArrayIterator([\"a\"]), \"/a/\");"
            ),
            RuntimeFeatures { regex: true, ..RuntimeFeatures::none() }
        );
    }

    /// Verifies class-aware filesystem emission requests PHAR libraries without regex helpers.
    #[test]
    fn test_runtime_features_include_phar_for_spl_filesystem_class_expansion() {
        assert_eq!(
            checked_features_for("<?php $file = new SplTempFileObject();"),
            RuntimeFeatures {
                phar_archive: true,
                ..RuntimeFeatures::none()
            }
        );
    }

    /// Verifies class-aware PHAR emission requests the PHAR bridge libraries.
    #[test]
    fn test_runtime_features_include_phar_for_phar_class_expansion() {
        assert_eq!(
            checked_features_for("<?php $phar = new Phar(\"archive.phar\");"),
            RuntimeFeatures {
                phar_archive: true,
                ..RuntimeFeatures::none()
            }
        );
    }

    /// Verifies plain programs do not request the dynamic builtin dispatcher.
    #[test]
    fn test_runtime_features_omit_descriptor_invoker_for_plain_program() {
        assert!(!features_for("<?php echo \"hi\";").descriptor_invoker);
    }

    /// Verifies a dynamic `call_user_func()` callback requests the dispatcher.
    #[test]
    fn test_runtime_features_include_descriptor_invoker_for_dynamic_call_user_func() {
        assert!(
            features_for("<?php $cb = \"strlen\"; echo call_user_func($cb, \"hi\");")
                .descriptor_invoker
        );
    }

    /// Verifies a direct dynamic string call requests the dispatcher.
    #[test]
    fn test_runtime_features_include_descriptor_invoker_for_direct_dynamic_call() {
        assert!(
            features_for("<?php $cb = \"strlen\"; echo $cb(\"hi\");").descriptor_invoker
        );
    }

    /// Verifies `new Fiber($cb)` with a dynamic callable requests the dispatcher.
    #[test]
    fn test_runtime_features_include_descriptor_invoker_for_fiber_dynamic_callable() {
        assert!(
            features_for("<?php $cb = \"job\"; $f = new Fiber($cb);").descriptor_invoker
        );
    }

    /// Verifies `iterator_apply()` with a dynamic callback requests the dispatcher.
    #[test]
    fn test_runtime_features_include_descriptor_invoker_for_iterator_apply() {
        assert!(features_for(
            "<?php $cb = \"cb\"; iterator_apply(new ArrayIterator([1]), $cb);"
        )
        .descriptor_invoker);
    }

    /// Verifies `preg_replace_callback()` with a runtime string callback requests the dispatcher.
    #[test]
    fn test_runtime_features_include_descriptor_invoker_for_preg_replace_callback_runtime_string() {
        assert!(features_for(
            "<?php $cb = \"cb\"; echo preg_replace_callback(\"/a/\", $cb, \"a\");"
        )
        .descriptor_invoker);
    }

    /// Verifies the dynamic dispatcher feature requests the crypto staticlib for linking.
    #[test]
    fn test_descriptor_invoker_runtime_features_require_elephc_crypto_library() {
        assert!(required_libraries_for_runtime_features(RuntimeFeatures {
            regex: false,
            phar_archive: false,
            descriptor_invoker: true,
        })
        .iter()
        .any(|lib| lib == "elephc_crypto"));
        assert!(!required_libraries_for_runtime_features(RuntimeFeatures::none())
            .iter()
            .any(|lib| lib == "elephc_crypto"));
    }
}
