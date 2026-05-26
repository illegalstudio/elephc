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

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};
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
        match &stmt.kind {
            StmtKind::Synthetic(stmts) => {
                for stmt in stmts {
                    self.check_stmt(stmt, env)?;
                }
                Ok(())
            }
            StmtKind::IncludeOnceMark { .. } => Ok(()),
            StmtKind::FunctionVariantGroup { .. } => Ok(()),
            StmtKind::FunctionVariantMark { .. } => Ok(()),
            StmtKind::IncludeOnceGuard { body, .. } => {
                for stmt in body {
                    self.check_stmt(stmt, env)?;
                }
                Ok(())
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
            | StmtKind::PropertyArrayAssign { .. } => self.check_assignment_like_stmt(stmt, env),
            StmtKind::Foreach { .. }
            | StmtKind::Switch { .. }
            | StmtKind::If { .. }
            | StmtKind::DoWhile { .. }
            | StmtKind::While { .. }
            | StmtKind::For { .. }
            | StmtKind::Throw(..)
            | StmtKind::Try { .. } => self.check_control_flow_stmt(stmt, env),
            StmtKind::Include { .. } => {
                Err(CompileError::new(stmt.span, "Unresolved include statement"))
            }
            StmtKind::PackedClassDecl { .. } => Ok(()),
            StmtKind::Break(levels) => self.check_loop_exit(stmt.span, "break", *levels),
            StmtKind::Continue(levels) => self.check_loop_exit(stmt.span, "continue", *levels),
            StmtKind::ExprStmt(expr) => {
                self.infer_type_with_assignment_effects(expr, env)?;
                Ok(())
            }
            StmtKind::FunctionDecl { .. } => Ok(()),
            StmtKind::Return(expr) => {
                if let Some(e) = expr {
                    self.infer_type_with_assignment_effects(e, env)?;
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
