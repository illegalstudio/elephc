//! Purpose:
//! Type-checks statements and updates block-local type environments.
//! Handles control flow, assignments, declarations, loops, branches, and statement-level diagnostics.
//!
//! Called from:
//! - `crate::types::checker::driver::top_level`
//! - `crate::types::checker::driver::functions`
//!
//! Key details:
//! - Statement environments must merge conservatively across branches, loops, throws, returns, and unreachable paths.

mod assignments;
mod control_flow;
mod narrowing;

use crate::errors::CompileError;
use crate::parser::ast::{ExprKind, Stmt, StmtKind};
use crate::types::TypeEnv;

use super::Checker;

/// Statement-level type checking for the Checker context.
impl Checker {
    /// Dispatches to assignment or control-flow checking based on statement kind.
    ///
    /// Synthetic, include-once-mark, and variant-mark statements are no-ops.
    /// Unresolved ifdef, namespace, use, and include statements produce errors.
    /// Echo and expression statements run inference with assignment effects.
    /// All class/function/enum/interface/trait/extern declarations are no-ops.
    ///
    /// # Errors
    /// Returns an error for unresolved conditionals, namespace/use directives,
    /// includes, or invalid break/continue levels.
    pub fn check_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> Result<(), CompileError> {
        // `declare(strict_types=1)` is scoped to the physical file the statement was written in,
        // and the checker only ever sees the merged program, so the file's answer travels on the
        // statement. Save/restore rather than assign: a strict file's `include` of a coercive one
        // must not leave the includer's setting behind, and vice versa.
        let outer_strict_types = self.strict_types;
        self.strict_types = stmt.strict_types;
        let result = crate::strict_php::with_source_mode(stmt.source_mode, || {
            self.check_stmt_in_current_source_mode(stmt, env)
        });
        self.strict_types = outer_strict_types;
        result
    }

    /// Checks one statement after its physical source profile has been installed.
    fn check_stmt_in_current_source_mode(
        &mut self,
        stmt: &Stmt,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Synthetic(stmts) => {
                if self.check_empty_indexed_nested_append(stmts, env)? {
                    return Ok(());
                }
                for stmt in stmts {
                    self.check_stmt(stmt, env)?;
                }
                Ok(())
            }
            StmtKind::IncludeOnceMark { .. } => Ok(()),
            StmtKind::FunctionVariantGroup { .. } => Ok(()),
            StmtKind::FunctionVariantMark { .. } => Ok(()),
            StmtKind::IncludeOnceGuard { body, .. } => {
                // The guard lowers to a real runtime branch (`Op::IncludeOnceGuard`), so its
                // body is conditional exactly like an `if` body.
                self.in_conditional_scope(env, |checker, env| {
                    for stmt in body {
                        checker.check_stmt(stmt, env)?;
                    }
                    Ok(())
                })
            }
            StmtKind::IfDef { .. } => {
                Err(CompileError::new(stmt.span, "Unresolved ifdef statement"))
            }
            StmtKind::NamespaceDecl { .. }
            | StmtKind::NamespaceBlock { .. }
            | StmtKind::UseDecl { .. } => Err(CompileError::new(
                stmt.span,
                "Unresolved namespace/use statement",
            )),
            StmtKind::Echo(expr) => {
                self.infer_type_with_assignment_effects(expr, env)?;
                Ok(())
            }
            StmtKind::Assign { .. }
            | StmtKind::RefAssign { .. }
            | StmtKind::ArrayAssign { .. }
            | StmtKind::NestedArrayAssign { .. }
            | StmtKind::ArrayPush { .. }
            | StmtKind::TypedAssign { .. }
            | StmtKind::ConstDecl { .. }
            | StmtKind::ListUnpack { .. }
            | StmtKind::Global { .. }
            | StmtKind::StaticVar { .. }
            | StmtKind::PropertyAssign { .. }
            | StmtKind::StaticPropertyAssign { .. }
            | StmtKind::StaticPropertyArrayPush { .. }
            | StmtKind::StaticPropertyArrayAssign { .. }
            | StmtKind::PropertyArrayPush { .. }
            | StmtKind::DynamicPropertyArrayPush { .. }
            | StmtKind::PropertyArrayAssign { .. } => self.check_assignment_like_stmt(stmt, env),
            StmtKind::Foreach { .. }
            | StmtKind::Switch { .. }
            | StmtKind::If { .. }
            | StmtKind::DoWhile { .. }
            | StmtKind::While { .. }
            | StmtKind::For { .. }
            | StmtKind::Throw(..)
            | StmtKind::Try { .. } => {
                self.in_conditional_scope(env, |checker, env| {
                    checker.check_control_flow_stmt(stmt, env)
                })
            }
            StmtKind::Include { .. } => {
                Err(CompileError::new(stmt.span, "Unresolved include statement"))
            }
            StmtKind::PackedClassDecl { .. } => Ok(()),
            StmtKind::Break(levels) => self.check_loop_exit(stmt.span, "break", *levels),
            StmtKind::Continue(levels) => self.check_loop_exit(stmt.span, "continue", *levels),
            StmtKind::ExprStmt(expr) => {
                // The one position an `unset(...)` may end a local binding from. elephc's parser
                // accepts `unset` as an expression (PHP's grammar does not), so the kill has to
                // be told where it is: from a `?:`/`??`/`&&` operand it would record decisions
                // for an operand that may never run, and its checker-side effects outlive the
                // discarded branch env. Saved and restored so a nested statement (a prelude
                // block, say) cannot leave this pointing at a finished node.
                let saved_statement_position = self
                    .statement_position_expr
                    .replace(expr as *const crate::parser::ast::Expr as usize);
                let result = self.infer_type_with_assignment_effects(expr, env);
                self.statement_position_expr = saved_statement_position;
                result?;
                Ok(())
            }
            StmtKind::FunctionDecl { .. } => Ok(()),
            StmtKind::Return(expr) => {
                if let Some(e) = expr {
                    let returned = self.infer_type_with_assignment_effects(e, env)?;
                    // Record the type as observed HERE, in flow order, so the later
                    // flow-insensitive return-coverage pass does not apply a narrowing that
                    // only holds further down the body to this return. See
                    // `Checker::flow_typed_returns`.
                    self.flow_typed_returns.insert(
                        stmt as *const Stmt as usize,
                        (stmt.span, returned),
                    );
                    // `function &f() { return $obj->prop; }` returns a reference to the
                    // property, so promote it to a reference property program-wide.
                    if self.current_by_ref_return {
                        if let ExprKind::PropertyAccess { object, property } = &e.kind {
                            let object_ty = self.infer_type(object, env)?;
                            if let Some(class) =
                                crate::types::checker::single_object_class_name(&object_ty)
                            {
                                self.reference_property_promotions
                                    .insert((class, property.clone()));
                            } else {
                                // A by-reference closure returning `$this->prop` whose `$this`
                                // is bound dynamically (`Closure::bind`) has a Mixed receiver, so
                                // the class is unknown here. Promote the property on every class
                                // that declares it so the reference resolves at runtime through
                                // class-id dispatch.
                                let owners: Vec<String> = self
                                    .classes
                                    .iter()
                                    .filter(|(_, info)| {
                                        info.properties.iter().any(|(n, _)| n == property)
                                    })
                                    .map(|(name, _)| name.clone())
                                    .collect();
                                for owner in owners {
                                    self.reference_property_promotions
                                        .insert((owner, property.clone()));
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
            StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. } => Ok(()),
            StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => Ok(()),
        }
    }

    /// Checks a statement group whose body may not run, keeping the local-binding eligibility
    /// state honest about it.
    ///
    /// `local_conditional_depth` is raised for the whole group (the loop/branch CONDITION included,
    /// which is conservative and accepted), so no `unset` or retype inside it is eligible — the
    /// checker shares one mutable `TypeEnv` across branches, so a kill in one arm would otherwise
    /// leak into its siblings and into the code after the join.
    ///
    /// Names the group INTRODUCED — `foreach` and `list()` targets, `catch` variables, builtin
    /// out-parameters, array auto-vivification — are deliberately NOT swept into
    /// `local_binding_depth` afterwards. The sweep this replaced cloned every key of `env` on entry
    /// and re-walked them on exit, roughly quadrupling the cost of type-checking a 400-local scope,
    /// and it bought nothing: a MISSING depth entry and a depth of 1 or more are the same answer to
    /// [`Checker::local_binding_is_killable`], which is the only killable-relevant reader, and the
    /// pre-scan's `contains_key` probe runs at body entry, before any group has been walked.
    fn in_conditional_scope<F>(&mut self, env: &mut TypeEnv, f: F) -> Result<(), CompileError>
    where
        F: FnOnce(&mut Self, &mut TypeEnv) -> Result<(), CompileError>,
    {
        self.local_conditional_depth += 1;
        let result = f(self, env);
        self.local_conditional_depth -= 1;
        result
    }

    /// Validates a `break` or `continue` statement against the current loop depth.
    ///
    /// `keyword` is either `"break"` or `"continue"`. `levels` is the number of
    /// enclosing loops to exit. Errors if `levels` exceeds the available loop
    /// nesting depth or if the jump would escape a `finally` block.
    fn check_loop_exit(
        &self,
        span: crate::span::Span,
        keyword: &str,
        levels: usize,
    ) -> Result<(), CompileError> {
        if levels <= self.break_continue_depth {
            if self.loop_exit_stays_inside_finally(levels) {
                Ok(())
            } else {
                Err(CompileError::new(
                    span,
                    "Cannot jump out of a finally block",
                ))
            }
        } else {
            Err(CompileError::new(
                span,
                &format!("Cannot '{}' {} levels", keyword, levels),
            ))
        }
    }

    /// Returns `true` if a loop exit of `levels` enclosing loops stays inside all
    /// `finally` blocks that enclose the target.
    ///
    /// If no `finally` block applies to the target depth, returns `true` (safe).
    /// Otherwise computes whether `levels` stays within the innermost `finally`
    /// block's range.
    fn loop_exit_stays_inside_finally(&self, levels: usize) -> bool {
        let Some(finally_base_depth) = self.finally_break_continue_bases.last() else {
            return true;
        };
        let local_target_depth = self.break_continue_depth - finally_base_depth;
        levels <= local_target_depth
    }
}
