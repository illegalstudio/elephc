//! Purpose:
//! Selects source constants and emits host-owned PHP constant registration.
//!
//! Called from:
//! - The PHP export scan and SDK adapter generator.
//!
//! Key details:
//! - Values use the compiler's constant resolver and folding semantics.
//! - Persistent scalar storage is allocated and retired by Zend, never the native heap.

use std::collections::{HashMap, HashSet};

use crate::{errors::CompileError, parser::ast::{Expr, ExprKind, Program, StmtKind, TypeExpr},
    span::Span, types::PhpType, codegen_support::platform::Target};

/// One explicitly selected global constant with a resolved host value.
#[derive(Clone, Debug)]
pub struct ExportedConstant {
    pub name: String,
    value: ConstantValue,
}

pub use super::constant_values::ConstantValue;

/// Selects declared constants without evaluating source code or activating includes.
pub fn collect(program: &Program, names: &[String], target: Target) -> Result<Vec<ExportedConstant>, CompileError> {
    if names.is_empty() { return Ok(Vec::new()); }
    let declarations = program.iter().filter_map(|stmt| match &stmt.kind {
        StmtKind::ConstDecl { name, value } => Some((name.as_str(), value)),
        _ => None,
    }).collect::<HashMap<_, _>>();
    let constants = context(program, target);
    let mut selected = HashSet::new();
    let mut exports = Vec::new();
    for name in names {
        let name = name.strip_prefix('\\').unwrap_or(name);
        if !selected.insert(name.to_string()) { continue; }
        let source = declarations.get(name).ok_or_else(|| CompileError::new(Span::dummy(),
            &format!("PHP constant export '{name}' is not a top-level const declaration")))?;
        if name == "__COMPILER_HALT_OFFSET__" {
            return Err(CompileError::new(source.span, "PHP constant export uses a reserved constant name"));
        }
        let value = evaluate(source, &constants).map_err(|_| CompileError::new(source.span,
            &format!("PHP constant export '{name}' requires a statically resolved scalar or array constant")))?;
        exports.push(ExportedConstant { name: name.to_string(), value });
    }
    Ok(exports)
}

pub(super) use super::constant_types::declared_type;
use super::constant_types::coercion_type;

/// Resolves hosted constant values before EIR storage inference and native reads.
pub fn materialize_native_constants(
    program: &mut Program, classes: &mut HashMap<String, crate::types::ClassInfo>,
    interfaces: &mut HashMap<String, crate::types::InterfaceInfo>, target: Target,
) {
    let mut constants = context(program, target);
    for (owner, info) in classes.iter() {
        for (name, value) in &info.constants {
            let ty = info.constant_types.get(name).map(coercion_type).unwrap_or(PhpType::Mixed);
            constants.entry(format!("{owner}::{name}")).or_insert((bind_scope(value.clone(), owner).kind, ty));
        }
    }
    for (owner, info) in interfaces.iter() {
        for (name, value) in &info.constants {
            let scope = info.constant_declaring_interfaces.get(name).map(String::as_str).unwrap_or(owner);
            let ty = info.constant_types.get(name).map(coercion_type).unwrap_or(PhpType::Mixed);
            constants.entry(format!("{owner}::{name}")).or_insert((bind_scope(value.clone(), scope).kind, ty));
        }
    }
    for (owner, info) in classes {
        native_schema(owner, &mut info.constants, &info.constant_types, None, &constants);
        for ((_, ty), value) in info.properties.iter().zip(&mut info.defaults) {
            if let Some(value) = value {
                if let Some(kind) = native_value(value, owner, ty, &constants) { value.kind = kind; }
            }
        }
    }
    for (owner, info) in interfaces {
        native_schema(owner, &mut info.constants, &info.constant_types,
            Some(&info.constant_declaring_interfaces), &constants);
    }
    for statement in program {
        if let StmtKind::ConstDecl { value, .. } = &mut statement.kind {
            if let Some(kind) = native_value(value, "", &PhpType::Mixed, &constants) { value.kind = kind; }
        }
    }
}

/// Preserves lexical interface owners while rewriting the supported native constant trees.
fn native_schema(
    owner: &str, values: &mut HashMap<String, Expr>, types: &HashMap<String, TypeExpr>,
    declaring: Option<&HashMap<String, String>>, constants: &HashMap<String, (ExprKind, PhpType)>,
) {
    for (name, value) in values {
        let ty = types.get(name).map(coercion_type).unwrap_or(PhpType::Mixed);
        let scope = declaring.and_then(|owners| owners.get(name)).map(String::as_str).unwrap_or(owner);
        if let Some(kind) = native_value(value, scope, &ty, constants) { value.kind = kind; }
    }
}

/// Rejects unresolved widening only after unused declarations have been pruned.
pub fn validate_native_widening(
    classes: &HashMap<String, crate::types::ClassInfo>, interfaces: &HashMap<String, crate::types::InterfaceInfo>,
) -> Result<(), CompileError> {
    let schemas = classes.iter().map(|(owner, info)| (owner, &info.constants, &info.constant_types))
        .chain(interfaces.iter().map(|(owner, info)| (owner, &info.constants, &info.constant_types)));
    for (owner, values, types) in schemas {
        for (name, value) in values {
            if types.get(name).map(coercion_type).is_some_and(|ty| ty.coerces_integer_to_float())
                && super::constant_values::from_expression(value, 0).is_err() {
                return Err(CompileError::new(value.span, &format!(
                    "PHP extension constant '{owner}::{name}' requires a statically resolved value for integer-to-float coercion")));
            }
        }
    }
    Ok(())
}

/// Keeps supported constant trees identical in native schemas and persistent Zend metadata.
fn native_value(value: &Expr, class: &str, ty: &PhpType, constants: &HashMap<String, (ExprKind, PhpType)>) -> Option<ExprKind> {
    let mut resolved = resolve(bind_scope(value.clone(), class), constants, &mut HashSet::new(), 0).ok()?;
    super::constant_values::from_expression(&resolved, 0).ok()?;
    if ty.coerces_integer_to_float() {
        if let ExprKind::IntLiteral(integer) = resolved.kind { resolved.kind = ExprKind::FloatLiteral(integer as f64); }
    }
    Some(resolved.kind)
}

/// Combines checked source constants with lexical bindings for class constant references.
pub(super) fn context(program: &Program, target: Target) -> HashMap<String, (ExprKind, PhpType)> {
    // Top-level statements never run when PHP loads the module, so define() is not a source.
    let mut constants = crate::codegen_support::collect_constants(&Vec::new(), target);
    for statement in program {
        if let StmtKind::ConstDecl { name, value } = &statement.kind {
            constants.entry(name.clone()).or_insert((value.kind.clone(), PhpType::Mixed));
        }
    }
    for stmt in program {
        let (class, declarations) = match &stmt.kind {
            StmtKind::ClassDecl { name, constants, .. } | StmtKind::InterfaceDecl { name, constants, .. }
                | StmtKind::EnumDecl { name, constants, .. } => (name, constants),
            _ => continue,
        };
        for declaration in declarations {
            let ty = declaration.type_expr.as_ref().map(coercion_type).unwrap_or(PhpType::Mixed);
            constants.insert(format!("{class}::{}", declaration.name),
                (bind_scope(declaration.value.clone(), class).kind, ty));
        }
    }
    constants
}

/// Keeps class-local self/static references lexical while sharing the scalar evaluator.
pub(super) fn evaluate_in_class(source: &Expr, class: &str, constants: &HashMap<String, (ExprKind, PhpType)>) -> Result<ConstantValue, CompileError> {
    evaluate(&bind_scope(source.clone(), class), constants)
}

/// Canonicalizes constant-bearing expression receivers without evaluating PHP source.
fn bind_scope(mut expr: Expr, class: &str) -> Expr {
    use crate::parser::ast::StaticReceiver;
    match &mut expr.kind {
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let owner = match receiver {
                StaticReceiver::Self_ | StaticReceiver::Static if !class.is_empty() => Some(class.to_string()),
                StaticReceiver::Named(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
                _ => None,
            };
            if let Some(owner) = owner { expr.kind = ExprKind::ConstRef(format!("{owner}::{name}").into()); }
        }
        ExprKind::ClassConstant { receiver } => {
            let owner = match receiver {
                StaticReceiver::Self_ | StaticReceiver::Static if !class.is_empty() => Some(class.to_string()),
                StaticReceiver::Named(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
                _ => None,
            };
            if let Some(owner) = owner { expr.kind = ExprKind::StringLiteral(owner); }
        }
        ExprKind::ArrayAccess { array, index } => {
            **array = bind_scope((**array).clone(), class);
            **index = bind_scope((**index).clone(), class);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            **left = bind_scope((**left).clone(), class);
            **right = bind_scope((**right).clone(), class);
        }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value) | ExprKind::Spread(value) | ExprKind::Cast { expr: value, .. } => {
            **value = bind_scope((**value).clone(), class);
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            **value = bind_scope((**value).clone(), class);
            **default = bind_scope((**default).clone(), class);
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            **condition = bind_scope((**condition).clone(), class);
            **then_expr = bind_scope((**then_expr).clone(), class);
            **else_expr = bind_scope((**else_expr).clone(), class);
        }
        ExprKind::ArrayLiteral(values) => {
            for value in values { *value = bind_scope(value.clone(), class); }
        }
        ExprKind::ArrayLiteralAssoc(values) => {
            for (key, value) in values {
                *key = bind_scope(key.clone(), class);
                *value = bind_scope(value.clone(), class);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries { *entry = entry.clone().map_exprs(|expr| bind_scope(expr, class)); }
        }
        _ => {}
    }
    expr
}

/// Resolves metadata values through shared compiler constant folding.
pub(super) fn evaluate(source: &Expr, constants: &HashMap<String, (ExprKind, PhpType)>) -> Result<ConstantValue, CompileError> {
    let expression = resolve(source.clone(), constants, &mut HashSet::new(), 0)?;
    super::constant_values::from_expression(&expression, 0)
}

/// Emits a Zend-owned persistent scalar for class declarations and property defaults.
pub(super) fn persistent_value(value: &ConstantValue, destination: &str) -> String {
    super::constant_values::persistent_value(value, destination)
}

/// Keeps signed zero and non-finite values valid in generated SDK C source.
fn float_literal(value: f64) -> String {
    super::constant_values::float_literal(value)
}

/// Expands known references, leaving unresolved or cyclic branches for the shared lazy fold.
fn resolve(mut expr: Expr, constants: &HashMap<String, (ExprKind, PhpType)>,
    active: &mut HashSet<String>, depth: usize) -> Result<Expr, CompileError> {
    if depth > 128 { return Err(CompileError::new(expr.span, "PHP constant export expression nesting limit exceeded")); }
    expr = bind_scope(expr, "");
    match &mut expr.kind {
        ExprKind::ConstRef(name) => {
            let key = name.as_str().trim_start_matches('\\').to_string();
            if key != "PHP_BINARY" && active.insert(key.clone()) {
                let value = constants.get(&key).or_else(|| {
                    let (owner, name) = key.rsplit_once("::")?;
                    constants.iter().find_map(|(candidate, value)| {
                        let (candidate_owner, candidate_name) = candidate.rsplit_once("::")?;
                        (candidate_owner.eq_ignore_ascii_case(owner) && candidate_name == name).then_some(value)
                    })
                });
                if let Some((value, ty)) = value {
                    let mut resolved = resolve(Expr { kind: value.clone(), span: expr.span }, constants, active, depth + 1)?;
                    if ty.coerces_integer_to_float() {
                        if let ConstantValue::Int(value) = super::constant_values::from_expression(&resolved, 0)? {
                            resolved.kind = ExprKind::FloatLiteral(value as f64);
                        }
                    }
                    expr.kind = resolved.kind;
                }
                active.remove(&key);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            **array = resolve((**array).clone(), constants, active, depth + 1)?;
            **index = resolve((**index).clone(), constants, active, depth + 1)?;
            if let Some(value) = super::constant_arrays::access(array, index) { expr.kind = value; }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            **left = resolve((**left).clone(), constants, active, depth + 1)?;
            **right = resolve((**right).clone(), constants, active, depth + 1)?;
        }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value) | ExprKind::Spread(value)
            | ExprKind::Cast { expr: value, .. } => {
            **value = resolve((**value).clone(), constants, active, depth + 1)?;
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            **value = resolve((**value).clone(), constants, active, depth + 1)?;
            **default = resolve((**default).clone(), constants, active, depth + 1)?;
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            **condition = resolve((**condition).clone(), constants, active, depth + 1)?;
            **then_expr = resolve((**then_expr).clone(), constants, active, depth + 1)?;
            **else_expr = resolve((**else_expr).clone(), constants, active, depth + 1)?;
        }
        ExprKind::ArrayLiteral(values) => {
            for value in values { *value = resolve(value.clone(), constants, active, depth + 1)?; }
        }
        ExprKind::ArrayLiteralAssoc(values) => {
            for (key, value) in values {
                *key = resolve(key.clone(), constants, active, depth + 1)?;
                *value = resolve(value.clone(), constants, active, depth + 1)?;
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            use crate::parser::ast::ArrayEntry;
            for entry in entries {
                match entry {
                    ArrayEntry::Value(value) | ArrayEntry::Spread(value) =>
                        *value = resolve(value.clone(), constants, active, depth + 1)?,
                    ArrayEntry::Keyed(key, value) => {
                        *key = resolve(key.clone(), constants, active, depth + 1)?;
                        *value = resolve(value.clone(), constants, active, depth + 1)?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(crate::optimize::fold_constant_expression(expr))
}

/// Emits persistent scalar registration after a collision preflight for the entire selection.
pub(super) fn registration(constants: &[ExportedConstant]) -> String {
    use super::producer::c_string;
    let mut text = String::from("#include <stdint.h>\nstatic zend_result ep_preflight_constants(void)\n{\n");
    if constants.iter().any(|constant| matches!(constant.value, ConstantValue::Array(_))) {
        text.insert_str(0, super::constant_values::support());
    }
    for constant in constants {
        let name = c_string(constant.name.as_bytes());
        text.push_str(&format!("    if (zend_get_constant_str({name}, {})) {{ php_error_docref(NULL, E_WARNING, \"PHP extension constant already defined: %s\", {name}); return FAILURE; }}\n", constant.name.len()));
    }
    text.push_str("    return SUCCESS;\n}\nstatic zend_result ep_register_constants(int module_number)\n{\n    if (ep_preflight_constants() != SUCCESS) { return FAILURE; }\n");
    for constant in constants {
        let name = c_string(constant.name.as_bytes());
        if matches!(constant.value, ConstantValue::Array(_)) {
            let initial = persistent_value(&constant.value, "&constant.value");
            text.push_str(&format!("    {{ zend_constant constant; {initial}\n        constant.name = zend_string_init({name}, {}, true);\n        ZEND_CONSTANT_SET_FLAGS(&constant, CONST_PERSISTENT, module_number);\n        if (!zend_register_constant(&constant)) {{ return FAILURE; }}\n    }}\n", constant.name.len()));
            continue;
        }
        let (function, value) = match &constant.value {
            ConstantValue::Null => ("null", String::new()),
            ConstantValue::Bool(value) => ("bool", format!("{}, ", u8::from(*value))),
            ConstantValue::Int(value) => ("long", format!("(zend_long)UINT64_C({}), ", *value as u64)),
            ConstantValue::Float(value) => {
                let literal = float_literal(*value);
                ("double", format!("{literal}, "))
            }
            ConstantValue::String(bytes) => ("stringl", format!("{}, {}, ", c_string(bytes), bytes.len())),
            ConstantValue::Array(_) => unreachable!(),
        };
        text.push_str(&format!("    if (!zend_register_{function}_constant({name}, {}, {value}CONST_PERSISTENT, module_number)) {{ return FAILURE; }}\n", constant.name.len()));
    }
    text.push_str("    return SUCCESS;\n}\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolves arithmetic aliases through the shared evaluator and deduplicates explicit names.
    #[test]
    fn phpext_constant_selection_resolves_scalar_aliases() {
        let tokens = crate::lexer::tokenize("<?php const BASE = 20; const ANSWER = BASE + 22;").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let exports = collect(&program, &["ANSWER".into(), "\\ANSWER".into()], Target::detect_host()).unwrap();
        assert_eq!(exports.len(), 1);
        assert!(matches!(exports[0].value, ConstantValue::Int(42)));
    }

    /// Does not silently export runtime define calls or implicitly expose other constants.
    #[test]
    fn phpext_constant_selection_requires_declared_names() {
        let tokens = crate::lexer::tokenize("<?php define('RUNTIME_VALUE', 42); const INTERNAL_VALUE = 1; const ALIAS = RUNTIME_VALUE;").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        assert!(collect(&program, &[], Target::detect_host()).unwrap().is_empty());
        let error = collect(&program, &["RUNTIME_VALUE".into()], Target::detect_host()).unwrap_err();
        assert!(error.message.contains("not a top-level const declaration"));
        assert!(collect(&program, &["ALIAS".into()], Target::detect_host()).is_err());
    }

    /// Keeps binary strings encoded as C data with an explicit byte length.
    #[test]
    fn phpext_constant_selection_registers_binary_strings() {
        let constants = [ExportedConstant { name: "Demo\\BYTES".into(), value: ConstantValue::String(vec![0, b'"', 255]) }];
        let source = registration(&constants);
        assert!(source.contains("zend_register_stringl_constant"));
        assert!(source.contains("\\000\\042\\377\", 3"));
        assert!(source.contains("CONST_PERSISTENT, module_number"));
    }

    /// Resolves nested class constants and global array aliases without executing source code.
    #[test]
    fn phpext_constant_selection_resolves_array_and_class_aliases() {
        let tokens = crate::lexer::tokenize("<?php final class Values { const BASE = 20; const ANSWER = self::BASE + 22; } const DATA = ['answer' => vAlUeS::ANSWER, ...[1, 2]];").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let constants = collect(&program, &["DATA".into()], Target::detect_host()).unwrap();
        let ConstantValue::Array(entries) = &constants[0].value else { panic!("array constant required"); };
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0], super::super::constant_values::ConstantArrayEntry::Keyed(_, ConstantValue::Int(42))));
    }

    /// Rejects unresolved recursive array definitions rather than recursing during materialization.
    #[test]
    fn phpext_constant_selection_rejects_recursive_arrays() {
        let tokens = crate::lexer::tokenize("<?php const LOOP = [LOOP];").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        assert!(collect(&program, &["LOOP".into()], Target::detect_host()).is_err());
    }
}
