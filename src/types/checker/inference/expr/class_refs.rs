//! Purpose:
//! Infers expression class refs forms for the checker.
//! Handles type facts and diagnostics for expression shapes that need more than scalar/operator inference.
//!
//! Called from:
//! - `crate::types::checker::inference::expr`
//!
//! Key details:
//! - Expression inference shares environments with statement checking, so variable and effect updates must stay synchronized.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, StaticReceiver};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

impl Checker {
    /// Validates `new` expressions on late-bound static constructor targets by inferring
    /// the object type for every class that descends from `base_class`.
    ///
    /// Used when `$obj::new(...)` or similar late-bound constructor syntax is used,
    /// to ensure each possible class variant is well-typed.
    pub(super) fn validate_late_bound_constructor_targets(
        &mut self,
        base_class: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let mut class_names: Vec<String> = self
            .classes
            .keys()
            .filter(|name| self.class_is_same_or_descends_from(name, base_class))
            .cloned()
            .collect();
        class_names.sort();

        for class_name in class_names {
            self.infer_new_object_type(&class_name, args, expr, env)?;
        }

        Ok(())
    }

    /// Checks whether `class_name` is either `base_class` itself or a descendant of it
    /// by walking the parent chain.
    fn class_is_same_or_descends_from(&self, class_name: &str, base_class: &str) -> bool {
        let mut current = Some(class_name);
        while let Some(name) = current {
            if name == base_class {
                return true;
            }
            current = self
                .classes
                .get(name)
                .and_then(|info| info.parent.as_deref());
        }
        false
    }

    /// Infers the type of a class constant or enum case accessed via scope resolution
    /// (e.g., `MyClass::CONSTANT` or `Color::Red`).
    ///
    /// Searches the class/interface hierarchy for the named constant, preferring enum cases
    /// when the receiver is an enum. Falls back to interface constants and finally returns
    /// an error if the constant is not found.
    pub(crate) fn infer_scoped_constant_access(
        &mut self,
        receiver: &StaticReceiver,
        name: &str,
        expr: &Expr,
    ) -> Result<PhpType, CompileError> {
        let class_name = self.resolve_static_receiver_class(receiver, expr.span)?;
        if !self.scoped_constant_receiver_is_known(&class_name) && self.eval_barrier_active {
            return Ok(PhpType::Mixed);
        }
        // First: enum case access (`Color::Red`). Enums shadow classes for
        // this syntax in PHP since 8.1. A name that is not a declared case is an enum *constant*
        // (`Scale::FACTOR`), which is resolved through the class-constant table below.
        if let Some(enum_info) = self.enums.get(&class_name) {
            if enum_info.cases.iter().any(|case| case.name == name) {
                return self.infer_enum_case_type(&class_name, name, expr);
            }
        }
        // Walk parent chain to find a class constant.
        let mut current_class = Some(class_name.clone());
        while let Some(cn) = current_class.as_deref() {
            if let Some(info) = self.classes.get(cn) {
                if let Some(type_expr) = info.constant_types.get(name).cloned() {
                    return self.resolve_type_expr(&type_expr, expr.span);
                }
                if let Some(value_expr) = info.constants.get(name).cloned() {
                    return self.infer_type(&value_expr, &TypeEnv::default());
                }
            }
            current_class = self.classes.get(cn).and_then(|i| i.parent.clone());
        }
        // Fallback: search implemented interfaces (and parent interfaces).
        if let Some(class_info) = self.classes.get(&class_name).cloned() {
            for iface_name in &class_info.interfaces {
                if let Some((value, type_expr)) = self.lookup_interface_constant(iface_name, name) {
                    if let Some(type_expr) = type_expr {
                        return self.resolve_type_expr(&type_expr, expr.span);
                    }
                    return self.infer_type(&value, &TypeEnv::default());
                }
            }
        }
        // Direct interface receiver (`Limits::MAX`).
        if let Some((value, type_expr)) = self.lookup_interface_constant(&class_name, name) {
            if let Some(type_expr) = type_expr {
                return self.resolve_type_expr(&type_expr, expr.span);
            }
            return self.infer_type(&value, &TypeEnv::default());
        }
        // On an enum, a `::name` that is neither a declared case nor a constant is an undefined
        // case — report that rather than the generic class-constant message.
        if self.enums.contains_key(&class_name) {
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined enum case: {}::{}", class_name, name),
            ));
        }
        Err(CompileError::new(
            expr.span,
            &format!("Undefined class constant: {}::{}", class_name, name),
        ))
    }

    /// Returns whether a scoped-constant receiver is known in static class-like metadata.
    fn scoped_constant_receiver_is_known(&self, class_name: &str) -> bool {
        self.classes.contains_key(class_name)
            || self.interfaces.contains_key(class_name)
            || self.declared_traits.contains(class_name)
            || self.enums.contains_key(class_name)
    }

    /// Looks up a constant by name on an interface, traversing parent interfaces breadth-first
    /// to find it. Returns its value expression and optional declared type.
    fn lookup_interface_constant(
        &self,
        interface_name: &str,
        const_name: &str,
    ) -> Option<(crate::parser::ast::Expr, Option<crate::parser::ast::TypeExpr>)> {
        let mut visited = std::collections::HashSet::new();
        let mut queue: Vec<String> = vec![interface_name.to_string()];
        while let Some(name) = queue.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            if let Some(iface) = self.interfaces.get(&name) {
                if let Some(value) = iface.constants.get(const_name) {
                    return Some((
                        value.clone(),
                        iface.constant_types.get(const_name).cloned(),
                    ));
                }
                queue.extend(iface.parents.iter().cloned());
            }
        }
        None
    }

    /// Resolves a `StaticReceiver` to its canonical class name string.
    ///
    /// - `Named` returns the class name directly.
    /// - `Self_` / `Static` return the current class, or error if not inside a class.
    /// - `Parent` returns the parent of the current class, or error if there is no parent.
    fn resolve_static_receiver_class(
        &self,
        receiver: &StaticReceiver,
        span: Span,
    ) -> Result<String, CompileError> {
        match receiver {
            StaticReceiver::Named(name) => Ok(name.as_canonical()),
            StaticReceiver::Self_ | StaticReceiver::Static => self
                .current_class
                .clone()
                .ok_or_else(|| CompileError::new(span, "Cannot use self:: outside a class context")),
            StaticReceiver::Parent => {
                let current = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(span, "Cannot use parent:: outside a class context")
                })?;
                self.classes
                    .get(current)
                    .and_then(|info| info.parent.clone())
                    .ok_or_else(|| {
                        CompileError::new(
                            span,
                            &format!("Class '{}' has no parent class", current),
                        )
                    })
            }
        }
    }

    /// Validates that `self::class`, `static::class`, or `parent::class` is used in an
    /// appropriate class context. Returns an error for invalid scope (e.g., outside a class
    /// or on a class with no parent for `parent::class`).
    pub(super) fn validate_class_constant_receiver(
        &self,
        receiver: &StaticReceiver,
        span: Span,
    ) -> Result<(), CompileError> {
        match receiver {
            StaticReceiver::Named(_) => Ok(()),
            StaticReceiver::Self_ | StaticReceiver::Static => {
                if self.current_class.is_some() {
                    Ok(())
                } else {
                    Err(CompileError::new(
                        span,
                        "Cannot use self::class or static::class outside a class context",
                    ))
                }
            }
            StaticReceiver::Parent => {
                let current = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(
                        span,
                        "Cannot use parent::class outside a class context",
                    )
                })?;
                if self
                    .classes
                    .get(current)
                    .and_then(|info| info.parent.as_ref())
                    .is_some()
                {
                    Ok(())
                } else {
                    Err(CompileError::new(
                        span,
                        &format!("Class '{}' has no parent class", current),
                    ))
                }
            }
        }
    }
}
