use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{TypeExpr, UseItem, UseKind};

use super::{resolved_name, Imports, Symbols};

pub(super) fn resolve_type_expr(
    type_expr: &TypeExpr,
    current_namespace: Option<&str>,
    imports: &Imports,
) -> TypeExpr {
    match type_expr {
        TypeExpr::Int => TypeExpr::Int,
        TypeExpr::Float => TypeExpr::Float,
        TypeExpr::Bool => TypeExpr::Bool,
        TypeExpr::Str => TypeExpr::Str,
        TypeExpr::Void => TypeExpr::Void,
        TypeExpr::Buffer(inner) => {
            TypeExpr::Buffer(Box::new(resolve_type_expr(inner, current_namespace, imports)))
        }
        TypeExpr::Nullable(inner) => {
            TypeExpr::Nullable(Box::new(resolve_type_expr(inner, current_namespace, imports)))
        }
        TypeExpr::Union(members) => TypeExpr::Union(
            members
                .iter()
                .map(|member| resolve_type_expr(member, current_namespace, imports))
                .collect(),
        ),
        TypeExpr::Ptr(None) => TypeExpr::Ptr(None),
        TypeExpr::Ptr(Some(name)) => {
            let raw = name.as_str();
            if matches!(raw, "int" | "float" | "bool" | "string") {
                TypeExpr::Ptr(Some(name.clone()))
            } else {
                TypeExpr::Ptr(Some(resolved_name(resolve_special_or_class_name(
                    name,
                    current_namespace,
                    imports,
                ))))
            }
        }
        TypeExpr::Named(name) => {
            let raw = name.as_str();
            if matches!(raw, "array" | "mixed" | "callable" | "void") {
                TypeExpr::Named(name.clone())
            } else {
                TypeExpr::Named(resolved_name(resolve_special_or_class_name(
                    name,
                    current_namespace,
                    imports,
                )))
            }
        }
    }
}

pub(super) fn register_imports(
    imports: &mut Imports,
    use_items: &[UseItem],
    span: crate::span::Span,
) -> Result<(), CompileError> {
    for item in use_items {
        let target = item.name.as_canonical();
        let alias_map = match item.kind {
            UseKind::Class => &mut imports.classes,
            UseKind::Function => &mut imports.functions,
            UseKind::Const => &mut imports.constants,
        };
        if alias_map.insert(item.alias.clone(), target).is_some() {
            return Err(CompileError::new(
                span,
                &format!("Duplicate import alias: {}", item.alias),
            ));
        }
    }
    Ok(())
}

pub(super) fn resolve_special_or_class_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
) -> String {
    match name.as_canonical().as_str() {
        "self" | "parent" | "static" => name.as_canonical(),
        _ => resolved_class_name(name, current_namespace, imports),
    }
}

pub(super) fn resolved_class_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| imports.classes.get(segment))
        {
            return alias.clone();
        }
    } else if let Some(first) = name.parts.first() {
        if let Some(alias) = imports.classes.get(first) {
            let suffix = &name.parts[1..];
            if suffix.is_empty() {
                return alias.clone();
            }
            return format!("{}\\{}", alias, suffix.join("\\"));
        }
    }
    if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, name.as_canonical());
        }
    }
    name.as_canonical()
}

pub(super) fn resolve_function_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| imports.functions.get(segment))
        {
            return alias.clone();
        }
        let local = if let Some(namespace) = current_namespace {
            if !namespace.is_empty() {
                format!("{}\\{}", namespace, name.as_canonical())
            } else {
                name.as_canonical()
            }
        } else {
            name.as_canonical()
        };
        if current_namespace.is_some() && symbols.has_function(&local) {
            return local;
        }
        if symbols.has_function(&name.as_canonical()) {
            return name.as_canonical();
        }
        return local;
    }
    if let Some(first) = name.parts.first() {
        if let Some(alias) = imports.functions.get(first) {
            let suffix = &name.parts[1..];
            if suffix.is_empty() {
                return alias.clone();
            }
            return format!("{}\\{}", alias, suffix.join("\\"));
        }
    }
    if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, name.as_canonical());
        }
    }
    name.as_canonical()
}

pub(super) fn resolve_constant_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| imports.constants.get(segment))
        {
            return alias.clone();
        }
        let local = if let Some(namespace) = current_namespace {
            if !namespace.is_empty() {
                format!("{}\\{}", namespace, name.as_canonical())
            } else {
                name.as_canonical()
            }
        } else {
            name.as_canonical()
        };
        if current_namespace.is_some() && symbols.has_constant(&local) {
            return local;
        }
        if symbols.has_constant(&name.as_canonical()) {
            return name.as_canonical();
        }
        return local;
    }
    if let Some(first) = name.parts.first() {
        if let Some(alias) = imports.constants.get(first) {
            let suffix = &name.parts[1..];
            if suffix.is_empty() {
                return alias.clone();
            }
            return format!("{}\\{}", alias, suffix.join("\\"));
        }
    }
    if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, name.as_canonical());
        }
    }
    name.as_canonical()
}
