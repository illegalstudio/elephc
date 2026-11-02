//! Purpose:
//! Implements the checker driver externs phase.
//! Owns one ordered step in building checker state and validating the program before optimization/codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//!
//! Key details:
//! - Phase order controls diagnostics, available declarations, required libraries, and function-local environments.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Program, StmtKind, TypeExpr};
use crate::types::{
    align_up, ctype_field_layout, ctype_to_abi_type, ctype_to_php_type, packed_type_layout,
    width_type_from_type_expr, ExternClassInfo, ExternFieldInfo, ExternFunctionSig, FunctionSig,
    PackedClassInfo, PackedFieldInfo, PhpType,
};

use super::super::Checker;

impl Checker {
    /// Iterates program statements, dispatching `ExternFunctionDecl`, `ExternClassDecl`,
    /// `PackedClassDecl`, and `ExternGlobalDecl` nodes to their respective prescan handlers.
    /// Collects validation errors without halting early, ensuring all declarations are processed.
    ///
    /// A packed class that embeds a packed class declared later in the program is sized once
    /// the embedded layout exists, so declaration order does not matter; a set of packed classes
    /// that embed each other in a cycle has no finite layout and is reported instead.
    pub(super) fn prescan_extern_decls(
        &mut self,
        program: &Program,
        errors: &mut Vec<CompileError>,
    ) {
        self.declared_native_records = program
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::PackedClassDecl { name, .. }
                | StmtKind::ExternClassDecl { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        let packed_decl_names: HashSet<&str> = program
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::PackedClassDecl { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        let mut deferred_packed = Vec::new();
        for stmt in program {
            match &stmt.kind {
                StmtKind::ExternFunctionDecl {
                    name,
                    params,
                    return_type,
                    library,
                } => self.prescan_extern_function(name, params, return_type, library, stmt.span, errors),
                StmtKind::ExternClassDecl { name, fields } => {
                    self.prescan_extern_class(name, fields, stmt.span, errors)
                }
                StmtKind::PackedClassDecl { name, fields } => {
                    let waits_for_later_decl = embedded_packed_names(fields).any(|inner| {
                        packed_decl_names.contains(inner) && !self.packed_classes.contains_key(inner)
                    });
                    if waits_for_later_decl {
                        deferred_packed.push((name, fields, stmt.span));
                    } else {
                        self.prescan_packed_class(name, fields, stmt.span, errors);
                    }
                }
                StmtKind::ExternGlobalDecl { name, c_type } => {
                    if let Err(error) = self.validate_extern_global_decl(name, c_type, stmt.span) {
                        errors.extend(error.flatten());
                        continue;
                    }
                    let php_type = ctype_to_php_type(c_type);
                    self.extern_globals.insert(name.clone(), php_type);
                }
                _ => {}
            }
        }
        self.prescan_deferred_packed_classes(deferred_packed, errors);
    }

    /// Sizes the packed classes that embed a packed class declared after them, in dependency
    /// order. Each pass sizes the first class whose embedded packed classes are no longer
    /// pending; when no class can progress, every class left embeds itself through a cycle.
    fn prescan_deferred_packed_classes(
        &mut self,
        mut pending: Vec<(&String, &Vec<crate::parser::ast::PackedField>, crate::span::Span)>,
        errors: &mut Vec<CompileError>,
    ) {
        while !pending.is_empty() {
            let pending_names: HashSet<&str> =
                pending.iter().map(|(name, _, _)| name.as_str()).collect();
            let ready = pending.iter().position(|(_, fields, _)| {
                embedded_packed_names(fields).all(|inner| !pending_names.contains(inner))
            });
            let Some(ready) = ready else {
                for (name, _, span) in pending {
                    errors.push(CompileError::new(
                        span,
                        &format!(
                            "Packed class {} embeds itself through its packed class fields",
                            name
                        ),
                    ));
                }
                return;
            };
            let (name, fields, span) = pending.remove(ready);
            self.prescan_packed_class(name, fields, span, errors);
        }
    }

    /// Checks whether a class name is already registered in `classes`, `extern_classes`, or
    /// `packed_classes`, or pre-declared in `declared_classes`, using PHP-symbol key comparison
    /// for case-insensitive matching.
    ///
    /// The extern prescan runs before the class schema pass fills `classes`, so the pre-declared
    /// names (every class in the class map, builtin injections included, plus every enum) are what
    /// catches an extern or packed class that reuses an ordinary class name.
    pub(super) fn has_class_decl_folded(&self, name: &str) -> bool {
        let key = php_symbol_key(name);
        self.classes
            .keys()
            .any(|existing| php_symbol_key(existing) == key)
            || self
                .declared_classes
                .iter()
                .any(|existing| php_symbol_key(existing) == key)
            || self
                .extern_classes
                .keys()
                .any(|existing| php_symbol_key(existing) == key)
            || self
                .packed_classes
                .keys()
                .any(|existing| php_symbol_key(existing) == key)
    }

    /// Validates and registers an extern function declaration if no prior declaration exists.
    /// Converts C types to PHP types, builds a `FunctionSig` and `ExternFunctionSig`, inserts
    /// both into `functions` and `extern_functions`, and appends the required library.
    #[allow(clippy::too_many_arguments)]
    fn prescan_extern_function(
        &mut self,
        name: &str,
        params: &[crate::parser::ast::ExternParam],
        return_type: &crate::parser::ast::CType,
        library: &Option<String>,
        span: crate::span::Span,
        errors: &mut Vec<CompileError>,
    ) {
        if self.extern_functions.contains_key(name)
            || self.fn_decls.contains_key(name)
            || self.has_function_decl_folded(name)
        {
            errors.push(CompileError::new(
                span,
                &format!("Duplicate function declaration: {}", name),
            ));
            return;
        }
        let php_params: Vec<(String, crate::types::PhpType)> = params
            .iter()
            .map(|p| (p.name.clone(), ctype_to_php_type(&p.c_type)))
            .collect();
        let php_ret = ctype_to_php_type(return_type);
        let abi_params: Vec<_> = params.iter().map(|p| ctype_to_abi_type(&p.c_type)).collect();
        let abi_return = ctype_to_abi_type(return_type);
        if let Err(error) = self.validate_extern_function_decl(
            name,
            params,
            return_type,
            &php_params,
            &php_ret,
            span,
        ) {
            errors.extend(error.flatten());
            return;
        }
        let sig = FunctionSig {
            params: php_params.clone(),
            param_type_exprs: vec![None; php_params.len()],
            param_attributes: Vec::new(),
            defaults: params.iter().map(|_| None).collect(),
            return_type: php_ret.clone(),
            declared_return: true,
            by_ref_return: false,
            ref_params: params.iter().map(|_| false).collect(),
            declared_params: vec![true; php_params.len()],
            variadic: None,
            deprecation: None,
            is_generator: false,
        };
        self.functions.insert(name.to_string(), sig);
        self.extern_functions.insert(
            name.to_string(),
            ExternFunctionSig {
                name: name.to_string(),
                params: php_params,
                abi_params,
                return_type: php_ret,
                abi_return,
                library: library.clone(),
            },
        );
        if let Some(lib) = library {
            if !self.required_libraries.contains(lib) {
                self.required_libraries.push(lib.clone());
            }
        }
    }

    /// Validates and registers an extern class declaration if no prior declaration exists.
    /// Lays the fields out like a C compiler does (each field at the next multiple of its
    /// natural alignment, the total size padded to the largest alignment), checks for duplicate
    /// fields, converts C types to PHP types, and stores an `ExternClassInfo` with the C
    /// `sizeof` and per-field offsets and ABI shapes.
    fn prescan_extern_class(
        &mut self,
        name: &str,
        fields: &[crate::parser::ast::ExternField],
        span: crate::span::Span,
        errors: &mut Vec<CompileError>,
    ) {
        if self.extern_classes.contains_key(name)
            || self.classes.contains_key(name)
            || self.has_class_decl_folded(name)
        {
            errors.push(CompileError::new(
                span,
                &format!("Duplicate class declaration: {}", name),
            ));
            return;
        }
        let mut extern_fields = Vec::new();
        let mut offset = 0usize;
        let mut struct_align = 1usize;
        let mut seen_fields = std::collections::HashSet::new();
        let mut class_has_errors = false;
        for f in fields {
            if let Err(error) = self.validate_extern_field_decl(name, f, span) {
                errors.extend(error.flatten());
                class_has_errors = true;
                continue;
            }
            if !seen_fields.insert(f.name.clone()) {
                errors.push(CompileError::new(
                    span,
                    &format!("Duplicate extern field: {}::{}", name, f.name),
                ));
                class_has_errors = true;
                continue;
            }
            let php_type = ctype_to_php_type(&f.c_type);
            let Some((size, align)) = ctype_field_layout(&f.c_type) else {
                // `validate_extern_field_decl` already refused the types without a layout.
                class_has_errors = true;
                continue;
            };
            offset = align_up(offset, align);
            struct_align = struct_align.max(align);
            extern_fields.push(ExternFieldInfo {
                name: f.name.clone(),
                php_type,
                offset,
                abi: ctype_to_abi_type(&f.c_type),
            });
            offset += size;
        }
        if class_has_errors {
            return;
        }
        self.extern_classes.insert(
            name.to_string(),
            ExternClassInfo {
                name: name.to_string(),
                total_size: align_up(offset, struct_align),
                fields: extern_fields,
            },
        );
    }

    /// Validates and registers a packed class declaration if no prior declaration exists.
    /// Resolves PHP types from field type expressions, validates POD constraints, computes
    /// naturally aligned offsets, checks for duplicate fields, and stores a `PackedClassInfo`.
    ///
    /// A field typed with a C width name (`i8`..`u32`, `f32`) keeps that storage width and
    /// reads as `int` or `float`; `i64`, `u64` and `f64` are the plain word types. Fields are
    /// laid out like a C struct: each at the next multiple of its own alignment, the record
    /// padded to the widest one, so a class of 8-byte fields keeps its former sequential layout.
    fn prescan_packed_class(
        &mut self,
        name: &str,
        fields: &[crate::parser::ast::PackedField],
        span: crate::span::Span,
        errors: &mut Vec<CompileError>,
    ) {
        if self.packed_classes.contains_key(name)
            || self.classes.contains_key(name)
            || self.extern_classes.contains_key(name)
            || self.has_class_decl_folded(name)
        {
            errors.push(CompileError::new(
                span,
                &format!("Duplicate packed class declaration: {}", name),
            ));
            return;
        }
        let mut packed_fields = Vec::new();
        let mut offset = 0usize;
        let mut class_align = 1usize;
        let mut seen_fields = std::collections::HashSet::new();
        let mut class_has_errors = false;
        for field in fields {
            if !seen_fields.insert(field.name.clone()) {
                errors.push(CompileError::new(
                    field.span,
                    &format!("Duplicate packed field: {}::{}", name, field.name),
                ));
                class_has_errors = true;
                continue;
            }
            let storage_type = match width_type_from_type_expr(&field.type_expr) {
                Some(width_type) => width_type,
                None => match self.resolve_type_expr(&field.type_expr, field.span) {
                    Ok(php_type) => php_type,
                    Err(error) => {
                        let error =
                            self.width_position_type_error(&field.type_expr, field.span, error);
                        errors.extend(error.flatten());
                        class_has_errors = true;
                        continue;
                    }
                },
            };
            let Some((size, align)) = packed_type_layout(&storage_type, &self.packed_classes)
            else {
                errors.push(CompileError::new(
                    field.span,
                    "Packed class fields must use POD scalars, pointers, or packed classes",
                ));
                class_has_errors = true;
                continue;
            };
            let width = match &storage_type {
                PhpType::Narrow(width) => Some(*width),
                _ => None,
            };
            offset = align_up(offset, align);
            class_align = class_align.max(align);
            packed_fields.push(PackedFieldInfo {
                name: field.name.clone(),
                php_type: storage_type.storage_value_type(),
                offset,
                width,
            });
            offset += size;
        }
        if class_has_errors {
            return;
        }
        self.packed_classes.insert(
            name.to_string(),
            PackedClassInfo {
                fields: packed_fields,
                total_size: align_up(offset, class_align),
                align: class_align,
            },
        );
    }
}

/// Yields the class names a packed declaration embeds by value (fields typed as a bare class
/// name). Pointer fields only store an address and do not need the target's layout, and an
/// unqualified C width name (`u8`, `f32`) is a storage format that `prescan_packed_class`
/// resolves before any class lookup, so it never waits for a class of that name.
fn embedded_packed_names(
    fields: &[crate::parser::ast::PackedField],
) -> impl Iterator<Item = &str> {
    fields.iter().filter_map(|field| match &field.type_expr {
        TypeExpr::Named(name) if width_type_from_type_expr(&field.type_expr).is_none() => {
            Some(name.as_str())
        }
        _ => None,
    })
}
