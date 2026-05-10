//! Purpose:
//! Validates statement control flow behavior.
//! Keeps control-flow and assignment effects synchronized with expression inference and return analysis.
//!
//! Called from:
//! - `crate::types::checker::stmt_check`
//!
//! Key details:
//! - Branch and loop handling must preserve PHP execution order and conservative type environments.

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    pub(crate) fn check_control_flow_stmt(
        &mut self,
        stmt: &crate::parser::ast::Stmt,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                body,
            } => {
                let arr_ty = self.infer_type_with_assignment_effects(array, env)?;
                if let PhpType::Array(elem_ty) = &arr_ty {
                    if let Some(k) = key_var {
                        env.insert(k.clone(), PhpType::Int);
                    }
                    env.insert(value_var.clone(), *elem_ty.clone());
                } else if let PhpType::AssocArray { key, value } = &arr_ty {
                    if let Some(k) = key_var {
                        env.insert(k.clone(), *key.clone());
                    }
                    env.insert(value_var.clone(), *value.clone());
                } else if let PhpType::Object(class_name) = &arr_ty {
                    let is_iter = self.class_implements_interface(class_name, "Iterator")
                        || self.interface_extends_interface(class_name, "Iterator");
                    let is_iter_agg = self
                        .class_implements_interface(class_name, "IteratorAggregate")
                        || self.interface_extends_interface(class_name, "IteratorAggregate");
                    if !is_iter && !is_iter_agg {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "foreach over object requires {} to implement Iterator or IteratorAggregate",
                                class_name
                            ),
                        ));
                    }
                    if let Some(k) = key_var {
                        env.insert(k.clone(), PhpType::Mixed);
                    }
                    env.insert(value_var.clone(), PhpType::Mixed);
                } else if let PhpType::Iterable = &arr_ty {
                    if let Some(k) = key_var {
                        env.insert(k.clone(), PhpType::Mixed);
                    }
                    env.insert(value_var.clone(), PhpType::Mixed);
                } else {
                    return Err(CompileError::new(
                        stmt.span,
                        "foreach requires an array, iterable, or an object implementing Iterator/IteratorAggregate",
                    ));
                }
                let errors = self.check_break_continue_target_body(body, env);
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                self.infer_type_with_assignment_effects(subject, env)?;
                let mut errors = Vec::new();
                for (values, _) in cases {
                    for v in values {
                        self.infer_type_with_assignment_effects(v, env)?;
                    }
                }
                self.break_continue_depth += 1;
                for (_, body) in cases {
                    errors.extend(self.check_body(body, env));
                }
                if let Some(body) = default {
                    errors.extend(self.check_body(body, env));
                }
                self.break_continue_depth -= 1;
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                self.infer_type_with_assignment_effects(condition, env)?;
                let mut errors = Vec::new();
                for s in then_body {
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                for (cond, body) in elseif_clauses {
                    self.infer_type_with_assignment_effects(cond, env)?;
                    for s in body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::DoWhile { body, condition } => {
                let errors = self.check_break_continue_target_body(body, env);
                self.infer_type_with_assignment_effects(condition, env)?;
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::While { condition, body } => {
                self.infer_type_with_assignment_effects(condition, env)?;
                let errors = self.check_break_continue_target_body(body, env);
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(s) = init {
                    self.check_stmt(s, env)?;
                }
                if let Some(c) = condition {
                    self.infer_type_with_assignment_effects(c, env)?;
                }
                if let Some(s) = update {
                    self.check_stmt(s, env)?;
                }
                let errors = self.check_break_continue_target_body(body, env);
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::Throw(expr) => {
                let thrown_ty = self.infer_type_with_assignment_effects(expr, env)?;
                match thrown_ty {
                    PhpType::Object(type_name)
                        if self.object_type_implements_throwable(&type_name) =>
                    {
                        Ok(())
                    }
                    PhpType::Object(_) => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object implementing Throwable",
                    )),
                    _ => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object value",
                    )),
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let mut errors = Vec::new();
                for s in try_body {
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                for catch_clause in catches {
                    let mut resolved_types = Vec::new();
                    for raw_exception_type in &catch_clause.exception_types {
                        let exception_type =
                            self.resolve_catch_type_name(raw_exception_type, stmt.span)?;
                        if !self.classes.contains_key(&exception_type)
                            && !self.interfaces.contains_key(&exception_type)
                        {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined class: {}", exception_type),
                            ));
                        }
                        if !self.object_type_implements_throwable(&exception_type) {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Catch type must extend or implement Throwable: {}",
                                    exception_type
                                ),
                            ));
                        }
                        resolved_types.push(exception_type);
                    }
                    if let Some(variable) = &catch_clause.variable {
                        env.insert(
                            variable.clone(),
                            PhpType::Object(self.common_catch_type_name(&resolved_types)),
                        );
                    }
                    for s in &catch_clause.body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if let Some(body) = finally_body {
                    self.finally_break_continue_bases
                        .push(self.break_continue_depth);
                    errors.extend(self.check_body(body, env));
                    self.finally_break_continue_bases.pop();
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            _ => unreachable!("non-control-flow statement routed to control-flow checker"),
        }
    }

    fn check_break_continue_target_body(
        &mut self,
        body: &[Stmt],
        env: &mut TypeEnv,
    ) -> Vec<CompileError> {
        self.break_continue_depth += 1;
        let errors = self.check_body(body, env);
        self.break_continue_depth -= 1;
        errors
    }

    fn check_body(&mut self, body: &[Stmt], env: &mut TypeEnv) -> Vec<CompileError> {
        let mut errors = Vec::new();
        for stmt in body {
            if let Err(error) = self.check_stmt(stmt, env) {
                errors.extend(error.flatten());
            }
        }
        errors
    }
}
