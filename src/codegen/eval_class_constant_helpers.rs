//! Purpose:
//! Emits user-assembly helpers that let libelephc-magician read generated/AOT
//! class-like constants and their Reflection metadata from eval fragments.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - Direct `Aot::CONST` reads enforce PHP visibility with the active eval class scope.
//! - Reflection probes intentionally bypass visibility, matching PHP Reflection.
//! - Values are limited to the same scalar and enum-case constant forms emitted by static Reflection metadata.

use std::collections::HashSet;

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::ir::{Function, LocalKind, Module};
use crate::names::{enum_case_symbol, php_symbol_key};
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, Visibility};
use crate::types::{ClassInfo, InterfaceInfo, PhpType};

const EVAL_REFLECTION_MEMBER_FLAG_PUBLIC: u64 = 2;
const EVAL_REFLECTION_MEMBER_FLAG_PROTECTED: u64 = 4;
const EVAL_REFLECTION_MEMBER_FLAG_PRIVATE: u64 = 8;
const EVAL_REFLECTION_MEMBER_FLAG_FINAL: u64 = 16;
const EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE: u64 = 128;

/// Constant slot metadata needed by eval direct reads and Reflection probes.
#[derive(Clone)]
struct EvalClassConstantSlot {
    reflected_class: String,
    declaring_class: String,
    allowed_scopes: Vec<String>,
    constant: String,
    visibility: Visibility,
    is_final: bool,
    is_enum_case: bool,
    value: EvalClassConstantValue,
}

/// Constant value forms the eval bridge can materialize as boxed Mixed cells.
#[derive(Clone)]
enum EvalClassConstantValue {
    Int(i64),
    Bool(bool),
    Float(f64),
    Str(String),
    Null,
    EnumCase { enum_name: String, case_name: String },
}

/// Emits eval class-constant helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_class_constant_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    if !module_uses_eval(module) {
        return;
    }
    let slots = collect_eval_class_constant_slots(module);
    emit_class_constant_get_helper(module, emitter, data, &slots);
    emit_reflection_constant_value_helper(module, emitter, data, &slots);
    emit_reflection_constant_names_helper(module, emitter, data, &slots);
    emit_reflection_constant_flags_helper(module, emitter, data, &slots);
    emit_reflection_constant_declaring_class_helper(module, emitter, data, &slots);
}

/// Returns true when the EIR module contains a function that can call eval.
fn module_uses_eval(module: &Module) -> bool {
    all_module_functions(module).any(function_uses_eval)
}

/// Iterates every EIR function body emitted or inspected by the backend.
fn all_module_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Returns true when a function has hidden eval state locals.
fn function_uses_eval(function: &Function) -> bool {
    function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::EvalContext | LocalKind::EvalScope | LocalKind::EvalGlobalScope
        )
    })
}

/// Collects class-like constants visible to direct eval reads and Reflection APIs.
fn collect_eval_class_constant_slots(module: &Module) -> Vec<EvalClassConstantSlot> {
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, _) in classes {
        collect_reflected_class_constant_slots(module, class_name, &mut slots);
    }

    let mut interfaces = module.interface_infos.iter().collect::<Vec<_>>();
    interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    for (interface_name, _) in interfaces {
        collect_reflected_interface_constant_slots(module, interface_name, &mut slots);
    }

    let mut traits = module.declared_trait_names.clone();
    traits.sort();
    for trait_name in traits {
        collect_reflected_trait_constant_slots(module, &trait_name, &mut slots);
    }
    slots
}

/// Adds constants visible when reflecting or reading one generated/AOT class.
fn collect_reflected_class_constant_slots(
    module: &Module,
    class_name: &str,
    slots: &mut Vec<EvalClassConstantSlot>,
) {
    for constant_name in class_constant_names(module, class_name) {
        let slot = if let Some(slot) = enum_case_slot(module, class_name, &constant_name) {
            Some(slot)
        } else {
            resolve_class_constant_slot(module, class_name, &constant_name)
        };
        if let Some(mut slot) = slot {
            slot.reflected_class = class_name.to_string();
            slots.push(slot);
        }
    }
}

/// Adds constants visible when reflecting or reading one generated/AOT interface.
fn collect_reflected_interface_constant_slots(
    module: &Module,
    interface_name: &str,
    slots: &mut Vec<EvalClassConstantSlot>,
) {
    for constant_name in interface_constant_names(module, interface_name) {
        if let Some(mut slot) = resolve_interface_constant_slot(module, interface_name, &constant_name) {
            slot.reflected_class = interface_name.to_string();
            slots.push(slot);
        }
    }
}

/// Adds constants visible when reflecting or reading one generated/AOT trait.
fn collect_reflected_trait_constant_slots(
    module: &Module,
    trait_name: &str,
    slots: &mut Vec<EvalClassConstantSlot>,
) {
    let Some(constants) = module.declared_trait_constants.get(trait_name) else {
        return;
    };
    let mut names = module
        .declared_trait_constant_names
        .get(trait_name)
        .cloned()
        .unwrap_or_else(|| constants.keys().cloned().collect());
    names.sort();
    for constant_name in names {
        if let Some(mut slot) = trait_constant_slot(module, trait_name, &constant_name) {
            slot.reflected_class = trait_name.to_string();
            slots.push(slot);
        }
    }
}

/// Returns class constant names in the same order as eval-backed ReflectionClass.
fn class_constant_names(module: &Module, class_name: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    if let Some(enum_info) = module.enum_infos.get(class_name) {
        for case in &enum_info.cases {
            push_unique_constant_name(&case.name, &mut names, &mut seen);
        }
    }
    for (declaring_class, class_info) in class_chain(module, class_name) {
        let mut own = class_info.constants.keys().cloned().collect::<Vec<_>>();
        own.sort();
        for constant_name in own {
            if is_private_inherited_class_constant(
                class_name,
                declaring_class,
                class_info,
                &constant_name,
            ) {
                continue;
            }
            push_unique_constant_name(&constant_name, &mut names, &mut seen);
        }
        for interface_name in &class_info.interfaces {
            for constant_name in interface_constant_names(module, interface_name) {
                push_unique_constant_name(&constant_name, &mut names, &mut seen);
            }
        }
    }
    names
}

/// Returns whether a parent private constant is hidden from a reflected child.
fn is_private_inherited_class_constant(
    reflected_class: &str,
    declaring_class: &str,
    declaring_info: &ClassInfo,
    constant_name: &str,
) -> bool {
    php_symbol_key(reflected_class) != php_symbol_key(declaring_class)
        && declaring_info
            .constant_visibilities
            .get(constant_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Private))
}

/// Returns the inheritance chain from reflected class toward its parents.
fn class_chain<'a>(module: &'a Module, class_name: &'a str) -> Vec<(&'a str, &'a ClassInfo)> {
    let mut result = Vec::new();
    let mut current = Some(class_name);
    let mut seen = HashSet::new();
    while let Some(name) = current {
        let Some((resolved_name, info)) = resolve_class(module, name) else {
            break;
        };
        if !seen.insert(php_symbol_key(resolved_name)) {
            break;
        }
        result.push((resolved_name, info));
        current = info.parent.as_deref();
    }
    result
}

/// Returns interface constant names with parent interfaces first.
fn interface_constant_names(module: &Module, interface_name: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    collect_interface_constant_names(module, interface_name, &mut names, &mut seen);
    names
}

/// Recursively collects interface constant names without duplicates.
fn collect_interface_constant_names(
    module: &Module,
    interface_name: &str,
    names: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    let Some((_, interface_info)) = resolve_interface(module, interface_name) else {
        return;
    };
    for parent in &interface_info.parents {
        collect_interface_constant_names(module, parent, names, seen);
    }
    let mut own = interface_info.constants.keys().cloned().collect::<Vec<_>>();
    own.sort();
    for constant_name in own {
        push_unique_constant_name(&constant_name, names, seen);
    }
}

/// Pushes one case-sensitive PHP constant name once.
fn push_unique_constant_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    if seen.insert(name.to_string()) {
        names.push(name.to_string());
    }
}

/// Resolves one class constant against class parents and implemented interfaces.
fn resolve_class_constant_slot(
    module: &Module,
    class_name: &str,
    constant_name: &str,
) -> Option<EvalClassConstantSlot> {
    let (resolved_name, class_info) = resolve_class(module, class_name)?;
    if let Some(value_expr) = class_info.constants.get(constant_name) {
        return class_constant_slot(module, resolved_name, class_info, constant_name, value_expr);
    }
    if let Some(parent) = class_info.parent.as_deref() {
        if let Some(slot) = resolve_class_constant_slot(module, parent, constant_name) {
            return Some(slot);
        }
    }
    for interface_name in &class_info.interfaces {
        if let Some(slot) = resolve_interface_constant_slot(module, interface_name, constant_name) {
            return Some(slot);
        }
    }
    None
}

/// Builds metadata for one class-declared constant.
fn class_constant_slot(
    module: &Module,
    declaring_class: &str,
    class_info: &ClassInfo,
    constant_name: &str,
    value_expr: &Expr,
) -> Option<EvalClassConstantSlot> {
    let value = eval_class_constant_value(module, declaring_class, Some(class_info), value_expr, 0)?;
    let visibility = class_info
        .constant_visibilities
        .get(constant_name)
        .cloned()
        .unwrap_or(Visibility::Public);
    Some(EvalClassConstantSlot {
        reflected_class: declaring_class.to_string(),
        declaring_class: declaring_class.to_string(),
        allowed_scopes: visibility_scope_names(module, declaring_class, &visibility),
        constant: constant_name.to_string(),
        visibility,
        is_final: class_info.final_constants.contains(constant_name),
        is_enum_case: false,
        value,
    })
}

/// Resolves one interface constant and preserves its original declaring interface.
fn resolve_interface_constant_slot(
    module: &Module,
    interface_name: &str,
    constant_name: &str,
) -> Option<EvalClassConstantSlot> {
    let (resolved_name, interface_info) = resolve_interface(module, interface_name)?;
    let value_expr = interface_info.constants.get(constant_name)?;
    let declaring_interface =
        interface_constant_declaring_interface(interface_info, resolved_name, constant_name);
    let value = eval_class_constant_value(module, declaring_interface, None, value_expr, 0)?;
    Some(EvalClassConstantSlot {
        reflected_class: resolved_name.to_string(),
        declaring_class: declaring_interface.to_string(),
        allowed_scopes: Vec::new(),
        constant: constant_name.to_string(),
        visibility: Visibility::Public,
        is_final: module
            .interface_infos
            .get(declaring_interface)
            .is_some_and(|info| info.final_constants.contains(constant_name)),
        is_enum_case: false,
        value,
    })
}

/// Builds metadata for one trait-declared constant.
fn trait_constant_slot(
    module: &Module,
    trait_name: &str,
    constant_name: &str,
) -> Option<EvalClassConstantSlot> {
    let value_expr = module
        .declared_trait_constants
        .get(trait_name)
        .and_then(|constants| constants.get(constant_name))?;
    let value = eval_class_constant_value(module, trait_name, None, value_expr, 0)?;
    let visibility = module
        .declared_trait_constant_visibilities
        .get(trait_name)
        .and_then(|constants| constants.get(constant_name))
        .cloned()
        .unwrap_or(Visibility::Public);
    Some(EvalClassConstantSlot {
        reflected_class: trait_name.to_string(),
        declaring_class: trait_name.to_string(),
        allowed_scopes: visibility_scope_names(module, trait_name, &visibility),
        constant: constant_name.to_string(),
        visibility,
        is_final: module
            .declared_trait_final_constants
            .get(trait_name)
            .is_some_and(|constants| constants.contains(constant_name)),
        is_enum_case: false,
        value,
    })
}

/// Builds metadata for one enum case exposed as a class constant.
fn enum_case_slot(
    module: &Module,
    enum_name: &str,
    case_name: &str,
) -> Option<EvalClassConstantSlot> {
    let enum_info = module.enum_infos.get(enum_name)?;
    enum_info.cases.iter().any(|case| case.name == case_name).then(|| {
        EvalClassConstantSlot {
            reflected_class: enum_name.to_string(),
            declaring_class: enum_name.to_string(),
            allowed_scopes: Vec::new(),
            constant: case_name.to_string(),
            visibility: Visibility::Public,
            is_final: false,
            is_enum_case: true,
            value: EvalClassConstantValue::EnumCase {
                enum_name: enum_name.to_string(),
                case_name: case_name.to_string(),
            },
        }
    })
}

/// Returns the interface that originally declared one inherited constant.
fn interface_constant_declaring_interface<'a>(
    info: &'a InterfaceInfo,
    fallback_interface: &'a str,
    constant_name: &str,
) -> &'a str {
    info.constant_declaring_interfaces
        .get(constant_name)
        .map(String::as_str)
        .unwrap_or(fallback_interface)
}

/// Evaluates one supported AOT class-like constant expression.
fn eval_class_constant_value(
    module: &Module,
    current_class: &str,
    current_info: Option<&ClassInfo>,
    expr: &Expr,
    depth: usize,
) -> Option<EvalClassConstantValue> {
    if depth > 16 {
        return None;
    }
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(EvalClassConstantValue::Int(*value)),
        ExprKind::BoolLiteral(value) => Some(EvalClassConstantValue::Bool(*value)),
        ExprKind::FloatLiteral(value) => Some(EvalClassConstantValue::Float(*value)),
        ExprKind::StringLiteral(value) => Some(EvalClassConstantValue::Str(value.clone())),
        ExprKind::Null => Some(EvalClassConstantValue::Null),
        ExprKind::Negate(inner) => {
            match eval_class_constant_value(module, current_class, current_info, inner, depth + 1)? {
                EvalClassConstantValue::Int(value) => {
                    value.checked_neg().map(EvalClassConstantValue::Int)
                }
                EvalClassConstantValue::Float(value) => Some(EvalClassConstantValue::Float(-value)),
                _ => None,
            }
        }
        ExprKind::BinaryOp { left, op, right } => {
            eval_binary_class_constant_value(module, current_class, current_info, left, op, right, depth + 1)
        }
        ExprKind::ClassConstant { receiver } => {
            let class_name = static_receiver_name(current_class, current_info, receiver)?;
            Some(EvalClassConstantValue::Str(class_name))
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let class_name = static_receiver_name(current_class, current_info, receiver)?;
            eval_scoped_class_constant_value(module, &class_name, name, depth + 1)
        }
        _ => None,
    }
}

/// Evaluates one supported binary class-constant expression.
fn eval_binary_class_constant_value(
    module: &Module,
    current_class: &str,
    current_info: Option<&ClassInfo>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    depth: usize,
) -> Option<EvalClassConstantValue> {
    let left = eval_class_constant_value(module, current_class, current_info, left, depth)?;
    let right = eval_class_constant_value(module, current_class, current_info, right, depth)?;
    match (&left, op, &right) {
        (
            EvalClassConstantValue::Int(left),
            BinOp::Add,
            EvalClassConstantValue::Int(right),
        ) => {
            (*left).checked_add(*right).map(EvalClassConstantValue::Int)
        }
        (
            EvalClassConstantValue::Int(left),
            BinOp::Sub,
            EvalClassConstantValue::Int(right),
        ) => {
            (*left).checked_sub(*right).map(EvalClassConstantValue::Int)
        }
        (
            EvalClassConstantValue::Int(left),
            BinOp::Mul,
            EvalClassConstantValue::Int(right),
        ) => {
            (*left).checked_mul(*right).map(EvalClassConstantValue::Int)
        }
        (
            EvalClassConstantValue::Int(left),
            BinOp::Mod,
            EvalClassConstantValue::Int(right),
        ) => {
            (*left).checked_rem(*right).map(EvalClassConstantValue::Int)
        }
        (
            EvalClassConstantValue::Int(left),
            BinOp::Pow,
            EvalClassConstantValue::Int(right),
        ) if *right >= 0 =>
        {
            let exponent = u32::try_from(*right).ok()?;
            (*left).checked_pow(exponent).map(EvalClassConstantValue::Int)
        }
        (
            EvalClassConstantValue::Str(left),
            BinOp::Concat,
            EvalClassConstantValue::Str(right),
        ) => Some(EvalClassConstantValue::Str(format!("{}{}", left, right))),
        (
            left,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow,
            right,
        ) => eval_float_binary_class_constant_value(left, op, right),
        _ => None,
    }
}

/// Evaluates a numeric class-constant expression that must produce a float.
fn eval_float_binary_class_constant_value(
    left: &EvalClassConstantValue,
    op: &BinOp,
    right: &EvalClassConstantValue,
) -> Option<EvalClassConstantValue> {
    let left = eval_class_constant_value_as_float(left)?;
    let right = eval_class_constant_value_as_float(right)?;
    let value = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div if right != 0.0 => left / right,
        BinOp::Pow => left.powf(right),
        _ => return None,
    };
    Some(EvalClassConstantValue::Float(value))
}

/// Returns the float representation of numeric eval class-constant metadata.
fn eval_class_constant_value_as_float(value: &EvalClassConstantValue) -> Option<f64> {
    match value {
        EvalClassConstantValue::Int(value) => Some(*value as f64),
        EvalClassConstantValue::Float(value) => Some(*value),
        _ => None,
    }
}

/// Evaluates a scoped class-like constant expression.
fn eval_scoped_class_constant_value(
    module: &Module,
    class_name: &str,
    constant_name: &str,
    depth: usize,
) -> Option<EvalClassConstantValue> {
    if let Some((resolved_name, info)) = resolve_class(module, class_name) {
        if let Some(value_expr) = info.constants.get(constant_name) {
            return eval_class_constant_value(module, resolved_name, Some(info), value_expr, depth);
        }
        if let Some(parent) = info.parent.as_deref() {
            if let Some(value) = eval_scoped_class_constant_value(module, parent, constant_name, depth) {
                return Some(value);
            }
        }
        for interface_name in &info.interfaces {
            if let Some(value) = eval_scoped_class_constant_value(module, interface_name, constant_name, depth) {
                return Some(value);
            }
        }
    }
    if let Some((resolved_name, info)) = resolve_interface(module, class_name) {
        if let Some(value_expr) = info.constants.get(constant_name) {
            return eval_class_constant_value(module, resolved_name, None, value_expr, depth);
        }
    }
    if let Some(value_expr) = module
        .declared_trait_constants
        .get(class_name)
        .and_then(|constants| constants.get(constant_name))
    {
        return eval_class_constant_value(module, class_name, None, value_expr, depth);
    }
    if module
        .enum_infos
        .get(class_name)
        .is_some_and(|info| info.cases.iter().any(|case| case.name == constant_name))
    {
        return Some(EvalClassConstantValue::EnumCase {
            enum_name: class_name.to_string(),
            case_name: constant_name.to_string(),
        });
    }
    None
}

/// Resolves `self`, `static`, `parent`, or named receivers in constant expressions.
fn static_receiver_name(
    current_class: &str,
    current_info: Option<&ClassInfo>,
    receiver: &StaticReceiver,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => Some(current_class.to_string()),
        StaticReceiver::Parent => current_info.and_then(|info| info.parent.clone()),
    }
}

/// Looks up class metadata by PHP-style case-insensitive name.
fn resolve_class<'a>(module: &'a Module, class_name: &str) -> Option<(&'a str, &'a ClassInfo)> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(name, info)| (name.as_str(), info))
}

/// Looks up interface metadata by PHP-style case-insensitive name.
fn resolve_interface<'a>(
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

/// Emits `__elephc_eval_value_class_constant_get(class, constant, scope) -> Mixed*`.
fn emit_class_constant_get_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user class constant get ---");
    label_c_global(module, emitter, "__elephc_eval_value_class_constant_get");
    match module.target.arch {
        Arch::AArch64 => emit_value_helper_aarch64(module, emitter, data, slots, ValueHelperMode::DirectGet),
        Arch::X86_64 => emit_value_helper_x86_64(module, emitter, data, slots, ValueHelperMode::DirectGet),
    }
}

/// Emits `__elephc_eval_reflection_constant_value(class, constant) -> Mixed*`.
fn emit_reflection_constant_value_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: reflection class constant value ---");
    label_c_global(module, emitter, "__elephc_eval_reflection_constant_value");
    match module.target.arch {
        Arch::AArch64 => emit_value_helper_aarch64(module, emitter, data, slots, ValueHelperMode::ReflectionValue),
        Arch::X86_64 => emit_value_helper_x86_64(module, emitter, data, slots, ValueHelperMode::ReflectionValue),
    }
}

/// Distinguishes direct class-constant reads from Reflection value probes.
#[derive(Clone, Copy)]
enum ValueHelperMode {
    DirectGet,
    ReflectionValue,
}

impl ValueHelperMode {
    /// Returns the shared label suffix for this value helper mode.
    const fn suffix(self) -> &'static str {
        match self {
            Self::DirectGet => "get",
            Self::ReflectionValue => "reflection_value",
        }
    }

    /// Returns whether this helper must enforce constant visibility.
    const fn checks_visibility(self) -> bool {
        matches!(self, Self::DirectGet)
    }
}

/// Emits an ARM64 class-constant value helper body.
fn emit_value_helper_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
    mode: ValueHelperMode,
) {
    let done_label = format!("__elephc_eval_class_constant_{}_done", mode.suffix());
    emitter.instruction("sub sp, sp, #64");                                     // reserve helper frame for class, constant, scope, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested constant-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested constant-name length
    if mode.checks_visibility() {
        emitter.instruction("str x4, [sp, #32]");                               // save the active eval class-scope pointer
        emitter.instruction("str x5, [sp, #40]");                               // save the active eval class-scope length
    }
    emit_aarch64_constant_value_dispatch(module, emitter, data, slots, mode);
    emitter.instruction("mov x0, xzr");                                         // report bridge miss with a null pointer
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after a miss
    emit_aarch64_value_slot_bodies(module, emitter, data, slots, mode, &done_label);
    emitter.label(&done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed class constant value to Rust
}

/// Emits an x86_64 class-constant value helper body.
fn emit_value_helper_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
    mode: ValueHelperMode,
) {
    let done_label = format!("__elephc_eval_class_constant_{}_done_x", mode.suffix());
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for class, constant, and scope slices
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested constant-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested constant-name length
    if mode.checks_visibility() {
        emitter.instruction("mov QWORD PTR [rbp - 40], r8");                    // save the active eval class-scope pointer
        emitter.instruction("mov QWORD PTR [rbp - 48], r9");                    // save the active eval class-scope length
    }
    emit_x86_64_constant_value_dispatch(module, emitter, data, slots, mode);
    emitter.instruction("xor eax, eax");                                        // report bridge miss with a null pointer
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after a miss
    emit_x86_64_value_slot_bodies(module, emitter, data, slots, mode, &done_label);
    emitter.label(&done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed class constant value to Rust
}

/// Emits ARM64 class-name and constant-name dispatch for value helpers.
fn emit_aarch64_constant_value_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
    mode: ValueHelperMode,
) {
    for slot in slots {
        let next_label = slot_miss_label(module, slot, mode.suffix());
        emit_aarch64_class_name_compare(emitter, data, &slot.reflected_class, &next_label);
        emit_aarch64_constant_name_compare(module, emitter, data, slot, mode, &next_label);
        emitter.label(&next_label);
    }
}

/// Emits x86_64 class-name and constant-name dispatch for value helpers.
fn emit_x86_64_constant_value_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
    mode: ValueHelperMode,
) {
    for slot in slots {
        let next_label = slot_miss_label(module, slot, mode.suffix());
        emit_x86_64_class_name_compare(emitter, data, &slot.reflected_class, &next_label);
        emit_x86_64_constant_name_compare(module, emitter, data, slot, mode, &next_label);
        emitter.label(&next_label);
    }
}

/// Emits one ARM64 case-insensitive class-name comparison.
fn emit_aarch64_class_name_compare(
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    next_label: &str,
) {
    let (label, len) = data.add_string(class_name.as_bytes());
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested class-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction(&format!("cbnz x0, {}", next_label));                   // continue dispatch when class names differ
}

/// Emits one x86_64 case-insensitive class-name comparison.
fn emit_x86_64_class_name_compare(
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    next_label: &str,
) {
    let (label, len) = data.add_string(class_name.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload requested class-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the class names matched
    emitter.instruction(&format!("jne {}", next_label));                        // continue dispatch when class names differ
}

/// Emits one ARM64 case-sensitive constant-name comparison.
fn emit_aarch64_constant_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    mode: ValueHelperMode,
    next_label: &str,
) {
    let (label, len) = data.add_string(slot.constant.as_bytes());
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload requested constant-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload requested constant-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_str_eq");                                      // compare constant names with PHP case-sensitive rules
    let target_label = slot_body_label(module, slot, mode.suffix());
    if !mode.checks_visibility() || matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("cbnz x0, {}", target_label));             // dispatch to the constant value body when names match
        return;
    }
    emitter.instruction(&format!("cbz x0, {}", next_label));                    // continue dispatch when constant names differ
    emit_aarch64_constant_scope_check(emitter, data, slot, &target_label, next_label);
}

/// Emits one x86_64 case-sensitive constant-name comparison.
fn emit_x86_64_constant_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    mode: ValueHelperMode,
    next_label: &str,
) {
    let (label, len) = data.add_string(slot.constant.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload requested constant-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload requested constant-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_str_eq");                                    // compare constant names with PHP case-sensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the constant names matched
    let target_label = slot_body_label(module, slot, mode.suffix());
    if !mode.checks_visibility() || matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("jne {}", target_label));                  // dispatch to the constant value body when names match
        return;
    }
    emitter.instruction(&format!("je {}", next_label));                         // continue dispatch when constant names differ
    emit_x86_64_constant_scope_check(emitter, data, slot, &target_label, next_label);
}

/// Emits ARM64 visibility checks for a protected/private constant bridge hit.
fn emit_aarch64_constant_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    target_label: &str,
    next_label: &str,
) {
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the active eval class-scope pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload the active eval class-scope length
    emitter.instruction(&format!("cbz x1, {}", next_label));                    // reject scoped constants outside a class scope
    for scope_name in &slot.allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction("ldr x1, [sp, #32]");                               // reload the active eval class-scope pointer
        emitter.instruction("ldr x2, [sp, #40]");                               // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "x3", &label);
        abi::emit_load_int_immediate(emitter, "x4", len as i64);
        emitter.instruction("bl __rt_strcasecmp");                              // compare current eval scope with an allowed class
        emitter.instruction(&format!("cbz x0, {}", target_label));              // dispatch when scoped visibility is satisfied
    }
    emitter.instruction(&format!("b {}", next_label));                          // continue dispatch after a visibility miss
}

/// Emits x86_64 visibility checks for a protected/private constant bridge hit.
fn emit_x86_64_constant_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    target_label: &str,
    next_label: &str,
) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the active eval class-scope pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload the active eval class-scope length
    emitter.instruction("test rdi, rdi");                                       // check whether eval is executing inside a class scope
    emitter.instruction(&format!("jz {}", next_label));                         // reject scoped constants outside a class scope
    for scope_name in &slot.allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                   // reload the active eval class-scope pointer
        emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                   // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "rdx", &label);
        abi::emit_load_int_immediate(emitter, "rcx", len as i64);
        emitter.instruction("call __rt_strcasecmp");                            // compare current eval scope with an allowed class
        emitter.instruction("test rax, rax");                                   // check whether the current scope matched
        emitter.instruction(&format!("je {}", target_label));                   // dispatch when scoped visibility is satisfied
    }
    emitter.instruction(&format!("jmp {}", next_label));                        // continue dispatch after a visibility miss
}

/// Emits ARM64 value bodies for all class-constant slots.
fn emit_aarch64_value_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
    mode: ValueHelperMode,
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, mode.suffix()));
        emit_aarch64_constant_value(emitter, data, &slot.value);
        emitter.instruction(&format!("b {}", done_label));                      // return after boxing the constant value
    }
}

/// Emits x86_64 value bodies for all class-constant slots.
fn emit_x86_64_value_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
    mode: ValueHelperMode,
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, mode.suffix()));
        emit_x86_64_constant_value(emitter, data, &slot.value);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after boxing the constant value
    }
}

/// Emits one ARM64 boxed Mixed value for a supported class constant.
fn emit_aarch64_constant_value(
    emitter: &mut Emitter,
    data: &mut DataSection,
    value: &EvalClassConstantValue,
) {
    match value {
        EvalClassConstantValue::Int(value) => {
            abi::emit_load_int_immediate(emitter, "x0", *value);
            emit_box_current_value_as_mixed(emitter, &PhpType::Int);
        }
        EvalClassConstantValue::Bool(value) => {
            abi::emit_load_int_immediate(emitter, "x0", i64::from(*value));
            emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
        }
        EvalClassConstantValue::Float(value) => {
            let label = data.add_float(*value);
            abi::emit_symbol_address(emitter, "x9", &label);
            emitter.instruction("ldr d0, [x9]");                                // load the float constant through the data-section symbol
            emit_box_current_value_as_mixed(emitter, &PhpType::Float);
        }
        EvalClassConstantValue::Str(value) => {
            let (label, len) = data.add_string(value.as_bytes());
            abi::emit_symbol_address(emitter, "x1", &label);
            abi::emit_load_int_immediate(emitter, "x2", len as i64);
            emit_box_current_value_as_mixed(emitter, &PhpType::Str);
        }
        EvalClassConstantValue::Null => {
            abi::emit_load_int_immediate(emitter, "x0", 0x7fff_ffff_ffff_fffe);
            emit_box_current_value_as_mixed(emitter, &PhpType::Void);
        }
        EvalClassConstantValue::EnumCase {
            enum_name,
            case_name,
        } => {
            let case_label = enum_case_symbol(enum_name, case_name);
            abi::emit_load_symbol_to_reg(emitter, "x0", &case_label, 0);
            emit_box_current_value_as_mixed(emitter, &PhpType::Object(enum_name.clone()));
        }
    }
}

/// Emits one x86_64 boxed Mixed value for a supported class constant.
fn emit_x86_64_constant_value(
    emitter: &mut Emitter,
    data: &mut DataSection,
    value: &EvalClassConstantValue,
) {
    match value {
        EvalClassConstantValue::Int(value) => {
            abi::emit_load_int_immediate(emitter, "rax", *value);
            emit_box_current_value_as_mixed(emitter, &PhpType::Int);
        }
        EvalClassConstantValue::Bool(value) => {
            abi::emit_load_int_immediate(emitter, "rax", i64::from(*value));
            emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
        }
        EvalClassConstantValue::Float(value) => {
            let label = data.add_float(*value);
            abi::emit_symbol_address(emitter, "r10", &label);
            emitter.instruction("movsd xmm0, QWORD PTR [r10]");                 // load the float constant through the data-section symbol
            emit_box_current_value_as_mixed(emitter, &PhpType::Float);
        }
        EvalClassConstantValue::Str(value) => {
            let (label, len) = data.add_string(value.as_bytes());
            abi::emit_symbol_address(emitter, "rax", &label);
            abi::emit_load_int_immediate(emitter, "rdx", len as i64);
            emit_box_current_value_as_mixed(emitter, &PhpType::Str);
        }
        EvalClassConstantValue::Null => {
            abi::emit_load_int_immediate(emitter, "rax", 0x7fff_ffff_ffff_fffe);
            emit_box_current_value_as_mixed(emitter, &PhpType::Void);
        }
        EvalClassConstantValue::EnumCase {
            enum_name,
            case_name,
        } => {
            let case_label = enum_case_symbol(enum_name, case_name);
            abi::emit_load_symbol_to_reg(emitter, "rax", &case_label, 0);
            emit_box_current_value_as_mixed(emitter, &PhpType::Object(enum_name.clone()));
        }
    }
}

/// Emits `__elephc_eval_reflection_constant_names(class) -> string-array Mixed*`.
fn emit_reflection_constant_names_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: reflection class constant names ---");
    label_c_global(module, emitter, "__elephc_eval_reflection_constant_names");
    match module.target.arch {
        Arch::AArch64 => emit_reflection_constant_names_aarch64(emitter, data, slots),
        Arch::X86_64 => emit_reflection_constant_names_x86_64(emitter, data, slots),
    }
}

/// Emits the ARM64 Reflection constant-name array helper.
fn emit_reflection_constant_names_aarch64(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    let done_label = "__elephc_eval_reflection_constant_names_done";
    let fail_label = "__elephc_eval_reflection_constant_names_fail";
    let array_new = emitter
        .target
        .extern_symbol("__elephc_eval_value_string_array_new");
    emitter.instruction("sub sp, sp, #64");                                     // reserve helper frame for class slice, array, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    abi::emit_load_int_immediate(emitter, "x0", slots.len() as i64);
    emitter.instruction(&format!("bl {}", array_new));                          // allocate the boxed string-array result
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // fail if the runtime could not allocate the result array
    emitter.instruction("str x0, [sp, #16]");                                   // save the accumulated boxed string array
    for (index, slot) in slots.iter().enumerate() {
        emit_aarch64_constant_name_push(emitter, data, slot, index, fail_label);
    }
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the accumulated boxed string array
    emitter.instruction(&format!("b {}", done_label));                          // skip null failure after a successful scan
    emitter.label(fail_label);
    emitter.instruction("mov x0, xzr");                                         // return null when allocation or append fails
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed constant-name array to Rust
}

/// Emits the x86_64 Reflection constant-name array helper.
fn emit_reflection_constant_names_x86_64(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    let done_label = "__elephc_eval_reflection_constant_names_done_x";
    let fail_label = "__elephc_eval_reflection_constant_names_fail_x";
    let array_new = emitter
        .target
        .extern_symbol("__elephc_eval_value_string_array_new");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for class slice and result array
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    abi::emit_load_int_immediate(emitter, "rdi", slots.len() as i64);
    emitter.instruction(&format!("call {}", array_new));                        // allocate the boxed string-array result
    emitter.instruction("test rax, rax");                                       // check whether allocation returned a boxed array
    emitter.instruction(&format!("jz {}", fail_label));                         // fail if the runtime could not allocate the result array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the accumulated boxed string array
    for (index, slot) in slots.iter().enumerate() {
        emit_x86_64_constant_name_push(emitter, data, slot, index, fail_label);
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the accumulated boxed string array
    emitter.instruction(&format!("jmp {}", done_label));                        // skip null failure after a successful scan
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // return null when allocation or append fails
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed constant-name array to Rust
}

/// Emits one ARM64 conditional append for a reflected constant name.
fn emit_aarch64_constant_name_push(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    index: usize,
    fail_label: &str,
) {
    let skip_label = format!("__elephc_eval_reflection_constant_names_skip_{}", index);
    let push_symbol = emitter
        .target
        .extern_symbol("__elephc_eval_value_string_array_push");
    emit_aarch64_class_name_compare(emitter, data, &slot.reflected_class, &skip_label);
    let (label, len) = data.add_string(slot.constant.as_bytes());
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the boxed result string array
    abi::emit_symbol_address(emitter, "x1", &label);
    abi::emit_load_int_immediate(emitter, "x2", len as i64);
    emitter.instruction(&format!("bl {}", push_symbol));                        // append the matched constant name to the result array
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // fail if appending returned a null array pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the updated boxed string array
    emitter.label(&skip_label);
}

/// Emits one x86_64 conditional append for a reflected constant name.
fn emit_x86_64_constant_name_push(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    index: usize,
    fail_label: &str,
) {
    let skip_label = format!("__elephc_eval_reflection_constant_names_skip_{}_x", index);
    let push_symbol = emitter
        .target
        .extern_symbol("__elephc_eval_value_string_array_push");
    emit_x86_64_class_name_compare(emitter, data, &slot.reflected_class, &skip_label);
    let (label, len) = data.add_string(slot.constant.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the boxed result string array
    abi::emit_symbol_address(emitter, "rsi", &label);
    abi::emit_load_int_immediate(emitter, "rdx", len as i64);
    emitter.instruction(&format!("call {}", push_symbol));                      // append the matched constant name to the result array
    emitter.instruction("test rax, rax");                                       // check whether append returned an updated array
    emitter.instruction(&format!("jz {}", fail_label));                         // fail if appending returned a null array pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the updated boxed string array
    emitter.label(&skip_label);
}

/// Emits `__elephc_eval_reflection_constant_flags(class, constant) -> flags`.
fn emit_reflection_constant_flags_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: reflection class constant flags ---");
    label_c_global(module, emitter, "__elephc_eval_reflection_constant_flags");
    match module.target.arch {
        Arch::AArch64 => emit_reflection_constant_flags_aarch64(module, emitter, data, slots),
        Arch::X86_64 => emit_reflection_constant_flags_x86_64(module, emitter, data, slots),
    }
}

/// Emits the ARM64 Reflection constant flags helper.
fn emit_reflection_constant_flags_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    let done_label = "__elephc_eval_reflection_constant_flags_done";
    emitter.instruction("sub sp, sp, #48");                                     // reserve helper frame for class/constant slices and fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested constant-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested constant-name length
    for slot in slots {
        let next_label = slot_miss_label(module, slot, "flags");
        emit_aarch64_class_name_compare(emitter, data, &slot.reflected_class, &next_label);
        emit_aarch64_flags_constant_name_compare(module, emitter, data, slot, &next_label);
        emitter.label(&next_label);
    }
    emitter.instruction("mov x0, #0");                                          // return zero when no constant metadata matched
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the matched flags, or zero, to Rust
}

/// Emits the x86_64 Reflection constant flags helper.
fn emit_reflection_constant_flags_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    let done_label = "__elephc_eval_reflection_constant_flags_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for class and constant slices
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested constant-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested constant-name length
    for slot in slots {
        let next_label = slot_miss_label(module, slot, "flags_x");
        emit_x86_64_class_name_compare(emitter, data, &slot.reflected_class, &next_label);
        emit_x86_64_flags_constant_name_compare(module, emitter, data, slot, &next_label);
        emitter.label(&next_label);
    }
    emitter.instruction("xor eax, eax");                                        // return zero when no constant metadata matched
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the matched flags, or zero, to Rust
}

/// Emits an ARM64 constant-name comparison that returns flags on a match.
fn emit_aarch64_flags_constant_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    next_label: &str,
) {
    let done_label = "__elephc_eval_reflection_constant_flags_done";
    let (label, len) = data.add_string(slot.constant.as_bytes());
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload requested constant-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload requested constant-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_str_eq");                                      // compare constant names with PHP case-sensitive rules
    emitter.instruction(&format!("cbz x0, {}", next_label));                    // continue dispatch when constant names differ
    abi::emit_load_int_immediate(emitter, "x0", slot_flags(slot) as i64);
    emitter.instruction(&format!("b {}", done_label));                          // return the matched constant flags
    let _ = module;
}

/// Emits an x86_64 constant-name comparison that returns flags on a match.
fn emit_x86_64_flags_constant_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    next_label: &str,
) {
    let done_label = "__elephc_eval_reflection_constant_flags_done_x";
    let (label, len) = data.add_string(slot.constant.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload requested constant-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload requested constant-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_str_eq");                                    // compare constant names with PHP case-sensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the constant names matched
    emitter.instruction(&format!("je {}", next_label));                         // continue dispatch when constant names differ
    abi::emit_load_int_immediate(emitter, "rax", slot_flags(slot) as i64);
    emitter.instruction(&format!("jmp {}", done_label));                        // return the matched constant flags
    let _ = module;
}

/// Emits `__elephc_eval_reflection_constant_declaring_class(class, constant) -> string Mixed*`.
fn emit_reflection_constant_declaring_class_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: reflection class constant declaring class ---");
    label_c_global(
        module,
        emitter,
        "__elephc_eval_reflection_constant_declaring_class",
    );
    match module.target.arch {
        Arch::AArch64 => emit_reflection_constant_declaring_class_aarch64(module, emitter, data, slots),
        Arch::X86_64 => emit_reflection_constant_declaring_class_x86_64(module, emitter, data, slots),
    }
}

/// Emits the ARM64 Reflection constant declaring-class helper.
fn emit_reflection_constant_declaring_class_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    let done_label = "__elephc_eval_reflection_constant_declaring_class_done";
    emitter.instruction("sub sp, sp, #48");                                     // reserve helper frame for class/constant slices and fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested constant-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested constant-name length
    for slot in slots {
        let next_label = slot_miss_label(module, slot, "declaring");
        emit_aarch64_class_name_compare(emitter, data, &slot.reflected_class, &next_label);
        emit_aarch64_declaring_constant_name_compare(emitter, data, slot, &next_label, done_label);
        emitter.label(&next_label);
    }
    emitter.instruction("mov x0, xzr");                                         // return null when no constant metadata matched
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the declaring class string, or null, to Rust
}

/// Emits the x86_64 Reflection constant declaring-class helper.
fn emit_reflection_constant_declaring_class_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalClassConstantSlot],
) {
    let done_label = "__elephc_eval_reflection_constant_declaring_class_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for class and constant slices
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested constant-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested constant-name length
    for slot in slots {
        let next_label = slot_miss_label(module, slot, "declaring_x");
        emit_x86_64_class_name_compare(emitter, data, &slot.reflected_class, &next_label);
        emit_x86_64_declaring_constant_name_compare(emitter, data, slot, &next_label, done_label);
        emitter.label(&next_label);
    }
    emitter.instruction("xor eax, eax");                                        // return null when no constant metadata matched
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the declaring class string, or null, to Rust
}

/// Emits an ARM64 constant-name comparison that returns declaring class on a match.
fn emit_aarch64_declaring_constant_name_compare(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    next_label: &str,
    done_label: &str,
) {
    let (constant_label, constant_len) = data.add_string(slot.constant.as_bytes());
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload requested constant-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload requested constant-name length
    abi::emit_symbol_address(emitter, "x3", &constant_label);
    abi::emit_load_int_immediate(emitter, "x4", constant_len as i64);
    emitter.instruction("bl __rt_str_eq");                                      // compare constant names with PHP case-sensitive rules
    emitter.instruction(&format!("cbz x0, {}", next_label));                    // continue dispatch when constant names differ
    let (class_label, class_len) = data.add_string(slot.declaring_class.as_bytes());
    abi::emit_symbol_address(emitter, "x1", &class_label);
    abi::emit_load_int_immediate(emitter, "x2", class_len as i64);
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // box the declaring class name for Rust
    emitter.instruction(&format!("b {}", done_label));                          // return the matched declaring class name
}

/// Emits an x86_64 constant-name comparison that returns declaring class on a match.
fn emit_x86_64_declaring_constant_name_compare(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalClassConstantSlot,
    next_label: &str,
    done_label: &str,
) {
    let (constant_label, constant_len) = data.add_string(slot.constant.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload requested constant-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload requested constant-name length
    abi::emit_symbol_address(emitter, "rdx", &constant_label);
    abi::emit_load_int_immediate(emitter, "rcx", constant_len as i64);
    emitter.instruction("call __rt_str_eq");                                    // compare constant names with PHP case-sensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the constant names matched
    emitter.instruction(&format!("je {}", next_label));                         // continue dispatch when constant names differ
    let (class_label, class_len) = data.add_string(slot.declaring_class.as_bytes());
    abi::emit_symbol_address(emitter, "rdi", &class_label);
    abi::emit_load_int_immediate(emitter, "rsi", class_len as i64);
    abi::emit_load_int_immediate(emitter, "rax", 1);
    emitter.instruction("call __rt_mixed_from_value");                          // box the declaring class name for Rust
    emitter.instruction(&format!("jmp {}", done_label));                        // return the matched declaring class name
}

/// Returns ReflectionClassConstant-style member flags for one slot.
fn slot_flags(slot: &EvalClassConstantSlot) -> u64 {
    let mut flags = match slot.visibility {
        Visibility::Public => EVAL_REFLECTION_MEMBER_FLAG_PUBLIC,
        Visibility::Protected => EVAL_REFLECTION_MEMBER_FLAG_PROTECTED,
        Visibility::Private => EVAL_REFLECTION_MEMBER_FLAG_PRIVATE,
    };
    if slot.is_final {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_FINAL;
    }
    if slot.is_enum_case {
        flags |= EVAL_REFLECTION_MEMBER_FLAG_ENUM_CASE;
    }
    flags
}

/// Returns class scopes that satisfy one constant visibility for a declaring class.
fn visibility_scope_names(
    module: &Module,
    declaring_class: &str,
    visibility: &Visibility,
) -> Vec<String> {
    match visibility {
        Visibility::Public => Vec::new(),
        Visibility::Private => vec![declaring_class.to_string()],
        Visibility::Protected => related_class_scope_names(module, declaring_class),
    }
}

/// Returns AOT classes in the same inheritance line as `declaring_class`.
fn related_class_scope_names(module: &Module, declaring_class: &str) -> Vec<String> {
    let mut scopes = module
        .class_infos
        .keys()
        .filter(|class_name| {
            is_same_or_descendant(module, class_name, declaring_class)
                || is_same_or_descendant(module, declaring_class, class_name)
        })
        .cloned()
        .collect::<Vec<_>>();
    scopes.sort_by(|left, right| {
        class_id_for_scope(module, left)
            .cmp(&class_id_for_scope(module, right))
            .then_with(|| left.cmp(right))
    });
    scopes
}

/// Returns true when `class_name` is `ancestor` or descends from it.
fn is_same_or_descendant(module: &Module, class_name: &str, ancestor: &str) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Returns the deterministic class id used to order generated scope checks.
fn class_id_for_scope(module: &Module, class_name: &str) -> u64 {
    module
        .class_infos
        .get(class_name)
        .map(|class_info| class_info.class_id)
        .unwrap_or(u64::MAX)
}

/// Returns a platform-safe body label for one class-constant slot.
fn slot_body_label(module: &Module, slot: &EvalClassConstantSlot, mode: &str) -> String {
    let suffix = match module.target.arch {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    format!(
        "__elephc_eval_class_constant_{}_{}_{}_{}{}",
        mode,
        label_fragment(&slot.reflected_class),
        label_fragment(&slot.declaring_class),
        label_fragment(&slot.constant),
        suffix
    )
}

/// Returns a platform-safe label for continuing after one slot miss.
fn slot_miss_label(module: &Module, slot: &EvalClassConstantSlot, mode: &str) -> String {
    format!("{}_miss", slot_body_label(module, slot, mode))
}

/// Converts arbitrary PHP metadata names into assembly-label-safe fragments.
fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// Emits a C-global label using the target's symbol spelling.
fn label_c_global(module: &Module, emitter: &mut Emitter, symbol: &str) {
    let label = module.target.extern_symbol(symbol);
    emitter.label_global(&label);
}
