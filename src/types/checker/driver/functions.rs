use std::collections::HashSet;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, Program, StmtKind, TypeExpr};

use super::super::{Checker, FnDecl};

impl Checker {
    pub(super) fn collect_function_decls(
        &mut self,
        program: &Program,
        errors: &mut Vec<CompileError>,
    ) {
        let mut seen_functions = HashSet::new();
        for stmt in program {
            if let StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                return_type,
                body,
                ..
            } = &stmt.kind
            {
                if !seen_functions.insert(php_symbol_key(name)) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate function declaration: {}", name),
                    ));
                    continue;
                }
                if let Some(builtin) =
                    crate::types::checker::builtins::canonical_builtin_function_name(name)
                {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Cannot redeclare built-in function: {}", builtin),
                    ));
                    continue;
                }
                let param_names: Vec<String> =
                    params.iter().map(|(n, _, _, _)| n.clone()).collect();
                let param_type_anns: Vec<Option<TypeExpr>> =
                    params.iter().map(|(_, t, _, _)| t.clone()).collect();
                let defaults: Vec<Option<Expr>> =
                    params.iter().map(|(_, _, d, _)| d.clone()).collect();
                let ref_flags: Vec<bool> = params.iter().map(|(_, _, _, r)| *r).collect();
                self.fn_decls.insert(
                    name.clone(),
                    FnDecl {
                        params: param_names,
                        param_types: param_type_anns,
                        defaults,
                        ref_params: ref_flags,
                        variadic: variadic.clone(),
                        return_type: return_type.clone(),
                        span: stmt.span,
                        body: body.clone(),
                    },
                );
            }
        }
    }

    pub(super) fn has_function_decl_folded(&self, name: &str) -> bool {
        let key = php_symbol_key(name);
        self.fn_decls
            .keys()
            .any(|existing| php_symbol_key(existing) == key)
            || self
                .extern_functions
                .keys()
                .any(|existing| php_symbol_key(existing) == key)
    }

    pub(super) fn resolve_unchecked_functions(&mut self, errors: &mut Vec<CompileError>) {
        let unchecked: Vec<String> = self
            .fn_decls
            .keys()
            .filter(|name| !self.functions.contains_key(*name))
            .cloned()
            .collect();
        for name in unchecked {
            if let Some(decl) = self.fn_decls.get(&name).cloned() {
                match self.initial_function_param_types(&name, &decl) {
                    Ok(param_types) => {
                        if let Err(error) =
                            self.resolve_function_signature(&name, &decl, param_types)
                        {
                            errors.extend(error.flatten());
                        }
                    }
                    Err(error) => errors.extend(error.flatten()),
                }
            }
        }
    }
}
