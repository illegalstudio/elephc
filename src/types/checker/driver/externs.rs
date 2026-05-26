//! Purpose:
//! Implements the checker driver externs phase.
//! Owns one ordered step in building checker state and validating the program before optimization/codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//!
//! Key details:
//! - Phase order controls diagnostics, available declarations, required libraries, and function-local environments.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Program, StmtKind};
use crate::types::{
    ctype_stack_size, ctype_to_php_type, packed_type_size, ExternClassInfo, ExternFieldInfo,
    ExternFunctionSig, FunctionSig, PackedClassInfo, PackedFieldInfo,
};

use super::super::Checker;

impl Checker {
    /// Iterates program statements, dispatching `ExternFunctionDecl`, `ExternClassDecl`,
    /// `PackedClassDecl`, and `ExternGlobalDecl` nodes to their respective prescan handlers.
    /// Collects validation errors without halting early, ensuring all declarations are processed.
    pub(super) fn prescan_extern_decls(
        &mut self,
        program: &Program,
        errors: &mut Vec<CompileError>,
    ) {
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
                    self.prescan_packed_class(name, fields, stmt.span, errors)
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
    }

    /// Checks whether a class name is already registered in `classes`, `extern_classes`, or
    /// `packed_classes`, using PHP-symbol key comparison for case-insensitive matching.
    pub(super) fn has_class_decl_folded(&self, name: &str) -> bool {
        let key = php_symbol_key(name);
        self.classes
            .keys()
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
            defaults: params.iter().map(|_| None).collect(),
            return_type: php_ret.clone(),
            declared_return: true,
            ref_params: params.iter().map(|_| false).collect(),
            declared_params: vec![true; php_params.len()],
            variadic: None,
            deprecation: None,
        };
        self.functions.insert(name.to_string(), sig);
        self.extern_functions.insert(
            name.to_string(),
            ExternFunctionSig {
                name: name.to_string(),
                params: php_params,
                return_type: php_ret,
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
    /// Computes field offsets sequentially, checks for duplicate fields, converts C types
    /// to PHP types, and stores an `ExternClassInfo` with total size and field metadata.
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
            let size = ctype_stack_size(&f.c_type);
            extern_fields.push(ExternFieldInfo {
                name: f.name.clone(),
                php_type,
                offset,
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
                total_size: offset,
                fields: extern_fields,
            },
        );
    }

    /// Validates and registers a packed class declaration if no prior declaration exists.
    /// Resolves PHP types from field type expressions, validates POD constraints, computes
    /// sequential offsets, checks for duplicate fields, and stores a `PackedClassInfo`.
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
            let php_type = match self.resolve_type_expr(&field.type_expr, field.span) {
                Ok(php_type) => php_type,
                Err(error) => {
                    errors.extend(error.flatten());
                    class_has_errors = true;
                    continue;
                }
            };
            let Some(size) = packed_type_size(&php_type, &self.packed_classes) else {
                errors.push(CompileError::new(
                    field.span,
                    "Packed class fields must use POD scalars, pointers, or packed classes",
                ));
                class_has_errors = true;
                continue;
            };
            packed_fields.push(PackedFieldInfo {
                name: field.name.clone(),
                php_type,
                offset,
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
                total_size: offset,
            },
        );
    }
}
